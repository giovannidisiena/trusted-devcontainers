mod cli;
mod github;
mod model;
mod payload;
mod process;
mod workflow;

use clap::Parser;

fn main() {
    let mut args = std::env::args();
    let _bin = args.next();
    if args.next().as_deref() == Some("__complete") {
        if let Err(err) = workflow::complete(args) {
            eprintln!("error: {err:#}");
            std::process::exit(1);
        }
        return;
    }

    let cli = cli::Cli::parse();
    if let Err(err) = workflow::run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
