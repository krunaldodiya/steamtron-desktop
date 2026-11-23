//! Windows game runtime provisioning.
//!
//! Steam runs each app's install script (usually a `_CommonRedist` folder) to
//! add DirectX and Visual C++ before the first launch. Managed downloads only
//! fetch content depots, so a game could start on a machine without
//! `XINPUT1_3.dll` and fail with "install DirectX first". These functions read
//! the launch target's PE import table and install the matching Microsoft
//! redistributables, cached per machine.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use steamtron_core::process::hidden_command;

const DIRECTX_URL: &str =
    "https://download.microsoft.com/download/8/4/A/84A35BF1-DAFE-4AE8-82AF-AD2AE20B6B14/directx_Jun2010_redist.exe";
const VC14_X64_URL: &str = "https://aka.ms/vc14/vc_redist.x64.exe";
const VC14_X86_URL: &str = "https://aka.ms/vc14/vc_redist.x86.exe";
const VC12_X64_URL: &str = "https://aka.ms/highdpimfc2013x64enu";
const VC12_X86_URL: &str = "https://aka.ms/highdpimfc2013x86enu";
const VC11_X64_URL: &str =
    "https://download.microsoft.com/download/1/6/B/16B06F60-3B20-4FF2-B699-5E9B7962F9AE/VSU_4/vcredist_x64.exe";
const VC11_X86_URL: &str =
    "https://download.microsoft.com/download/1/6/B/16B06F60-3B20-4FF2-B699-5E9B7962F9AE/VSU_4/vcredist_x86.exe";
const VC10_X64_URL: &str =
    "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x64.exe";
const VC10_X86_URL: &str =
    "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x86.exe";

const INSTALL_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum VcVersion {
    V10,
    V11,
    V12,
    V14,
}

impl VcVersion {
    fn label(self) -> &'static str {
        match self {
            VcVersion::V10 => "Visual C++ 2010 Redistributable",
            VcVersion::V11 => "Visual C++ 2012 Redistributable",
            VcVersion::V12 => "Visual C++ 2013 Redistributable",
            VcVersion::V14 => "Visual C++ 2015-2022 Redistributable",
        }
    }

    fn url(self, x86: bool) -> &'static str {
        match (self, x86) {
            (VcVersion::V14, false) => VC14_X64_URL,
            (VcVersion::V14, true) => VC14_X86_URL,
            (VcVersion::V12, false) => VC12_X64_URL,
            (VcVersion::V12, true) => VC12_X86_URL,
            (VcVersion::V11, false) => VC11_X64_URL,
            (VcVersion::V11, true) => VC11_X86_URL,
            (VcVersion::V10, false) => VC10_X64_URL,
            (VcVersion::V10, true) => VC10_X86_URL,
        }
    }

    fn args(self) -> &'static [&'static str] {
        match self {
            // The 2010 package predates the /install form.
            VcVersion::V10 => &["/q", "/norestart"],
            _ => &["/install", "/quiet", "/norestart"],
        }
    }

    fn tag(self) -> &'static str {
        match self {
            VcVersion::V10 => "vc2010",
            VcVersion::V11 => "vc2012",
            VcVersion::V12 => "vc2013",
            VcVersion::V14 => "vc2015-2022",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Component {
    DirectX,
    VcRedist { version: VcVersion, x86: bool },
}

impl Component {
    fn label(self) -> String {
        match self {
            Component::DirectX => "DirectX End-User Runtime (June 2010)".into(),
            Component::VcRedist { version, x86 } => format!(
                "{} ({})",
                version.label(),
                if x86 { "x86" } else { "x64" }
            ),
        }
    }

    fn args(self) -> &'static [&'static str] {
        match self {
            Component::DirectX => &["/silent"],
            Component::VcRedist { version, .. } => version.args(),
        }
    }
}

