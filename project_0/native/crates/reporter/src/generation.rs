//! Monotone publisher epoch. A process-level exclusive lock is mandatory before advancing.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
pub fn next(path: &Path) -> Result<u64, String> {
    let current = match fs::read_to_string(path) {
        Ok(s) => s
            .trim()
            .parse::<u64>()
            .map_err(|_| "GENERATION_FILE_CORRUPT".to_owned())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => return Err(format!("generation read: {e}")),
    };
    let generation = current.checked_add(1).ok_or("GENERATION_OVERFLOW")?;
    let temp = path.with_extension("next");
    let result = (|| -> Result<(), String> {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("generation temp: {e}"))?;
        writeln!(out, "{generation}").map_err(|e| e.to_string())?;
        out.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            fs::File::open(dir)
                .and_then(|d| d.sync_all())
                .map_err(|e| format!("generation directory sync: {e}"))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map(|()| generation)
}
