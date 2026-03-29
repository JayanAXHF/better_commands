mod file_buffer;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use std::ffi::OsString;
use std::process::{Command, Stdio};

use crate::tui::App;

#[derive(Debug, Parser)]
#[command(name = "bc")]
#[command(about = "Run a command in the better_commands TUI")]
#[command(trailing_var_arg = true)]
struct Cli {
    #[arg(required = true, num_args = 1.., allow_hyphen_values = true)]
    command: Vec<OsString>,
}

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let mut command_parts = cli.command.into_iter();
    let program = command_parts
        .next()
        .context("a command is required after `--`")?;

    let child = Command::new(&program)
        .args(command_parts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn command {:?}", program))?;

    let mut app = App::new();
    app.set_handle(child)?;
    app.run()
}
