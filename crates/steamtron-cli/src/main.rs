use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use steamtron_core::config::Config;
use steamtron_core::fixes::{export_name, export_zip, install_zip};
use steamtron_core::steam;
use steamtron_doctor::run as run_doctor;
use steamtron_overlay::{OverlayGame, PlatformOverlay};
use steamtron_unlock::{UnlockBackend, platform_backend};

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
    /// Report the platform Steam unlock backend status.
    Backend,
    /// Validate Steam, helpers, fixes, and overlay requirements.
    Doctor,
    /// List installed Steam games.
    Games,
    /// Manage per-game fixes mappings.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Install, export, or inspect fixes ZIPs.
    Fixes {
        #[command(subcommand)]
        command: FixesCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    List,
    Set {
        appid: String,
        fixes: PathBuf,
        #[arg(long)]
        disabled: bool,
    },
    Remove { appid: String },
}

#[derive(Debug, Subcommand)]
enum FixesCommand {
    Install { appid: String, zip: PathBuf },
    Export { appid: String, destination: PathBuf },
    List,
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
            let overlay = PlatformOverlay::new(game);
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
            let mut overlay = PlatformOverlay::new(game);
            let merged = overlay.mount()?.to_path_buf();
            let mut rewritten = Vec::with_capacity(command.len());
            for value in &command {
                rewritten.push(overlay.game().rewrite_arg(value, &merged));
            }
            let workdir = cwd
                .as_deref()
                .map(|path| overlay.game().rewrite_cwd(path, &merged))
                .unwrap_or_else(|| {
                    rewritten
                        .first()
                        .map(std::path::Path::new)
                        .and_then(std::path::Path::parent)
                        .map(std::path::Path::to_path_buf)
                        .unwrap_or_else(|| merged.clone())
                });
            let mut process = Command::new(rewritten.first().context("missing command after --")?);
            process.args(&rewritten[1..]).current_dir(workdir);
            if let Some(appid) = overlay.game().appid() {
                process.env("SteamAppId", appid);
                process.env("SteamGameId", appid);
            }
            let status = process.status().context("could not launch command")?;
            overlay.unmount()?;
            std::process::exit(status.code().unwrap_or(1));
        }
        CommandKind::Doctor => {
            let report = run_doctor();
            println!("{}", serde_json::to_string_pretty(&report).unwrap_or_else(|_| format!("{report:#?}")));
        }
        CommandKind::Backend => {
            let backend = platform_backend()?;
            println!("{:#?}", backend.status()?);
        }
        CommandKind::Games => {
            let managed = steam::managed_appids()?;
            for game in steam::games()? {
                let marker = if managed.contains(&game.appid) { "*" } else { " " };
                println!(
                    "{marker} {:<10} {:<40} build={} common={}",
                    game.appid,
                    game.name,
                    game.build_id,
                    game.common.display()
                );
            }
        }
        CommandKind::Config { command } => {
            let mut config = Config::load()?;
            match command {
                ConfigCommand::List => {
                    for entry in config.entries() {
                        println!(
                            "{} enabled={} fixes={}",
                            entry.appid,
                            entry.enabled,
                            entry.fixes.display()
                        );
                    }
                }
                ConfigCommand::Set {
                    appid,
                    fixes,
                    disabled,
                } => {
                    let entry = config.upsert(appid, fixes, !disabled)?;
                    println!(
                        "{} enabled={} fixes={}",
                        entry.appid,
                        entry.enabled,
                        entry.fixes.display()
                    );
                }
                ConfigCommand::Remove { appid } => {
                    println!("{}", if config.remove(&appid)? { "removed" } else { "not found" });
                }
            }
        }
        CommandKind::Fixes { command } => {
            let mut config = Config::load()?;
            match command {
                FixesCommand::Install { appid, zip } => {
                    let target = install_zip(&appid, &zip)?;
                    let entry = config.upsert(&appid, target.clone(), true)?;
                    println!(
                        "{} enabled={} fixes={}",
                        entry.appid,
                        entry.enabled,
                        entry.fixes.display()
                    );
                }
                FixesCommand::Export { appid, destination } => {
                    let entry = config
                        .entry(&appid)
                        .with_context(|| format!("no fixes configured for {appid}"))?;
                    export_zip(&entry.fixes, &destination, &appid)?;
                    println!("{}", destination.display());
                    println!("name={}", export_name(&appid, &appid));
                }
                FixesCommand::List => {
                    for entry in config.entries() {
                        println!("{} {}", entry.appid, entry.fixes.display());
                    }
                }
            }
        }
    }
    Ok(())
}
