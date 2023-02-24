use std::{fs::read_dir, path::PathBuf};

use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Debug, Clone)]
enum Command {
    TurnOn,
    TurnOff,
}

/// Simple program to turn on/off the display.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Command to perform on the screen
    command: Command,
}

fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    color_backtrace::install();
    let args = Args::parse();

    let path = get_bl_power_file()?;

    std::fs::write(
        path,
        match args.command {
            Command::TurnOn => "0",
            Command::TurnOff => "4",
        },
    )?;

    Ok(())
}

fn get_bl_power_file() -> Result<PathBuf, anyhow::Error> {
    let mut path = PathBuf::from("/sys/class/backlight");
    let name = read_dir(&path)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_symlink()).unwrap_or(false))
        .map(|entry| entry.file_name())
        .next()
        .ok_or_else(|| anyhow::anyhow!("No backlight found"))?;
    path.push(name);
    path.push("bl_power");
    Ok(path)
}
