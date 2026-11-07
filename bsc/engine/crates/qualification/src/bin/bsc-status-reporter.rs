//! Read-only local evidence publisher. Never reads secret environment files or sends chain RPC.
use darkbot_bsc_qualification::status::readiness_job::{self, Mark};
use darkbot_bsc_qualification::status::{census, gui_readiness, projection, publisher};
use prost::Message;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(std::env::var_os("DARKBOT_BSC_ROOT").ok_or("DARKBOT_BSC_ROOT missing")?);
    let run = PathBuf::from(std::env::var_os("DARKBOT_RUNDIR").ok_or("DARKBOT_RUNDIR missing")?);
    if std::env::args().nth(1).as_deref() == Some("--field-inventory") {
        println!(
            "{}",
            serde_json::to_string_pretty(&census::inventory(&root)?)?
        );
        return Ok(());
    }
    // Audit E 4.13: the readiness job's one native owner: the job itself (its launchd
    // program), the controller's marks and the job's state.
    if let Some(code) = readiness(&root, &run)? {
        std::process::exit(code);
    }
    // BSC-048: the typed provenance of a GUI readiness result (no JSON).
    if std::env::args().nth(1).as_deref() == Some("--seal-readiness-provenance") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        let [dir, launch_id, relative] = args.as_slice() else {
            return Err(
                "usage: --seal-readiness-provenance <dir> <launch_id> <relative_path>".into(),
            );
        };
        gui_readiness::seal(&root, std::path::Path::new(dir), launch_id, relative)?;
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--check-readiness-provenance") {
        let dir = std::env::args().nth(2).ok_or("missing directory")?;
        use darkbot_bsc_record::schema::g07::gui_readiness_provenance as p;
        let r = gui_readiness::check(std::path::Path::new(&dir))?;
        println!(
            "kind={}\nlaunch_id={}\nresult_sha256={}",
            r.text(p::KIND)?,
            r.text(p::LAUNCH_ID)?,
            darkbot_bsc_record::hex(&r.b256(p::RESULT_SHA256)?)
        );
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(&projection::project(&root, &run).status.encode_to_vec())?;
        return Ok(());
    }
    // Audit E 3.5: the controller that writes this reporter's plist (ops/bsc-control.sh) is the
    // one source of the socket path and the readiness job's label; no second default here.
    let socket =
        PathBuf::from(std::env::var_os("DARKBOT_BSC_SOCKET").ok_or("DARKBOT_BSC_SOCKET missing")?);
    let label = std::env::var("DARKBOT_BSC_READINESS_LABEL")
        .map_err(|_| "DARKBOT_BSC_READINESS_LABEL missing")?;
    let readiness_bin = std::env::var_os("DARKBOT_BSC_READINESS_BIN").map(PathBuf::from);
    publisher::serve(&root, &run, &socket, &label, readiness_bin.as_deref())?;
    Ok(())
}

/// The readiness job's commands; none when the arguments are none of them.
fn readiness(
    root: &std::path::Path,
    run: &std::path::Path,
) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let readiness_bin = std::env::var_os("DARKBOT_BSC_READINESS_BIN").map(PathBuf::from);
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--readiness-job"] => {
            let launch_id = std::env::var("DARKBOT_BSC_LAUNCH_ID")
                .map_err(|_| "DARKBOT_BSC_LAUNCH_ID missing")?;
            let readiness_bin = readiness_bin.ok_or("DARKBOT_BSC_READINESS_BIN missing")?;
            Ok(Some(readiness_job::run(&readiness_job::Job {
                root,
                rundir: run,
                launch_id: &launch_id,
                readiness_bin: &readiness_bin,
            })))
        }
        ["--readiness-mark", kind, launch_id] => {
            let mark = Mark::of_controller(kind)
                .ok_or("usage: --readiness-mark launch|stopping <launch_id>")?;
            readiness_job::write(run, mark, launch_id, readiness_job::now_s(), None)?;
            Ok(Some(0))
        }
        // Exit 0 once the job of `launch_id` armed its stop handler.
        ["--readiness-armed", launch_id] => Ok(Some(i32::from(
            readiness_job::read(run, Mark::Armed).is_none_or(|m| m.launch_id != *launch_id),
        ))),
        ["--readiness-launch"] => {
            println!(
                "{}",
                readiness_job::read(run, Mark::Launch)
                    .unwrap_or_default()
                    .launch_id
            );
            Ok(Some(0))
        }
        ["--readiness-result"] => {
            println!(
                "{}",
                readiness_job::read(run, Mark::Result)
                    .unwrap_or_default()
                    .result_code
            );
            Ok(Some(0))
        }
        // The job's state as its typed status (protobuf), for the gateway.
        ["--readiness-status", label] => {
            use std::io::Write;
            let available = readiness_job::controller_available(root, readiness_bin.as_deref());
            std::io::stdout()
                .lock()
                .write_all(&readiness_job::status(run, label, available).encode_to_vec())?;
            Ok(Some(0))
        }
        // BSC-V8: one bounded read-only query (`POST /api/v1/bsc/query`): the
        // request on stdin, the answer on stdout; never a write.
        ["--query"] => {
            use std::io::{Read, Write};
            let mut request = Vec::new();
            std::io::stdin().lock().read_to_end(&mut request)?;
            let answer = darkbot_bsc_qualification::status::query::answer(&request, root, run);
            std::io::stdout().lock().write_all(&answer)?;
            Ok(Some(0))
        }
        // A malformed readiness command fails; it never starts the socket server.
        [first, ..] if first.starts_with("--readiness-") => {
            Err(format!("usage: {first}: see bsc-control.sh").into())
        }
        _ => Ok(None),
    }
}