/// Import names that belong to a runtime Steam would install from the game's
/// own install script, but whose official installer is not bundled here. They
/// are logged so a launch failure is diagnosable instead of silent.
const UNPROVISIONED: &[&str] = &[
    "msvcp90.dll",
    "msvcr90.dll",
    "msvcp80.dll",
    "msvcr80.dll",
    "physxloader.dll",
    "physxcore.dll",
    "nvcuda.dll",
    "mscoree.dll",
];

pub(crate) fn unprovisioned_imports(imports: &[String]) -> Vec<String> {
    imports
        .iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            UNPROVISIONED.contains(&lower.as_str())
        })
        .cloned()
        .collect()
}

/// Known runtime DLLs mapped from a PE import name. Everything outside this
/// table is left to the game's own bundling.
pub(crate) fn component_for_import(name: &str, x86: bool) -> Option<Component> {
    let name = name.to_ascii_lowercase();
    if name.starts_with("d3dx9_")
        || name.starts_with("d3dx10_")
        || name.starts_with("d3dx11_")
        || matches!(
            name.as_str(),
            "d3dcompiler_43.dll"
                | "xinput1_3.dll"
                | "x3daudio1_7.dll"
                | "xapofx1_5.dll"
                | "xaudio2_7.dll"
        )
    {
        return Some(Component::DirectX);
    }
    let version = if matches!(
        name.as_str(),
        "msvcp140.dll" | "vcruntime140.dll" | "vcruntime140_1.dll" | "concrt140.dll"
    ) {
        VcVersion::V14
    } else if matches!(name.as_str(), "msvcp120.dll" | "msvcr120.dll" | "vccorlib120.dll") {
        VcVersion::V12
    } else if matches!(name.as_str(), "msvcp110.dll" | "msvcr110.dll") {
        VcVersion::V11
    } else if matches!(name.as_str(), "msvcp100.dll" | "msvcr100.dll") {
        VcVersion::V10
    } else {
        return None;
    };
    Some(Component::VcRedist { version, x86 })
}

pub(crate) fn components_for_imports(imports: &[String], x86: bool) -> Vec<Component> {
    let mut result = Vec::new();
    for name in imports {
        if let Some(component) = component_for_import(name, x86) {
            if !result.contains(&component) {
                result.push(component);
            }
        }
    }
    result
}

/// Install whatever the launch target needs and log to the play log. A no-op
/// when every import already resolves. The user sees one UAC prompt per
/// missing redistributable, not a manual installer.
pub(crate) fn ensure_for_game(
    executable: &Path,
    game_root: &Path,
    fixes: Option<&Path>,
    log: &File,
) -> Result<Vec<String>, String> {
    // Steam runs a game's install script at install time; managed downloads and
    // restored backups never do. PlayStation PC ports ship the PSPC SDK
    // runtime installer and their helper (`crs-video.exe`) reports it missing
    // without it. The runtime is optional for gameplay, so this is best-effort
    // and runs before the import scan can fail.
    ensure_playstation_runtime(game_root, log);
    let Some((machine, imports)) = pe_imports(executable) else {
        note(log, format!("could not read imports from {}", executable.display()));
        return Ok(Vec::new());
    };
    let x86 = machine == 0x014c;
    for name in unprovisioned_imports(&imports) {
        note(
            log,
            format!("no automatic installer for {name}; the game may need its own redistributable"),
        );
    }
    // Group the imports that are actually missing by installer. Checking the
    // concrete DLL (not one probe per component) matters when a DX runtime
    // partially exists: Steam's install would add every missing piece.
    let mut pending: Vec<(Component, Vec<String>)> = Vec::new();
    for name in &imports {
        let Some(component) = component_for_import(name, x86) else { continue };
        if dll_present(name, x86, executable, game_root, fixes) {
            continue;
        }
        match pending.iter_mut().find(|(existing, _)| *existing == component) {
            Some((_, list)) => {
                if !list.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                    list.push(name.clone());
                }
            }
            None => pending.push((component, vec![name.clone()])),
        }
    }
    let mut installed = Vec::new();
    for (component, dlls) in pending {
        note(
            log,
            format!("missing {} for {}", component.label(), dlls.join(", ")),
        );
        install(component, game_root, log)?;
        for dll in &dlls {
            if !dll_present(dll, x86, executable, game_root, fixes) {
                return Err(format!(
                    "{} installed but {} is still missing.",
                    component.label(),
                    dll
                ));
            }
        }
        note(log, format!("installed {}", component.label()));
        installed.push(component.label());
    }
    Ok(installed)
}

