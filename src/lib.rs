pub mod db;
pub mod model;
pub mod sync;
pub mod tailscale;

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

pub fn default_database_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("digital", "Eastern Kentucky Digital", "Tracker")
        .context("could not determine the operating system data directory")?;
    Ok(dirs.data_local_dir().join("tracker.db"))
}
