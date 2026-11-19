use std::path::Path;

use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{InitError, RollingFileAppender, Rotation},
};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const LOG_FILE_PREFIX: &str = "raindex-api.log";
const MAX_LOG_FILES: usize = 14;
const DEFAULT_ENV_FILTER: &str = "raindex_rest_api=info,raindex_common=info,rocket=warn,warn";

fn file_appender(log_dir: &Path) -> Result<RollingFileAppender, InitError> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)
}

pub struct LoggingGuard {
    _file_guard: WorkerGuard,
}

pub fn init(log_dir: &Path) -> Result<LoggingGuard, Box<dyn std::error::Error + Send + Sync>> {
    let appender = file_appender(log_dir)?;
    let (file_writer, file_guard) = tracing_appender::non_blocking(appender);
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_ENV_FILTER));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().json().with_current_span(false))
        .with(
            fmt::layer()
                .json()
                .with_current_span(false)
                .with_writer(file_writer),
        )
        .try_init()?;

    Ok(LoggingGuard {
        _file_guard: file_guard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, fs::File};

    #[test]
    fn file_appender_retains_fourteen_daily_logs() {
        let directory = tempfile::tempdir().expect("temporary log directory");
        for day in 1..=15 {
            File::create(
                directory
                    .path()
                    .join(format!("{LOG_FILE_PREFIX}.2000-01-{day:02}")),
            )
            .expect("seed daily log");
        }
        let unrelated = directory.path().join("unrelated.log");
        File::create(&unrelated).expect("seed unrelated file");

        drop(file_appender(directory.path()).expect("build file appender"));

        let retained = fs::read_dir(directory.path())
            .expect("read log directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(LOG_FILE_PREFIX)
            })
            .count();
        assert_eq!(retained, MAX_LOG_FILES);
        assert!(unrelated.exists());
    }
}