/// Files Steam's install script runs for PlayStation PC ports. This is an
/// allow-list: SteamTron never executes arbitrary install scripts from a game
/// folder (0098 non-goal).
const PSPC_INSTALL_SCRIPT: &str = "install_pspc_sdk_runtime.bat";
const PSPC_MANAGER_MSI: &str = "PsPcSdkRuntimeManager.msi";
const PSPC_RUNTIME_MSI: &str = "PsPcSdkRuntimeInstaller.msi";

fn playstation_bundle_present(game_root: &Path) -> bool {
    game_root.join(PSPC_INSTALL_SCRIPT).is_file()
        && game_root.join(PSPC_MANAGER_MSI).is_file()
        && game_root.join(PSPC_RUNTIME_MSI).is_file()
}

fn playstation_runtime_installed() -> bool {
    let Some(program_data) = std::env::var_os("ProgramData") else {
        return false;
    };
    playstation_runtime_in(Path::new(&program_data))
}

/// `%ProgramData%\Sony Interactive Entertainment Inc\PSPC_SDK\<family>\<version>\runtime.dll`
fn playstation_runtime_in(program_data: &Path) -> bool {
    let root = program_data
        .join("Sony Interactive Entertainment Inc")
        .join("PSPC_SDK");
    let Ok(families) = fs::read_dir(&root) else {
        return false;
    };
    for family in families.flatten() {
        let Ok(versions) = fs::read_dir(family.path()) else {
            continue;
        };
        for version in versions.flatten() {
            if version.path().join("runtime.dll").is_file() {
                return true;
            }
        }
    }
    false
}

/// Install the PlayStation PC SDK runtime that Steam's install script would add.
///
/// Best-effort: the runtime only powers the game's PSN helpers, so a declined
/// or failed install is logged and the game still launches. The batch file runs
/// both MSIs, so the user sees one elevation prompt.
fn ensure_playstation_runtime(game_root: &Path, log: &File) {
    if !playstation_bundle_present(game_root) || playstation_runtime_installed() {
        return;
    }
    note(
        log,
        "PlayStation PC SDK Runtime is missing; running the game's install script",
    );
    let script = game_root.join(PSPC_INSTALL_SCRIPT);
    let command = format!("\"{}\" quiet", script.display());
    let result = run_elevated(Path::new("cmd.exe"), &["/c", command.as_str()], log);
    if playstation_runtime_installed() {
        note(log, "installed PlayStation PC SDK Runtime");
        return;
    }
    match result {
        Ok(()) => note(
            log,
            "PlayStation PC SDK Runtime installer finished but the runtime was not found",
        ),
        Err(error) => note(
            log,
            format!("PlayStation PC SDK Runtime was not installed: {error}"),
        ),
    }
}

fn dll_present(
    dll: &str,
    x86: bool,
    executable: &Path,
    game_root: &Path,
    fixes: Option<&Path>,
) -> bool {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(parent) = executable.parent() {
        dirs.push(parent.to_path_buf());
    }
    dirs.push(game_root.to_path_buf());
    if let Some(fixes) = fixes {
        dirs.push(fixes.to_path_buf());
    }
    let windows = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    // The same DLL name exists in both system directories on 64-bit Windows.
    // Accept either copy; the redistributable installs the architecture that
    // matches the game.
    dirs.push(windows.join(if x86 { "SysWOW64" } else { "System32" }));
    dirs.push(windows.join(if x86 { "System32" } else { "SysWOW64" }));
    for dir in dirs {
        if dir.join(dll).is_file() {
            return true;
        }
        if dir.join("steam_settings").join(dll).is_file() {
            return true;
        }
    }
    false
}

