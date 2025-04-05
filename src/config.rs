use std::time::Duration;

use anyhow::anyhow;
use clap::ArgAction;
use clap::Parser;
use clap::ValueEnum;
use serde::Serialize;
use serde::Serializer;
use serde_json::to_value;
use tracing::level_filters::LevelFilter;

use crate::zvuk::Quality;
use crate::zvuk::ZVUK_DEFAULT_COVER_RESIZE_COMMAND;
use crate::zvuk::ZVUK_USER_AGENT;

/// Download albums and tracks in high quality (FLAC) from Zvuk.com
#[derive(Debug, Parser, Serialize)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    #[allow(clippy::doc_markdown)]
    /// URLs of releases or tracks
    ///
    /// URLs must look like https://zvuk.com/track/128672726 or https://zvuk.com/release/29970563
    #[arg(required = true, num_args = 1..)]
    pub urls: Vec<String>,

    /// Zvuk Token
    #[serde(serialize_with = "mask")]
    #[arg(long, env, hide_env_values = true)]
    pub token: String,

    /// Output directory
    #[arg(long, short, env, default_value_t = String::from("."))]
    pub output_dir: String,

    /// Quality of tracks to grab
    #[arg(long, short, env, value_enum, default_value_t = Quality::Flac)]
    pub quality: Quality,

    /// Embed album cover into tracks
    #[arg(
        long,
        env,
        action = ArgAction::Set,
        default_value_t = false,
        default_missing_value = "true",
        require_equals = true,
        num_args=0..=1,
    )]
    pub embed_cover: bool,

    /// Resize album cover
    #[arg(
        long,
        env,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        require_equals = true,
        num_args=0..=1,
    )]
    pub resize_cover: bool,

    /// Resize if cover size in bytes bigger than this value
    #[arg(long, env, default_value_t = 2 * 1000 * 1000)]
    pub resize_cover_limit: u64,

    /// Download and embed lyrics
    #[arg(
        long,
        env,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        require_equals = true,
        num_args=0..=1,
    )]
    pub download_lyrics: bool,

    /// Resize cover command.
    /// By default uses imagemagick
    #[arg(
        long,
        env,
        value_parser = resize_command_validator,
        default_value_t = ZVUK_DEFAULT_COVER_RESIZE_COMMAND.to_string(),
    )]
    pub resize_command: String,

    /// User Agent
    #[arg(
        long,
        env,
        default_value_t = ZVUK_USER_AGENT.to_string(),
    )]
    pub user_agent: String,

    /// How long to wait between getting track links
    #[arg(
        long,
        env,
        hide = true,
        default_value = "1s",
        value_parser = humantime::parse_duration,
    )]
    pub pause_between_getting_track_links: Duration,

    /// Verbosity of logging
    #[arg(long, value_enum, env, default_value_t = LogLevel::Debug)]
    pub log_level: LogLevel,

    /// Format of logs
    #[arg(long, value_enum, env, default_value_t = LogFormat::Console)]
    pub log_format: LogFormat,
}

#[derive(ValueEnum, Debug, Clone, Copy, Serialize)]
pub enum LogFormat {
    /// Pretty logs for debugging
    Console,
    /// JSON logs
    Json,
}

#[derive(ValueEnum, Debug, Clone, Copy, Serialize)]
pub enum LogLevel {
    Off,
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Off => Self::OFF,
            LogLevel::Trace => Self::TRACE,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Info => Self::INFO,
            LogLevel::Warn => Self::WARN,
            LogLevel::Error => Self::ERROR,
        }
    }
}

pub fn mask<S, T>(_: &T, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    static MASK: &str = "******";
    s.serialize_str(MASK)
}

pub trait LogStruct {
    fn log(&self);
}

impl<T> LogStruct for T
where
    T: Serialize,
{
    fn log(&self) {
        if let Ok(json_obj) = to_value(self) {
            if let Ok(json_obj) =
                json_obj.as_object().ok_or_else(|| anyhow!("WTF"))
            {
                for (key, value) in json_obj {
                    tracing::debug!("Config {}={}", key, value);
                }
            }
        }
    }
}

fn resize_command_validator(value: &str) -> anyhow::Result<String> {
    if value.contains("{source}") && value.contains("{target}") {
        return Ok(String::from(value));
    }
    Err(anyhow!(
        "command is required to have {{source}} and {{target}} placeholders"
    ))
}

#[cfg(test)]
mod tests {
    use super::resize_command_validator;
    use super::Config;

    #[test]
    fn validate_resize_command() {
        let successes = &["cmd {source} {target}"];
        let fails = &["cmd {target}", "cmd {source}", "cmd", ""];

        for case in successes {
            assert!(resize_command_validator(case).is_ok());
        }

        for case in fails {
            assert!(resize_command_validator(case).is_err());
        }
    }

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Config::command().debug_assert();
    }
}
