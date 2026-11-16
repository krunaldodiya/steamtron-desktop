//! Process helpers shared by the desktop and core crates.

use std::ffi::OsStr;
use std::process::Command;

/// Create a command that never opens a console window on Windows.
///
/// Windows GUI applications otherwise briefly flash a console window for every
/// helper process they start (`reg`, `tasklist`, `taskkill`, PowerShell, ...).
/// The flag is a no-op on other platforms.
#[allow(unused_mut)]
pub fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