fn note(log: &File, message: impl AsRef<str>) {
    let _ = writeln!(&*log, "[runtime] {}", message.as_ref());
}

fn cache_dir() -> Result<PathBuf, String> {
    let path = steamtron_core::paths::layout().home().join("runtimes");
    steamtron_core::paths::ensure_private_dir(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

fn download(url: &str, target: &Path, log: &File) -> Result<(), String> {
    if target.is_file() {
        if signature_ok(target) {
            note(log, format!("cached {}", target.display()));
            return Ok(());
        }
        fs::remove_file(target).map_err(|error| error.to_string())?;
    }
    let mut last_error = String::new();
    for attempt in 0..2 {
        match download_once(url, target, log) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = error;
                let _ = fs::remove_file(target);
                let _ = fs::remove_file(target.with_extension("part"));
                if attempt == 0 {
                    note(log, "runtime download failed; retrying once");
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        }
    }
    Err(last_error)
}

fn download_once(url: &str, target: &Path, log: &File) -> Result<(), String> {
    note(log, format!("downloading {url}"));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("download failed: {error}"))?;
    let part = target.with_extension("part");
    let mut file = File::create(&part).map_err(|error| format!("{}: {error}", part.display()))?;
    response
        .copy_to(&mut file)
        .map_err(|error| format!("download failed: {error}"))?;
    file.flush().map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(&part, target).map_err(|error| error.to_string())?;
    if !signature_ok(target) {
        let _ = fs::remove_file(target);
        return Err(format!(
            "{} is not a valid Microsoft-signed installer",
            target.display()
        ));
    }
    Ok(())
}

fn signature_ok(path: &Path) -> bool {
    let quoted = path.display().to_string().replace('\'', "''");
    let script = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath '{quoted}'; if ($s.Status -eq 'Valid' -and $s.SignerCertificate.Subject -like '*Microsoft*') {{ 'ok' }} else {{ 'bad' }}"
    );
    hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "ok")
        .unwrap_or(false)
}

fn bundled_name_matches(name: &str, version: VcVersion, x86: bool) -> bool {
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".exe") || !(lower.contains("vcredist") || lower.starts_with("vc_redist")) {
        return false;
    }
    let arch = if x86 {
        lower.contains("x86") || lower.contains("win32")
    } else {
        lower.contains("x64") || lower.contains("amd64")
    };
    if !arch {
        return false;
    }
    let year = ["2010", "2012", "2013", "2015", "2017", "2019", "2022"]
        .into_iter()
        .find(|year| lower.contains(year));
    match (version, year) {
        (VcVersion::V10, Some("2010")) => true,
        (VcVersion::V11, Some("2012")) => true,
        (VcVersion::V12, Some("2013")) => true,
        (VcVersion::V14, Some("2015" | "2017" | "2019" | "2022")) => true,
        (VcVersion::V14, None) if lower.starts_with("vc_redist") => true,
        _ => false,
    }
}

/// Directories Steam repacks and Steam install scripts use for redistributables.
const REDIST_DIRS: &[(&str, usize)] = &[
    ("", 1),
    ("_Redist", 3),
    ("_CommonRedist", 3),
    ("Redist", 3),
    ("Redistributables", 3),
    ("Support", 3),
    ("_Support", 3),
    ("Installers", 3),
    ("__Installer", 3),
    ("_Installer", 3),
    ("Install", 3),
    ("Runtime", 3),
    ("Runtimes", 3),
];

