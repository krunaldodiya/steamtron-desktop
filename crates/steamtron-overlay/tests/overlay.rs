#![cfg(target_os = "linux")]

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
fn stable_launch_root_stays_identical_while_mounted() {
    if !Path::new("/usr/bin/fuse-overlayfs").is_file() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let common = temp.path().join("steamapps/common/Game Name");
    let launch = temp
        .path()
        .join("steamapps/common/.steamtron-junction/Game Name");
    let fixes = temp.path().join("fixes");
    let data_home = temp.path().join("data");
    fs::create_dir_all(common.join("sub")).unwrap();
    fs::create_dir_all(&fixes).unwrap();
    fs::create_dir_all(launch.parent().unwrap()).unwrap();
    write_executable(
        &common.join("run.sh"),
        "#!/bin/sh\necho \"argv0=$0\"\necho \"cwd=$(pwd)\"\necho CONTENT\n",
    );
    write_executable(
        &fixes.join("run.sh"),
        "#!/bin/sh\necho \"argv0=$0\"\necho \"cwd=$(pwd)\"\necho FIXED\n",
    );

    std::os::unix::fs::symlink(&common, &launch).unwrap();
    assert_eq!(fs::read_link(&launch).unwrap(), common);

    // Play removes the idle symlink; fuse-overlayfs mounts at the same path.
    fs::remove_file(&launch).unwrap();
    let game = OverlayGame::new(&common, &fixes, Some("42")).unwrap();
    let mut overlay =
        LinuxOverlay::with_data_home(game.clone(), &data_home).with_mount_point(&launch);
    let merged = overlay.mount().unwrap().to_path_buf();
    assert_eq!(merged, launch);
    assert!(is_mountpoint(&launch));

    let output = Command::new(merged.join("run.sh"))
        .current_dir(merged.join("sub"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FIXED"), "{stdout}");
    assert!(
        stdout.contains(&format!("argv0={}", merged.join("run.sh").display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("cwd={}", merged.join("sub").display())),
        "{stdout}"
    );

    fs::write(merged.join("written-by-game.txt"), "upper").unwrap();
    assert!(!common.join("written-by-game.txt").exists());
    assert!(overlay.layout().upper.join("written-by-game.txt").is_file());

    overlay.unmount().unwrap();
    assert!(!is_mountpoint(&launch));
    assert!(!launch.exists());

    // Stop recreates the idle symlink at exactly the same path.
    std::os::unix::fs::symlink(&common, &launch).unwrap();
    assert_eq!(fs::read_link(&launch).unwrap(), common);
    assert_eq!(
        fs::read_to_string(launch.join("run.sh")).unwrap(),
        fs::read_to_string(common.join("run.sh")).unwrap()
    );
}

fn is_mountpoint(path: &Path) -> bool {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let path = path.to_string_lossy();
    mountinfo.lines().any(|line| {
        line.split(" - ")
            .next()
            .and_then(|prefix| prefix.split_whitespace().nth(4))
            .map(unescape_mountinfo)
            .is_some_and(|value| value == path)
    })
}

fn unescape_mountinfo(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let digits = &value[index + 1..index + 4];
            if digits.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
                if let Ok(decoded) = u8::from_str_radix(digits, 8) {
                    result.push(decoded as char);
                    index += 4;
                    continue;
                }
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
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
