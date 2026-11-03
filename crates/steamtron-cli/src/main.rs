use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use steamtron_overlay::{LinuxOverlay, OverlayGame};

#[derive(Debug, Parser)]
#[command(name = "steamtron-overlay", version)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Mount the fixes overlay, run a command, then unmount.
    Run {
        #[arg(long)]
        appid: Option<String>,
        #[arg(long)]
        common: PathBuf,
        #[arg(long)]
        fixes: PathBuf,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Print the resolved overlay layout without mounting.
    Inspect {
        #[arg(long)]
        appid: Option<String>,
        #[arg(long)]
        common: PathBuf,
        #[arg(long)]
        fixes: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        CommandKind::Inspect {
            appid,
            common,
            fixes,
        } => {
            let game = OverlayGame::new(common, fixes, appid)?;
            let overlay = LinuxOverlay::new(game);
            println!("game={}", overlay.game().common().display());
            println!("fixes={}", overlay.game().fixes().display());
            println!("upper={}", overlay.layout().upper.display());
            println!("work={}", overlay.layout().work.display());
            println!("merged={}", overlay.layout().merged.display());
        }
        CommandKind::Run {
            appid,
            common,
            fixes,
            cwd,
            command,
        } => {
            let game = OverlayGame::new(common, fixes, appid)?;
            let mut overlay = LinuxOverlay::new(game);
            let merged = overlay.mount()?.to_path_buf();
            let mut rewritten = Vec::with_capacity(command.len());
            for value in &command {
                rewritten.push(overlay.game().rewrite_arg(value, &merged));
            }
            let workdir = cwd
                .as_deref()
                .map(|path| overlay.game().rewrite_cwd(path, &merged))
                .unwrap_or_else(|| merged.clone());
            let mut process = Command::new(
                rewritten
                    .first()
                    .context("missing command after --")?,
            );
            process.args(&rewritten[1..]).current_dir(workdir);
            if let Some(appid) = overlay.game().appid() {
                process.env("SteamAppId", appid);
                process.env("SteamGameId", appid);
            }
            let status = process.status().context("could not launch command")?;
            overlay.unmount()?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
    Ok(())
}