fn bundled_installer(root: &Path, component: Component, log: &File) -> Option<PathBuf> {
    let (version, x86) = match component {
        Component::VcRedist { version, x86 } => (version, x86),
        Component::DirectX => return None,
    };
    for &(relative, max_depth) in REDIST_DIRS {
        let base = if relative.is_empty() {
            root.to_path_buf()
        } else {
            root.join(relative)
        };
        if !base.is_dir() {
            continue;
        }
        if let Some(path) = find_bundled_in(&base, 0, max_depth, version, x86, log) {
            return Some(path);
        }
    }
    None
}

fn find_bundled_in(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    version: VcVersion,
    x86: bool,
    log: &File,
) -> Option<PathBuf> {
    if depth > max_depth {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < max_depth {
                if let Some(found) =
                    find_bundled_in(&path, depth + 1, max_depth, version, x86, log)
                {
                    return Some(found);
                }
            }
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !bundled_name_matches(name, version, x86) {
            continue;
        }
        if !signature_ok(&path) {
            note(
                log,
                format!(
                    "ignoring bundled runtime without a Microsoft signature: {}",
                    path.display()
                ),
            );
            continue;
        }
        return Some(path);
    }
    None
}

fn install(component: Component, game_root: &Path, log: &File) -> Result<(), String> {
    // Steam's install script runs the redistributables the game already ships.
    // Prefer those signed Microsoft installers over a network download; some
    // older packages are no longer reliably downloadable.
    if let Some(path) = bundled_installer(game_root, component, log) {
        note(
            log,
            format!(
                "using bundled {} for {}",
                path.display(),
                component.label()
            ),
        );
        return run_elevated(&path, component.args(), log);
    }
    let cache = cache_dir()?;
    match component {
        Component::DirectX => {
            let archive = cache.join("directx_Jun2010_redist.exe");
            download(DIRECTX_URL, &archive, log)?;
            let extracted = cache.join("directx");
            if !extracted.join("DXSETUP.exe").is_file() {
                note(log, "extracting DirectX redistributable");
                let status = hidden_command(&archive)
                    .arg("/Q")
                    .arg(format!("/T:{}", extracted.display()))
                    .status()
                    .map_err(|error| format!("could not extract DirectX: {error}"))?;
                if !status.success() {
                    return Err("DirectX redistributable extraction failed.".into());
                }
            }
            let setup = extracted.join("DXSETUP.exe");
            run_elevated(&setup, &["/silent"], log)
        }
        Component::VcRedist { version, x86 } => {
            let name = format!(
                "{}-{}.exe",
                version.tag(),
                if x86 { "x86" } else { "x64" }
            );
            let installer = cache.join(name);
            download(version.url(x86), &installer, log)?;
            run_elevated(&installer, version.args(), log)
        }
    }
}

