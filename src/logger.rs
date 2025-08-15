use std::fmt::Write as _;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::config::{LogFormat, LogLevel};

pub fn setup(
    log_level: LogLevel,
    log_format: LogFormat,
    bin_name: Option<&str>,
) {
    let log_level: LevelFilter = log_level.into();

    let with_color = supports_color::on(supports_color::Stream::Stderr)
        .filter(|s| s.has_basic)
        .is_some();

    let mut default_filter =
        format!("{}={log_level}", env!("CARGO_PKG_NAME").replace('-', "_"));
    if let Some(bin_name) = bin_name {
        let _ = write!(
            default_filter,
            ",{}={log_level}",
            bin_name.replace('-', "_")
        );
    }

    let filter = EnvFilter::builder().try_from_env().unwrap_or_else(|_| {
        EnvFilter::builder()
            .parse(default_filter)
            .expect("hardcoded filter should be correct")
    });

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(with_color);

    let _ = match log_format {
        LogFormat::Console => builder.try_init(),
        LogFormat::Json => builder.json().flatten_event(true).try_init(),
    };
}

#[cfg(test)]
mod tests {
    use crate::config::{LogFormat, LogLevel};

    use super::setup;

    #[test]
    fn setup_console_logger() {
        setup(LogLevel::Info, LogFormat::Console, Some("zvuk-dl"));
    }

    #[test]
    fn setup_json_logger() {
        setup(LogLevel::Info, LogFormat::Json, None);
    }
}
