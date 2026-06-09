mod cli;
mod runner;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    runner::run(args)
}