fn run_elevated(program: &Path, args: &[&str], log: &File) -> Result<(), String> {
    let file = program.display().to_string().replace('\'', "''");
    let arguments = args
        .iter()
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    note(log, "waiting for the Windows elevation prompt");
    let script = format!(
        "$p = Start-Process -FilePath '{file}' -ArgumentList @({arguments}) -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
    );
    let mut child = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start the runtime installer: {error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => {
                let code = status.code().unwrap_or(-1);
                // 3010/1641 mean success with a pending restart.
                if code == 0 || code == 3010 || code == 1641 {
                    return Ok(());
                }
                return Err(format!(
                    "runtime installation failed (exit {}{}). Approve the elevation prompt to continue.",
                    code,
                    if started.elapsed() > INSTALL_TIMEOUT { ", timed out" } else { "" }
                ));
            }
            None if started.elapsed() > INSTALL_TIMEOUT => {
                let _ = child.kill();
                return Err("runtime installation timed out".into());
            }
            None => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

/// Minimal PE import reader: DOS header, COFF header, optional header, one
/// section table, and the import directory. Enough for runtime detection.
fn pe_imports(path: &Path) -> Option<(u16, Vec<String>)> {
    let mut bytes = Vec::new();
    File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    if bytes.get(0..2)? != b"MZ" {
        return None;
    }
    let pe = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if bytes.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let machine = u16::from_le_bytes(bytes.get(coff..coff + 2)?.try_into().ok()?);
    let sections = u16::from_le_bytes(bytes.get(coff + 2..coff + 4)?.try_into().ok()?) as usize;
    let optional_size =
        u16::from_le_bytes(bytes.get(coff + 16..coff + 18)?.try_into().ok()?) as usize;
    let optional = coff + 20;
    let magic = u16::from_le_bytes(bytes.get(optional..optional + 2)?.try_into().ok()?);
    let directory = optional + if magic == 0x20b { 112 } else { 96 };
    // Data directory 1 is the import table; directory 0 is the export table.
    let import_rva =
        u32::from_le_bytes(bytes.get(directory + 8..directory + 12)?.try_into().ok()?) as usize;
    if import_rva == 0 {
        return Some((machine, Vec::new()));
    }
    let section_table = optional + optional_size;
    let span = |index: usize| -> Option<(usize, usize, usize, usize)> {
        let section = section_table + index * 40;
        Some((
            u32::from_le_bytes(bytes.get(section + 8..section + 12)?.try_into().ok()?) as usize,
            u32::from_le_bytes(bytes.get(section + 12..section + 16)?.try_into().ok()?) as usize,
            u32::from_le_bytes(bytes.get(section + 16..section + 20)?.try_into().ok()?) as usize,
            u32::from_le_bytes(bytes.get(section + 20..section + 24)?.try_into().ok()?) as usize,
        ))
    };
    let file_offset = |rva: usize| -> Option<usize> {
        for index in 0..sections {
            let (virtual_size, virtual_address, raw_size, raw_pointer) = span(index)?;
            if rva >= virtual_address && rva < virtual_address + virtual_size.max(raw_size) {
                return Some(raw_pointer + rva - virtual_address);
            }
        }
        None
    };
    let mut imports = Vec::new();
    let mut descriptor = file_offset(import_rva)?;
    for _ in 0..512 {
        let name_rva =
            u32::from_le_bytes(bytes.get(descriptor + 12..descriptor + 16)?.try_into().ok()?)
                as usize;
        let lookup =
            u32::from_le_bytes(bytes.get(descriptor..descriptor + 4)?.try_into().ok()?);
        if name_rva == 0 && lookup == 0 {
            break;
        }
        if name_rva != 0 {
            if let Some(start) = file_offset(name_rva) {
                if let Some(end) = bytes[start..].iter().position(|byte| *byte == 0) {
                    let name = String::from_utf8_lossy(&bytes[start..start + end]).into_owned();
                    if !name.is_empty() && !imports.contains(&name) {
                        imports.push(name);
                    }
                }
            }
        }
        descriptor += 20;
    }
    Some((machine, imports))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_runtime_names_match_version_and_architecture() {
        assert!(bundled_name_matches(
            "vcredist_x64_2012_x64.exe",
            VcVersion::V11,
            false
        ));
        assert!(bundled_name_matches(
            "vcredist_x86_2012_x86.exe",
            VcVersion::V11,
            true
        ));
        assert!(!bundled_name_matches(
            "vcredist_x86_2012_x86.exe",
            VcVersion::V11,
            false
        ));
        assert!(!bundled_name_matches(
            "vcredist_x64_2012_x64.exe",
            VcVersion::V11,
            true
        ));
        assert!(bundled_name_matches("vc_redist.x64.exe", VcVersion::V14, false));
        assert!(!bundled_name_matches("vc_redist.x64.exe", VcVersion::V11, false));
        assert!(bundled_name_matches(
            "vcredist_x64_2013_x64.exe",
            VcVersion::V12,
            false
        ));
        assert!(!bundled_name_matches(
            "vcredist_x64_2013_x64.exe",
            VcVersion::V11,
            false
        ));
    }

    #[test]
    fn maps_directx_and_vc_imports() {
        let imports = vec![
            "XINPUT1_3.dll".to_string(),
            "d3dx9_43.dll".to_string(),
            "MSVCP140.dll".to_string(),
            "KERNEL32.dll".to_string(),
        ];
        assert_eq!(
            components_for_imports(&imports, false),
            vec![
                Component::DirectX,
                Component::VcRedist { version: VcVersion::V14, x86: false }
            ]
        );
    }

    #[test]
    fn maps_older_visual_c_versions() {
        let imports = vec!["msvcp120.dll".into(), "msvcr110.dll".into(), "msvcr100.dll".into()];
        assert_eq!(
            components_for_imports(&imports, true),
            vec![
                Component::VcRedist { version: VcVersion::V12, x86: true },
                Component::VcRedist { version: VcVersion::V11, x86: true },
                Component::VcRedist { version: VcVersion::V10, x86: true },
            ]
        );
    }

    #[test]
    fn reports_imports_without_a_provisioner() {
        let imports = vec!["msvcr90.dll".into(), "PhysXLoader.dll".into(), "kernel32.dll".into()];
        assert_eq!(unprovisioned_imports(&imports), vec!["msvcr90.dll", "PhysXLoader.dll"]);
    }

    #[test]
    fn deduplicates_and_ignores_unknown_imports() {
        let imports = vec!["xinput1_3.dll".into(), "xapofx1_5.dll".into(), "user32.dll".into()];
        assert_eq!(components_for_imports(&imports, true), vec![Component::DirectX]);
    }

    #[test]
    fn accepts_microsoft_signed_binaries() {
        let notepad = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        if !notepad.is_file() {
            return;
        }
        assert!(signature_ok(&notepad));
    }

    #[test]
    fn reads_imports_from_a_system_binary() {
        let notepad = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        if !notepad.is_file() {
            return;
        }
        let (_, imports) = pe_imports(&notepad).expect("notepad imports");
        assert!(imports.iter().any(|name| name.eq_ignore_ascii_case("gdi32.dll")));
    }

    #[test]
    fn reads_little_nightmares_imports_when_installed() {
        let game = PathBuf::from(
            r"D:\SteamTronLibrary\steamtronapps\common\Little Nightmares\Atlas\Binaries\Win64\LittleNightmares.exe",
        );
        if !game.is_file() {
            return;
        }
        let (machine, imports) = pe_imports(&game).expect("game imports");
        assert_eq!(machine, 0x8664);
        assert!(imports.iter().any(|name| name.eq_ignore_ascii_case("xinput1_3.dll")));
        assert_eq!(
            components_for_imports(&imports, false),
            vec![
                Component::DirectX,
                Component::VcRedist { version: VcVersion::V14, x86: false }
            ]
        );
    }

    #[test]
    fn detects_the_playstation_runtime_bundle() {
        let temp = tempfile::tempdir().unwrap();
        assert!(!playstation_bundle_present(temp.path()));
        fs::write(temp.path().join(PSPC_INSTALL_SCRIPT), b"").unwrap();
        assert!(!playstation_bundle_present(temp.path()));
        fs::write(temp.path().join(PSPC_MANAGER_MSI), b"").unwrap();
        assert!(!playstation_bundle_present(temp.path()));
        fs::write(temp.path().join(PSPC_RUNTIME_MSI), b"").unwrap();
        assert!(playstation_bundle_present(temp.path()));
    }

    #[test]
    fn detects_an_installed_playstation_runtime() {
        let temp = tempfile::tempdir().unwrap();
        assert!(!playstation_runtime_in(temp.path()));
        let version = temp
            .path()
            .join("Sony Interactive Entertainment Inc")
            .join("PSPC_SDK")
            .join("S22")
            .join("2.50.00.10");
        fs::create_dir_all(&version).unwrap();
        assert!(!playstation_runtime_in(temp.path()));
        fs::write(version.join("runtime.dll"), b"").unwrap();
        assert!(playstation_runtime_in(temp.path()));
        fs::remove_file(version.join("runtime.dll")).unwrap();
        assert!(!playstation_runtime_in(temp.path()));
    }
}
