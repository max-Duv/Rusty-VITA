mod analytics;
mod capture;
mod chaos;
mod cli;
mod dsp;
mod emit;
mod gui;
mod logging;
mod metrics;
mod profile;
mod scenario;
mod state;
mod telemetry;
mod vita49;
mod worker;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command};
use crate::profile::ResolvedConfig;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let resolved = ResolvedConfig::from_cli(&cli)?;

    match &cli.command {
        Command::Probe(args) => capture::run_probe(&resolved, args.seconds),
        _ if cli.headless => worker::run_headless(resolved),
        _ => gui::run(resolved),
    }
}
