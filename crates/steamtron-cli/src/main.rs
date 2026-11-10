use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use steamtron_core::config::Config;
use steamtron_core::fixes::{export_name, export_zip, install_zip};
use steamtron_core::steam;
use steamtron_doctor::run as run_doctor;
use steamtron_overlay::{OverlayGame, PlatformOverlay};
use steamtron_register::{platform_backend, RegistrationBackend};

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
    /// Report the platform Steam registration backend status.
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
    /// Register App IDs with the platform backend.
    Register {
        #[command(subcommand)]
        command: RegisterCommand,
    },
    /// Add a game to the managed SteamTron library.
    Managed {
        #[command(subcommand)]
        command: ManagedCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RegisterCommand {
    /// Show the backend status.
    Status,
    /// Report whether one App ID is registered.
    Verify {
        appid: String,
    },
    /// Register one App ID from a Lua file.
    Add {
        appid: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        lua: PathBuf,
    },
    /// Remove one App ID registration.
    Remove {
        appid: String,
    },
}

#[derive(Debug, Subcommand)]
enum ManagedCommand {
    /// Download a manifest, register it, and write the library entry.
    Add {
        appid: u32,
        #[arg(long)]
        title: Option<String>,
        /// Drive root, e.g. /mnt/games
        #[arg(long)]
        drive: PathBuf,
        #[arg(long)]
        label: Option<String>,
        /// api (signed in) or steamml (public)
        #[arg(long, default_value = "steamml")]
        provider: String,
        #[arg(long)]
        proton: Option<String>,
    },
    /// Plan or start a managed game without Steam.
    Play {
        game_id: String,
        #[arg(long)]
        target: Option<String>,
        /// Print the plan without starting the game.
        #[arg(long)]
        dry_run: bool,
    },
    /// Verify stored Steam registrations and clear pending states.
    Reconcile,
    /// Rewrite imported entries out of UmuTron paths into SteamTron's layout.
    DetachUmutron,
    /// List, install or remove managed fixes.
    Fixes {
        #[command(subcommand)]
        command: ManagedFixesCommand,
    },
    /// Record the game executable and runner for Play.
    Setup {
        game_id: String,
        #[arg(long)]
        executable: PathBuf,
        #[arg(long)]
        proton: Option<String>,
        #[arg(long, value_delimiter = ' ')]
        arguments: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ManagedFixesCommand {
    List {
        game_id: String,
    },
    Install {
        game_id: String,
        zip: PathBuf,
    },
    Remove {
        game_id: String,
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
        CommandKind::Register { command } => {
            let backend = platform_backend()?;
            match command {
                RegisterCommand::Status => {
                    println!("{}", serde_json::to_string_pretty(&backend.status()?)?);
                }
                RegisterCommand::Verify { appid } => {
                    println!("{}", serde_json::to_string_pretty(&backend.verify(&appid)?)?);
                }
                RegisterCommand::Add { appid, title, lua } => {
                    let lua = std::fs::read_to_string(&lua)
                        .with_context(|| format!("could not read {}", lua.display()))?;
                    let title = title.unwrap_or_else(|| format!("Steam App {appid}"));
                    let manifests = std::collections::BTreeMap::new();
                    let request = steamtron_register::RegistrationRequest {
                        app_id: &appid,
                        title: &title,
                        lua: &lua,
                        manifests: &manifests,
                    };
                    let registration = backend.register(&request)?;
                    let state = backend.verify(&appid)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "backend": registration.backend,
                            "lua": registration.lua_path.display().to_string(),
                            "registered": state.registered,
                        }))?
                    );
                }
                RegisterCommand::Remove { appid } => {
                    backend.unregister(&appid)?;
                    println!("{}", serde_json::to_string_pretty(&backend.verify(&appid)?)?);
                }
            }
        }
        CommandKind::Managed { command } => {
            let backend = platform_backend()?;
            match command {
                ManagedCommand::Add {
                    appid,
                    title,
                    drive,
                    label,
                    provider,
                    proton,
                } => {
                    let provider = match provider.as_str() {
                        "api" => steamtron_library::managed::ManifestProvider::Api,
                        "steamml" => steamtron_library::managed::ManifestProvider::SteamMl,
                        other => anyhow::bail!("unknown provider {other}; use api or steamml"),
                    };
                    let report = steamtron_library::managed::add_managed_game(
                        steamtron_library::managed::ManagedAddRequest {
                            app_id: appid,
                            title: title.unwrap_or_default(),
                            drive_root: drive,
                            drive_label: label.unwrap_or_default(),
                            provider,
                            proton: proton.unwrap_or_default(),
                        },
                        &backend,
                    )?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                ManagedCommand::Setup {
                    game_id,
                    executable,
                    proton,
                    arguments,
                } => {
                    let report = steamtron_library::managed::set_launch_target(
                        steamtron_library::managed::SetTargetRequest {
                            game_id,
                            executable,
                            arguments,
                            proton: proton.unwrap_or_default(),
                            name: String::new(),
                        },
                    )?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                ManagedCommand::Fixes { command } => match command {
                    ManagedFixesCommand::List { game_id } => {
                        let view = steamtron_library::managed::fixes_view(&game_id)?;
                        println!("{}", serde_json::to_string_pretty(&view)?);
                    }
                    ManagedFixesCommand::Install { game_id, zip } => {
                        let view =
                            steamtron_library::managed::install_fixes(&game_id, &zip)?;
                        println!("{}", serde_json::to_string_pretty(&view)?);
                    }
                    ManagedFixesCommand::Remove { game_id } => {
                        let removed =
                            steamtron_library::managed::remove_fixes(&game_id)?;
                        println!("{}", serde_json::to_string_pretty(&removed)?);
                    }
                },
                ManagedCommand::DetachUmutron => {
                    let mut library = steamtron_core::library::Library::load()?;
                    let detached =
                        library.detach_umutron_paths();
                    library.save()?;
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                        "detached": detached,
                        "games": library.games().len(),
                    }))?);
                }
                ManagedCommand::Reconcile => {
                    let report =
                        steamtron_library::managed::reconcile_registrations(&backend)?;
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
                ManagedCommand::Play {
                    game_id,
                    target,
                    dry_run,
                } => {
                    let library =
                        steamtron_core::library::Library::load().context("library not found")?;
                    let mut game = library
                        .game(&game_id)
                        .cloned()
                        .with_context(|| format!("game {game_id} is not in the library"))?;
                    steamtron_core::runtime::use_default_proton(
                        &mut game,
                        &library.settings().default_proton,
                    );
                    let target = match target {
                        Some(id) => game
                            .launch_targets
                            .iter()
                            .find(|candidate| candidate.id == id)
                            .with_context(|| format!("target {id} does not exist"))?,
                        None => steamtron_core::runtime::primary_target(&game)
                            .context("game has no launch target")?,
                    };
                    let steam_root = steam::steam_root()?;
                    let steam_mode = game.launch.requires_steam
                        || game.steam.as_ref().is_some_and(|registration| {
                            matches!(
                                registration.launch_mode,
                                steamtron_core::library::LaunchMode::SteamClient
                            )
                        });
                    let logs = steamtron_core::paths::layout().diagnostics_for(&game.id);
                    steamtron_core::paths::ensure_private_dir(&logs)?;
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or(0);
                    let log = logs.join(format!("play-{stamp}.log"));
                    if steam_mode {
                        let app_id =
                            steamtron_library::managed::steam_ready(&game, &backend)?;
                        let plan = steamtron_core::runtime::steam_launch_plan(app_id, &steam_root);
                        let summary = serde_json::json!({
                            "mode": "steam",
                            "program": plan.program.display().to_string(),
                            "arguments": plan.arguments,
                        });
                        if dry_run {
                            println!("{}", serde_json::to_string_pretty(&summary)?);
                            return Ok(());
                        }
                        let stdout = std::fs::File::create(&log)?;
                        let stderr = stdout.try_clone()?;
                        let mut child = std::process::Command::new("setsid")
                            .arg(&plan.program)
                            .args(&plan.arguments)
                            .stdin(std::process::Stdio::null())
                            .stdout(stdout)
                            .stderr(stderr)
                            .spawn()
                            .context("could not start Steam")?;
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "pid": child.id(),
                                "log": log.display().to_string(),
                                "mode": "steam",
                            }))?
                        );
                        let _ = child.wait();
                        return Ok(());
                    }
                    let plan = steamtron_core::runtime::managed_launch_plan(&game, target, &steam_root)?;
                    let summary = serde_json::json!({
                        "mode": "proton",
                        "program": plan.program.display().to_string(),
                        "arguments": plan.arguments,
                        "environment": plan.environment,
                        "cwd": plan.cwd.display().to_string(),
                    });
                    if dry_run {
                        println!("{}", serde_json::to_string_pretty(&summary)?);
                        return Ok(());
                    }
                    let stdout = std::fs::File::create(&log)?;
                    let stderr = stdout.try_clone()?;
                    let mut child = plan
                        .command()
                        .stdin(std::process::Stdio::null())
                        .stdout(stdout)
                        .stderr(stderr)
                        .spawn()
                        .context("could not start the game")?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "pid": child.id(),
                            "log": log.display().to_string(),
                            "mode": "proton",
                        }))?
                    );
                    let _ = child.wait();
                }
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
                    println!("name={}", export_name(&appid));
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
