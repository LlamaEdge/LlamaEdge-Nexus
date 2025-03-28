use crate::error::{ServerError, ServerResult};
use once_cell::sync::OnceCell;
// use serde::{Deserialize, Serialize};
use tracing::Level;

// Global log configuration
pub(crate) static LOG_DESTINATION: OnceCell<String> = OnceCell::new();

/// Initialize logging based on the specified destination
pub fn init_logging(destination: &str, file_path: Option<&str>) -> ServerResult<()> {
    // Store the log destination for later use
    LOG_DESTINATION.set(destination.to_string()).map_err(|_| {
        let err_msg = "Failed to set log destination".to_string();
        eprintln!("{}", err_msg);
        ServerError::Operation(err_msg)
    })?;

    let log_level = get_log_level_from_env();

    match destination {
        "stdout" => {
            // Terminal output preserves colors
            tracing_subscriber::fmt()
                .with_target(false)
                .with_level(true)
                .with_file(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_max_level(log_level)
                .init();
            Ok(())
        }
        "file" => {
            if let Some(path) = file_path {
                let file = std::fs::File::create(path).map_err(|e| {
                    let err_msg = format!("Failed to create log file: {}", e);
                    eprintln!("{}", err_msg);
                    ServerError::Operation(err_msg)
                })?;

                // File output disables ANSI colors
                tracing_subscriber::fmt()
                    .with_target(false)
                    .with_level(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_max_level(log_level)
                    .with_writer(file)
                    .with_ansi(false) // Disable ANSI colors
                    .init();
                Ok(())
            } else {
                Err(ServerError::Operation("Missing log file path".to_string()))
            }
        }
        "both" => {
            if let Some(path) = file_path {
                // Create directory if it doesn't exist
                if let Some(parent) = std::path::Path::new(path).parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            let err_msg = format!("Failed to create directory for log file: {}", e);
                            eprintln!("{}", err_msg);
                            ServerError::Operation(err_msg)
                        })?;
                    }
                }

                // Create file appender and disable colors
                let file_appender = tracing_appender::rolling::never(
                    std::path::Path::new(path)
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(".")),
                    std::path::Path::new(path).file_name().unwrap_or_default(),
                );
                let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

                // Configure subscriber, disable ANSI colors
                tracing_subscriber::fmt()
                    .with_target(false)
                    .with_level(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_max_level(log_level)
                    .with_writer(non_blocking)
                    .with_ansi(false) // Disable ANSI colors
                    .init();

                println!("Logging to both stdout and file: {}", path);

                Ok(())
            } else {
                Err(ServerError::Operation("Missing log file path".to_string()))
            }
        }
        _ => {
            let err_msg = format!(
                "Invalid log destination: {}. Valid values are 'stdout', 'file', or 'both'",
                destination
            );
            eprintln!("{}", err_msg);
            Err(ServerError::Operation(err_msg))
        }
    }
}

fn get_log_level_from_env() -> Level {
    match std::env::var("LLAMA_LOG").ok().as_deref() {
        Some("trace") => Level::TRACE,
        Some("debug") => Level::DEBUG,
        Some("info") => Level::INFO,
        Some("warn") => Level::WARN,
        Some("error") => Level::ERROR,
        _ => Level::INFO,
    }
}

// Helper macro for dual logging (to both stdout and log file)
#[macro_export]
macro_rules! dual_log {
    ($level:expr, $($arg:tt)+) => {{
        let msg = format!($($arg)+);
        if $crate::utils::LOG_DESTINATION.get().map_or(false, |d| d == "both") {
            println!("{}: {}", $level, msg);
        }
        match $level {
            "INFO" => tracing::info!("{}", msg),
            "WARN" => tracing::warn!("{}", msg),
            "ERROR" => tracing::error!("{}", msg),
            "DEBUG" => tracing::debug!("{}", msg),
            _ => tracing::trace!("{}", msg),
        }
    }};
}

// Convenience macros for each log level
#[macro_export]
macro_rules! dual_info {
    ($($arg:tt)+) => { $crate::dual_log!("INFO", $($arg)+) };
}

#[macro_export]
macro_rules! dual_warn {
    ($($arg:tt)+) => { $crate::dual_log!("WARN", $($arg)+) };
}

#[macro_export]
macro_rules! dual_error {
    ($($arg:tt)+) => { $crate::dual_log!("ERROR", $($arg)+) };
}

#[macro_export]
macro_rules! dual_debug {
    ($($arg:tt)+) => { $crate::dual_log!("DEBUG", $($arg)+) };
}
