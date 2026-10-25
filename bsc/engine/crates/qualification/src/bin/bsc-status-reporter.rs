//! Read-only local evidence publisher. Never reads secret environment files or sends chain RPC.
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
    if std::env::args().nth(1).as_deref() == Some("--validate-readiness") {
        let p = std::env::args().nth(2).ok_or("missing path")?;
        let bytes = std::fs::read(p)?;
        use darkbot_bsc_record::schema::g07::readiness;
        let decoded = darkbot_bsc_record::schema::decode_any(&bytes)?;
        if decoded.schema() != &readiness::SCHEMA {
            return Err("not a readiness record".into());
        }
        // The typed native verdict and mode, for the job wrapper's result.
        let status = decoded.text(readiness::STATUS)?;
        let mode = decoded.text(readiness::MODE)?;
        if !["PASS_READ_ONLY", "BLOCKED", "STOP"].contains(&status)
            || !["READ_ONLY_RPC", "OFFLINE_ZERO_EGRESS"].contains(&mode)
        {
            return Err("unknown readiness status or mode".into());
        }
        println!("status={status}\nmode={mode}");
        return Ok(());
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
    let socket = PathBuf::from(
        std::env::var_os("DARKBOT_BSC_SOCKET").unwrap_or_else(|| "/tmp/darkbot/bsc.sock".into()),
    );
    let label = std::env::var("DARKBOT_BSC_READINESS_LABEL")
        .unwrap_or_else(|_| "com.darkbot.bsc.readiness".into());
    let readiness_bin = std::env::var_os("DARKBOT_BSC_READINESS_BIN").map(PathBuf::from);
    publisher::serve(&root, &run, &socket, &label, readiness_bin.as_deref())?;
    Ok(())
}
