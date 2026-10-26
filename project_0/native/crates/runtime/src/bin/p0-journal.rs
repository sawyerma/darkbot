//! Offline inspection of the one binary journal contract (PERSIST-01/04). Read only, text output
//! (JFI-01/03, P0-PROD-CTRL-02: no JSON or JSONL is written or printed):
//! `verify` checks magic, version, every CRC, the SHA256 chain and the manifest;
//! `frames JOURNAL` prints every frame (kind, sequence, provenance, typed value) as text;
//! `import KIND JOURNAL` writes typed `key=value` rows from stdin into a new journal (census kinds
//! only);
//! `import-provider-evidence JSON JOURNAL` turns a measured provider evidence JSON into the typed
//! `PROVIDER_EVIDENCE` record the network boot audit reads (X2);
//! `renders RUN` checks every existing historical view of a run byte for byte against its
//! journal (read only; a missing view stays MISSING, nothing is rendered into the run; DEC-09:
//! only in the `historical` build).
use darkbot_runner::analytics::journal::{self, Reader, MANIFEST};
#[cfg(feature = "historical")]
use darkbot_runner::analytics::renders;
use std::{collections::BTreeMap, path::Path};
fn verify(path: &Path) -> Result<String, String> {
    // SK-07: the chain is verified first (links, successors, orphans); then every segment's
    // frames are counted. A single-segment journal is a chain of one.
    let chain = darkbot_runner::storage::segments::verify_chain(path)?;
    let mut kinds = BTreeMap::<u16, u64>::new();
    let (mut frames, mut root) = (0u64, [0u8; 32]);
    for index in 1..=chain.segments {
        let segment = darkbot_runner::storage::segments::segment_path(path, index);
        let mut reader = Reader::open(&segment).map_err(|e| e.to_string())?;
        while let Some(frame) = reader.next_frame().map_err(|e| e.to_string())? {
            *kinds.entry(frame.kind).or_default() += 1;
            if frame.kind != MANIFEST && !journal::kind::ALL.contains(&frame.kind) {
                return Err("EVENT_KIND_UNKNOWN".into());
            }
        }
        frames += reader.frames();
        root = reader.root();
    }
    let status = if !chain.sealed {
        "OPEN"
    } else if chain.complete {
        "COMPLETE"
    } else {
        "INCOMPLETE"
    };
    let kinds: Vec<String> = kinds.iter().map(|(k, n)| format!("{k}:{n}")).collect();
    Ok(format!(
        "JOURNAL status={status} frames={frames} segments={} sealed={} root_sha256={} kinds={}",
        chain.segments,
        chain.sealed,
        journal::hex(&root),
        kinds.join(",")
    ))
}
/// Explicit offline import of one row stream (the C0 census). Only kinds listed as importable
/// are accepted, so runtime or financial journals can never be forged here. JFI: each stdin line
/// is decoded once into the kind's typed row (`key=value` text, no JSON); a malformed row
/// fails the import and seals the journal INCOMPLETE.
fn import(kind: u16, path: &Path) -> Result<(), String> {
    use darkbot_runner::analytics::payload::{C0CensusImportAccount, C0CensusImportGroupAccount};
    use std::io::BufRead;
    if !journal::kind::IMPORTABLE.contains(&kind) {
        return Err("KIND_NOT_IMPORTABLE".into());
    }
    let mut writer = journal::Writer::create(path).map_err(|e| e.to_string())?;
    let mut rows = 0u64;
    let appended = (|| -> Result<(), String> {
        for line in std::io::stdin().lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.is_empty() {
                continue;
            }
            let provenance = |slot| journal::Provenance {
                source: journal::source::C0_CENSUS,
                source_seq: rows,
                slot: Some(slot),
                ..Default::default()
            };
            match kind {
                journal::kind::C0_CENSUS_ACCOUNT => {
                    let row = C0CensusImportAccount::from_import_line(&line)?;
                    writer.append_typed(provenance(row.slot), &row)
                }
                _ => {
                    let row = C0CensusImportGroupAccount::from_import_line(&line)?;
                    writer.append_typed(provenance(row.slot), &row)
                }
            }
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
        Ok(())
    })();
    let durable = writer.seal(appended.is_ok()).map_err(|e| e.to_string())?;
    appended?;
    println!(
        "C0_CENSUS_IMPORT journal={} frames={rows} root_sha256={} status=COMPLETE",
        path.display(),
        journal::hex(&durable.hash)
    );
    Ok(())
}
/// Explicit offline import of measured provider evidence (X2): the JSON is decoded once into the
/// typed record; the boot audit reads only that record and binds its bytes.
fn import_provider_evidence(json: &Path, out: &Path) -> Result<(), String> {
    use darkbot_runner::orchestration::provider_evidence::ProviderEvidence;
    if std::fs::metadata(json).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("PROVIDER_EVIDENCE_LIMIT".into());
    }
    let evidence =
        ProviderEvidence::from_measured_json(&std::fs::read(json).map_err(|e| e.to_string())?)?;
    let mut writer = journal::Writer::create(out).map_err(|e| e.to_string())?;
    writer
        .append_typed(journal::Provenance::default(), &evidence)
        .map_err(|e| e.to_string())?;
    let durable = writer.seal(true).map_err(|e| e.to_string())?;
    println!(
        "PROVIDER_EVIDENCE {} providers={} source_sha256={} root_sha256={}",
        out.display(),
        evidence.providers.len(),
        evidence.source_sha256,
        journal::hex(&durable.hash)
    );
    Ok(())
}
fn frames(path: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut reader =
        darkbot_runner::storage::segments::ChainReader::open(path).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    while let Some(f) = reader.next_frame().map_err(|e| e.to_string())? {
        let p = &f.provenance;
        // Typed frames as text; historical value-tree frames as their stored value (DEC-09: the
        // JSON text only in the `historical` build, else the native tree).
        let payload = if f.schema == 1 {
            #[cfg(feature = "historical")]
            let text = f.legacy_value().map(|v| v.to_string());
            #[cfg(not(feature = "historical"))]
            let text = f.legacy_tree().map(|v| format!("{v:?}"));
            text
        } else {
            darkbot_runner::analytics::payload::describe(f.kind, f.schema, &f.payload)
        }
        .map_err(|e| e.to_string())?;
        writeln!(
            out,
            "seq={} kind={} schema={} source={} source_seq={} slot={:?} rx_ns={:?} unix_ns={:?} payload={payload}",
            f.journal_seq, f.kind, f.schema, p.source, p.source_seq, p.slot, p.rx_ns, p.unix_ns
        )
        .map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())
}
#[cfg(feature = "historical")]
fn views(run: &Path) -> Result<(), String> {
    let paths = renders::detect(run)?;
    for (path, status) in renders::check(&paths)? {
        println!(
            "VIEW {} {status}",
            path.strip_prefix(run).unwrap_or(&path).display()
        );
    }
    Ok(())
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["verify", path] => verify(Path::new(path)).map(|v| println!("{v}")),
        ["frames", path] => frames(Path::new(path)),
        ["import", kind, path] => kind
            .parse()
            .map_err(|_| "KIND_INVALID".to_string())
            .and_then(|k| import(k, Path::new(path))),
        ["import-provider-evidence", json, out] => {
            import_provider_evidence(Path::new(json), Path::new(out))
        }
        #[cfg(feature = "historical")]
        ["renders", run] => views(Path::new(run)),
        #[cfg(not(feature = "historical"))]
        ["renders", _] => Err(darkbot_runner::analytics::renders::HISTORICAL_READER_NOT_BUILT.into()),
        _ => Err(
            "usage: p0-journal verify JOURNAL | p0-journal frames JOURNAL | \
                  p0-journal import KIND JOURNAL | p0-journal import-provider-evidence JSON JOURNAL | \
                  p0-journal renders RUN"
                .into(),
        ),
    };
    if let Err(error) = result {
        eprintln!("JOURNAL_INVALID: {error}");
        std::process::exit(2);
    }
}
