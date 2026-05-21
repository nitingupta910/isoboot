mod cli;
mod disk;
mod grub;
mod install;
mod runner;
mod tools;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::List => disk::list_candidates(),
        cli::Command::Install(args) => install::install(&args),
        cli::Command::UpdateMenu(args) => install::update_menu(&args),
        cli::Command::Verify(args) => install::verify(&args),
    }
}
