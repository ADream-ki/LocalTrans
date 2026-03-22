use std::backtrace::Backtrace;
use std::path::PathBuf;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static LOG_GUARDS: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

pub fn default_log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("logs")
}

/// Initialize logging system (console + rolling file + panic hook).
pub fn init_logging() {
    init_logging_with_stderr(true);
}

/// Initialize logging system and optionally enable stderr logs.
pub fn init_logging_with_stderr(enable_stderr: bool) {
    let log_dir = default_log_dir();
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("failed to create log directory {}: {}", log_dir.display(), e);
    }

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,localtrans=debug,localtrans_lib=debug"));

    let rolling_file = tracing_appender::rolling::daily(&log_dir, "localtrans.log");
    let (file_writer, file_guard) = tracing_appender::non_blocking(rolling_file);

    let stderr_appender = std::io::stderr();
    let (stderr_writer, stderr_guard) = tracing_appender::non_blocking(stderr_appender);

    let guards = if enable_stderr {
        vec![file_guard, stderr_guard]
    } else {
        vec![file_guard]
    };
    let _ = LOG_GUARDS.set(guards);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(file_writer)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);
    if enable_stderr {
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(stderr_writer)
            .with_target(false)
            .with_thread_ids(true)
            .with_line_number(true);
        let _ = registry.with(stderr_layer).try_init();
    } else {
        let _ = registry.try_init();
    }

    install_panic_hook();

    tracing::info!(
        log_dir = %log_dir.display(),
        "logging initialized; daily log file prefix: localtrans.log"
    );
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "non-string panic payload"
        };

        tracing::error!(
            location = %location,
            panic = %payload,
            backtrace = %Backtrace::force_capture(),
            "unhandled panic detected"
        );
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_init() {
        // This should not panic
        init_logging();
    }
}
