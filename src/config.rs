use anyhow::{Context, Result};
use directories::BaseDirs;
use std::fs;
use std::path::PathBuf;

/// Gets the default/main configuration directory
///
/// # Errors
///
/// - No valid home directory path could be retrieved from the operating system.
/// - Config dir could not be created
pub fn get_config_dir() -> Result<PathBuf> {
    // Locate XDG config directory
    let base_dirs = BaseDirs::new().context("Could not find user directories")?;
    let config_dir: PathBuf = base_dirs.config_dir().join("flint");

    if !&config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    Ok(config_dir)
}

/// Gets the user repos directory
///
/// # Errors
///
/// - No valid home directory path could be retrieved from the operating system.
/// - Repositorys dir could not be created
pub fn get_user_repos_dir() -> Result<PathBuf> {
    let repos_dir = get_user_data_dir()?.join("repos");

    if !repos_dir.exists() {
        fs::create_dir_all(&repos_dir)?;
    }

    Ok(repos_dir)
}

/// Gets the system repos directory
///
/// # Errors
///
/// - Repositorys dir could not be created
pub fn get_system_repos_dir() -> Result<PathBuf> {
    let repos_dir = get_system_data_dir().join("repos");

    if !repos_dir.exists() {
        fs::create_dir_all(&repos_dir)
            .with_context(|| "Could not create system data dir. Try sudo?")?;
    }

    Ok(repos_dir)
}

/// Gets the user chunks directory
///
/// # Errors
///
/// - No valid home directory path could be retrieved from the operating system.
/// - Chunks dir could not be created
pub fn get_user_chunks_dir() -> Result<PathBuf> {
    let chunks_dir = get_user_data_dir()?.join("chunks");

    if !chunks_dir.exists() {
        fs::create_dir_all(&chunks_dir)?;
    }

    Ok(chunks_dir)
}

/// Gets the system chunks directory
///
/// # Errors
///
/// - Chunks dir could not be created
pub fn get_system_chunks_dir() -> Result<PathBuf> {
    let chunks_dir = get_system_data_dir().join("chunks");

    if !chunks_dir.exists() {
        fs::create_dir_all(&chunks_dir)
            .with_context(|| "Could not create system data dir. Try sudo?")?;
    }

    Ok(chunks_dir)
}

/// Gets the system-wide quicklaunch path
///
/// # Errors
///
/// - Quicklaunch dir could not be created
pub fn get_system_quicklaunch_dir() -> Result<PathBuf> {
    let quicklaunch_dir = get_system_data_dir().join("quicklaunch");

    if !quicklaunch_dir.exists() {
        fs::create_dir_all(&quicklaunch_dir)
            .with_context(|| "Could not create quicklaunch dir. Try sudo?")?;
    }

    Ok(quicklaunch_dir)
}

/// Gets the users quicklaunch directory
///
/// # Errors
///
/// - No valid home directory path could be retrieved from the operating system.
/// - Quicklaunch dir could not be created
pub fn get_user_quicklaunch_dir() -> Result<PathBuf> {
    let quicklaunch_dir = get_user_data_dir()?.join("quicklaunch");

    if !quicklaunch_dir.exists() {
        fs::create_dir_all(&quicklaunch_dir)
            .with_context(|| "Could not create quicklaunch dir. Try sudo?")?;
    }

    Ok(quicklaunch_dir)
}

/// Gets the build cache directory
///
/// # Errors
///
/// - No valid home directory path could be retrieved from the operating system.
/// - Cache dir could not be created
pub fn get_build_cache_dir() -> Result<PathBuf> {
    // Locate XDG config directory
    let base_dirs = BaseDirs::new().context("Could not find user directories")?;
    let build_cache_dir: PathBuf = base_dirs.cache_dir().join("flint");

    if !&build_cache_dir.exists() {
        fs::create_dir_all(&build_cache_dir)?;
    }

    Ok(build_cache_dir)
}

#[must_use]
/// Gets the SYSTEM-WIDE Repositorys path
fn get_system_data_dir() -> PathBuf {
    cfg_select! {
        unix => PathBuf::from("/var/lib/flint"),
        target_os = "macos" => PathBuf::from("/Library/Application Support/flint"),
        target_os = "windows" => PathBuf::from(r"C:\ProgramData\Flint")
    }
}

/// Gets the user data directory
///
/// # Errors
///
/// - No valid home directory path could be retrieved from the operating system.
fn get_user_data_dir() -> Result<PathBuf> {
    // Locate XDG data directory
    let base_dirs = BaseDirs::new().context("Could not find user directories")?;
    let data_dir: PathBuf = base_dirs.data_dir().join("flint");

    if !&data_dir.exists() {
        fs::create_dir_all(&data_dir)?;
    }

    Ok(data_dir)
}
