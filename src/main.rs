mod cli;
mod github;
mod model;
mod payload;
mod process;
mod workflow;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(err) = workflow::run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
