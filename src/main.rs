mod app;
mod config;
mod reader;
mod route;
mod state;

use clap::{App, Arg};
use config::Config;

fn main() -> anyhow::Result<()> {
    let args = App::new("irclogger-viewer").arg(
        Arg::with_name("config_path")
            .required(true)
            .value_name("CONFIG")
            .help("Path to JSON config file."),
    );

    let matches = args.get_matches();
    let config_content = std::fs::read(matches.value_of("config_path").unwrap())?;
    let config: Config = serde_json::from_slice(&config_content)?;

    crate::app::run(config)?;

    Ok(())
}
