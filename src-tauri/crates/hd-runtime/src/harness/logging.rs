//! Durable, scrubbed harness logs.
//!
//! The live console holds a bounded ring for the UI; this is the file a user
//! attaches to a bug report three days later. One file per launch, named after
//! the day and the process, so restarts never append into a file a reader is
//! still tailing.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hd_core::paths;

/// Today's UTC date as `YYYY-MM-DD`, for the file name.
fn day_stamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Days since epoch → civil date (Howard Hinnant's algorithm), then Y-M-D.
    let days = seconds / 86_400;
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// A per-launch log file the supervisor streams into.
pub struct PersistentLog {
    file: Option<(PathBuf, std::fs::File)>,
}

impl PersistentLog {
    /// Open today's log under the application logs directory.
    pub fn managed() -> Self {
        let dir = paths::logs_dir();
        Self::under(&dir)
    }

    pub fn under(dir: &Path) -> Self {
        let _ = std::fs::create_dir_all(dir);
        let path = dir.join(format!(
            "harnesslite-{}-{}.log",
            day_stamp(),
            std::process::id()
        ));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
            .map(|file| (path, file));
        Self { file }
    }

    /// Where this launch's log lives, when it could be opened.
    pub fn path(&self) -> Option<PathBuf> {
        self.file.as_ref().map(|(path, _)| path.clone())
    }

    /// Append one tagged line. A log that cannot be written is never allowed
    /// to break the thing it is logging.
    pub fn write(&mut self, stream: &str, line: &str) {
        let Some((_, file)) = self.file.as_mut() else {
            return;
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "[{stamp}] [{stream}] {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_opens_records_and_reports_its_path() {
        let root = std::env::temp_dir().join(format!(
            "harnesslite-logging-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("logs");

        let mut log = PersistentLog::under(&dir);
        log.write("out", "dsh web: http://127.0.0.1:52175");
        log.write("err", "something broke");

        let path = log.path().expect("path");
        let body = std::fs::read_to_string(&path).expect("body");
        assert!(body.contains("[out] dsh web: http://127.0.0.1:52175"));
        assert!(body.contains("[err] something broke"));
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("harnesslite-"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn day_stamp_is_a_valid_date() {
        let stamp = day_stamp();
        assert_eq!(stamp.len(), 10);
        assert_eq!(stamp.as_bytes()[4], b'-');
        assert_eq!(stamp.as_bytes()[7], b'-');
    }
}
