use clap::Parser;

use sentinel::cli::Cli;

fn main() {
    let cli = Cli::parse();
    let config = match sentinel::config::Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("sentinel: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = sentinel::commands::run(cli.command, config) {
        eprintln!("sentinel: {error}");
        std::process::exit(1);
    }
}
