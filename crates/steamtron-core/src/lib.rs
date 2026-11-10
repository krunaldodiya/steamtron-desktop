pub mod config;
pub mod import;
pub mod launch;
pub mod paths;
pub mod runtime;
pub mod library;
pub mod fixes;
pub mod steam;
pub mod play_history;
pub mod prelaunch;

use std::path::PathBuf;

pub fn data_home() -> PathBuf {
    paths::layout().data_home
}

pub fn steamtron_home() -> PathBuf {
    paths::layout().home()
}
