use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use steamtron_overlay::{LinuxOverlay, OverlayGame};

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn overlays_fixes_and_persists_writes() {
    if !Path::new("/usr/bin/fuse-overlayfs").is_file() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let common = temp.path().join("steamapps/common/Game");
    let fixes = temp.path().join("fixes");
    let data_home = temp.path().join("data");
    fs::create_dir_all(&common).unwrap();
    fs::create_dir_all(&fixes).unwrap();
    write_executable(&common.join("run.sh"), "#!/bin/sh\necho ORIGINAL\n");
    write_executable(&fixes.join("run.sh"), "#!/bin/sh\necho FIXED\n");

    let game = OverlayGame::new(&common, &fixes, Some("42")).unwrap();
    let mut overlay = LinuxOverlay::with_data_home(game.clone(), &data_home);
    let merged = overlay.mount().unwrap().to_path_buf();

    let output = Command::new(merged.join("run.sh")).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "FIXED");

    fs::write(merged.join("written-by-game.txt"), "upper").unwrap();
    overlay.unmount().unwrap();
    assert!(overlay.layout().upper.join("written-by-game.txt").is_file());

    let mut reopened = LinuxOverlay::with_data_home(game, &data_home);
    let merged = reopened.mount().unwrap().to_path_buf();
    assert_eq!(
        fs::read_to_string(merged.join("written-by-game.txt")).unwrap(),
        "upper"
    );
    reopened.unmount().unwrap();
}
