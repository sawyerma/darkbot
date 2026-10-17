//! Golden renders of report documents (test only, JFI-05 parity): a document is compared with
//! its recorded render after the run-specific values (wall clock, process-global telemetry,
//! run id) are masked. `P0_RECORD_GOLDEN=1` records the current render instead.
use serde_json::Value;
use std::path::PathBuf;

fn mask(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if keys.contains(&k.as_str()) {
                    *x = Value::String("MASKED".into());
                } else {
                    mask(x, keys);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| mask(x, keys)),
        _ => {}
    }
}

/// Asserts that `value` (masked) equals `testdata/<name>.json`.
pub fn check(name: &str, value: &Value, masked: &[&str]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/golden")
        .join(format!("{name}.json"));
    let mut v = value.clone();
    mask(&mut v, masked);
    let text = serde_json::to_string_pretty(&v).unwrap() + "\n";
    if std::env::var_os("P0_RECORD_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let expected =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("golden {}: {e}", path.display()));
    if expected != text {
        let actual = std::env::temp_dir().join(format!("{name}.actual.json"));
        std::fs::write(&actual, &text).unwrap();
        panic!(
            "golden {name} differs: expected {} actual {}",
            path.display(),
            actual.display()
        );
    }
}
