mod config;
mod logger;
mod zvuk;

use clap::Parser;
use dotenvy::dotenv;

use config::Config;
use config::LogStruct;

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let config = Config::parse();

    logger::setup(
        config.log_level,
        config.log_format,
        option_env!("CARGO_BIN_NAME"),
    );

    config.log();

    zvuk::download(&config)?;

    Ok(())
}
