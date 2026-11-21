//! Log files with secrets filtered out (spec §6f.1).
pub mod scrub;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Days of log files kept.
pub const KEEP_DAYS: u32 = 7;

/// `%APPDATA%\FishyClient\logs`.
pub fn default_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("FishyClient").join("logs")
}

/// Deletes `fishy-client.YYYY-MM-DD.log` files older than `keep_days`. Returns how many went.
pub fn prune(dir: &Path, today: chrono::NaiveDate, keep_days: u32) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(date) = name.strip_prefix("fishy-client.").and_then(|r| r.strip_suffix(".log")) else { continue };
        let Ok(date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") else { continue };
        if (today - date).num_days() >= i64::from(keep_days) && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Passes everything through [`scrub::scrub`] before writing it on.
pub struct ScrubWriter<W: Write>(pub W);

impl<W: Write> Write for ScrubWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.0.write_all(scrub::scrub(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn appender(dir: &Path) -> io::Result<tracing_appender::rolling::RollingFileAppender> {
    tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("fishy-client")
        .filename_suffix("log")
        .build(dir)
        .map_err(io::Error::other)
}

/// The scrubbed daily writer without installing it globally (for tests).
pub fn make_writer(dir: &Path) -> io::Result<Mutex<ScrubWriter<tracing_appender::rolling::RollingFileAppender>>> {
    Ok(Mutex::new(ScrubWriter(appender(dir)?)))
}

/// Keep this alive for the app's lifetime; dropping it flushes the log.
pub struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

/// Creates the folder, prunes old files and sends Info-and-up logs to today's file.
pub fn init(dir: &Path) -> io::Result<LogGuard> {
    std::fs::create_dir_all(dir)?;
    prune(dir, chrono::Utc::now().date_naive(), KEEP_DAYS); // file names use UTC dates too
    let (writer, guard) = tracing_appender::non_blocking(ScrubWriter(appender(dir)?));
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(writer)
        .try_init()
        .map_err(io::Error::other)?;
    Ok(LogGuard(guard))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn log_name(d: &str) -> String {
        format!("fishy-client.{d}.log")
    }

    #[test]
    fn prune_removes_only_old_log_files() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["2026-11-01", "2026-11-14", "2026-11-15", "2026-11-21"] {
            std::fs::write(dir.path().join(log_name(d)), b"x").unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        assert_eq!(prune(dir.path(), today, 7), 2);
        assert!(!dir.path().join(log_name("2026-11-01")).exists());
        assert!(!dir.path().join(log_name("2026-11-14")).exists());
        assert!(dir.path().join(log_name("2026-11-15")).exists());
        assert!(dir.path().join(log_name("2026-11-21")).exists());
        assert!(dir.path().join("notes.txt").exists());
    }

    #[test]
    fn a_logged_token_is_written_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let sub = tracing_subscriber::fmt().with_ansi(false).with_writer(make_writer(dir.path()).unwrap()).finish();
        tracing::subscriber::with_default(sub, || {
            tracing::info!("refreshing with access_token=SECRET123");
        });
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(files.len(), 1, "{files:?}");
        let name = files[0].file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("fishy-client.") && name.ends_with(".log"), "{name}");
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("access_token=[redacted]"), "{text}");
        assert!(!text.contains("SECRET123"), "{text}");
    }

    #[test]
    fn init_fails_cleanly_when_the_folder_cant_be_made() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a-file");
        std::fs::write(&file, b"x").unwrap();
        assert!(init(&file.join("logs")).is_err());
    }
}
