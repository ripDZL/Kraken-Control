//! Diagnostics log: `%APPDATA%\KrakenControl\log.txt`, rotated at 1 MiB to
//! `log.old.txt`. Events are always logged; packet hex only when enabled.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1 << 20;

static PACKETS: AtomicBool = AtomicBool::new(false);
static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn dir() -> PathBuf {
    crate::store::Store::path().parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn path() -> PathBuf {
    dir().join("log.txt")
}

pub fn set_packet_logging(on: bool) {
    PACKETS.store(on, Ordering::Relaxed);
}

pub fn packet_logging() -> bool {
    PACKETS.load(Ordering::Relaxed)
}

pub fn line(msg: impl AsRef<str>) {
    let Ok(mut guard) = FILE.lock() else { return };
    if guard.is_none() {
        *guard = open();
    }
    let Some(f) = guard.as_mut() else { return };
    let _ = writeln!(f, "{} {}", timestamp(), msg.as_ref());
    if f.metadata().map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        *guard = None;
        let _ = fs::rename(path(), dir().join("log.old.txt"));
    }
}

/// Logs a packet if packet logging is on. `dir` is "->" (sent) or "<-" (received).
pub fn packet(dir: &str, bytes: &[u8]) {
    if !packet_logging() {
        return;
    }
    let mut s = String::with_capacity(4 + bytes.len() * 3);
    s.push_str(dir);
    for b in bytes {
        s.push_str(&format!(" {b:02x}"));
    }
    line(s);
}

fn open() -> Option<File> {
    fs::create_dir_all(dir()).ok()?;
    OpenOptions::new().create(true).append(true).open(path()).ok()
}

/// UTC `YYYY-MM-DD HH:MM:SS.mmm`.
pub(crate) fn timestamp() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60,
        d.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn timestamp_shape() {
        let t = super::timestamp();
        assert_eq!(t.len(), 24, "{t}");
        assert!(t.ends_with('Z'));
    }
}
