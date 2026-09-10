use anyhow::{Context, Result, bail};
use dialoguer::{Select, theme::ColorfulTheme};
use std::{
    fs,
    path::{Path, PathBuf},
};

use flintpkg::{
    build::BuildManifest,
    chunks::{utils::clean_unused, verify_all_chunks},
    repo::{
        PackageManifest, get_package, read_manifest,
        versions::{get_versions, remove_version},
    },
    run::{install_package, start},
    utils::{resolve_package, resolve_repo},
};

pub async fn build_cmd(
    base_path: &Path,
    repo_name: &str,
    build_manifest_path: &Path,
    chunk_store_path: &Path,
    force: bool,
) -> Result<()> {
    let repo_path = resolve_repo(base_path, repo_name)?;
    let build_manifest = BuildManifest::load(build_manifest_path)?;

    if force {
        build_manifest
            .force_build(build_manifest_path, &repo_path, None, chunk_store_path)
            .await?;
    } else {
        build_manifest
            .build(build_manifest_path, &repo_path, None, chunk_store_path)
            .await?;
    }

    clean_unused(base_path, chunk_store_path)?;

    Ok(())
}

pub async fn install_cmd(
    base_path: &Path,
    repo_name: Option<String>,
    chunk_store_path: &Path,
    package_id: &str,
) -> Result<()> {
    let target_repo_path: PathBuf = if let Some(repo_name) = repo_name {
        resolve_repo(base_path, &repo_name)?
    } else {
        let possible_repos = resolve_package(base_path, package_id, |_| true)?;

        if possible_repos.len() > 1 {
            choose_repo(possible_repos)?.0
        } else if let Some(possible_repo) = possible_repos.first() {
            possible_repo.0.clone()
        } else {
            bail!("No Repositories contain that package.")
        }
    };

    install_package(&target_repo_path, package_id, chunk_store_path).await?;

    Ok(())
}

pub fn remove_cmd(base_path: &Path, repo_name: Option<String>, package_id: &str) -> Result<()> {
    let target_repo_path: PathBuf = if let Some(repo_name) = repo_name {
        resolve_repo(base_path, &repo_name)?
    } else {
        let possible_repos = resolve_package(base_path, package_id, |repo_path| {
            repo_path.join("installed").join(package_id).exists()
        })?;

        if possible_repos.len() > 1 {
            choose_repo(possible_repos)?.0
        } else if let Some(possible_repo) = possible_repos.first() {
            possible_repo.0.clone()
        } else {
            bail!("No Repositories contain that package.")
        }
    };

    fs::remove_dir_all(target_repo_path.join("installed").join(package_id))?;

    for version in get_versions(&target_repo_path, package_id)? {
        remove_version(&target_repo_path, &version, package_id)?;
    }

    Ok(())
}

#[cfg(feature = "network")]
pub async fn update_cmd(
    base_path: &Path,
    quicklaunch_path: &Path,
    chunk_store_path: &Path,
) -> Result<()> {
    use flintpkg::run::quicklaunch::update_quicklaunch;

    use crate::update_all_repos;

    update_all_repos(base_path, chunk_store_path).await?;

    update_quicklaunch(base_path, quicklaunch_path)?;
    clean_unused(base_path, chunk_store_path)
}

pub async fn run_cmd(
    path: &Path,
    repo_name: Option<String>,
    chunk_store_path: &Path,
    package: String,
    entrypoint: Option<String>,
    args: Option<Vec<String>>,
) -> Result<()> {
    let (target_repo_path, package_manifest) = if let Some(repo_name) = repo_name {
        // Resolve the path, and then read the package manifest
        let repo_path = resolve_repo(path, &repo_name)?;
        let repo_manifest = read_manifest(&repo_path)?;
        let package_manifest = get_package(&repo_manifest, &package)?;

        (repo_path, package_manifest)
    } else {
        let possible_repos = resolve_package(path, &package, |_| true)?;

        if possible_repos.len() > 1 {
            choose_repo(possible_repos)?
        } else if let Some(possible_repo) = possible_repos.first() {
            possible_repo.clone()
        } else {
            bail!("No Repositories contain that package.")
        }
    };

    let entrypoint = if let Some(e) = entrypoint {
        e
    } else {
        let first_command = package_manifest
            .commands
            .first()
            .context("Package has no commands defined")?;

        first_command
            .to_str()
            .context("First command path was not valid UTF-8")?
            .to_string()
    };

    // Install if not installed
    #[cfg(feature = "network")]
    if !target_repo_path
        .join("installed/")
        .join(&package)
        .join("install.meta")
        .exists()
    {
        install_package(&target_repo_path, &package, chunk_store_path)
            .await
            .with_context(|| "Failed to install package.")?;
    }

    start(
        &target_repo_path,
        package_manifest,
        &entrypoint,
        &args.unwrap_or_default(),
    )?;

    Ok(())
}

pub fn verify_cmd(base_path: &Path, repo_name: &str, chunk_store_path: &Path) -> Result<()> {
    let target_repo_path = resolve_repo(base_path, repo_name)?;
    verify_all_chunks(&target_repo_path, chunk_store_path)?;
    clean_unused(base_path, chunk_store_path)
}

/// Lets the user choose a Repository from a list
fn choose_repo(
    possible_repos: Vec<(PathBuf, PackageManifest)>,
) -> Result<(PathBuf, PackageManifest)> {
    let items: Vec<String> = possible_repos
        .iter()
        .map(|(path, manifest)| {
            format!(
                "{} ({} {})",
                path.file_name().unwrap().to_string_lossy(),
                manifest.metadata.title.clone().unwrap_or_default(),
                manifest.metadata.version.clone().unwrap_or_default()
            )
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Multiple repositories contain this package, pick one")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(possible_repos.into_iter().nth(selection).unwrap())
}
