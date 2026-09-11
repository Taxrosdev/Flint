pub mod quicklaunch;
pub mod sandbox;

use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

#[cfg(feature = "network")]
use crate::chunks::install_tree;
use crate::{
    repo::{
        PackageManifest, get_package, read_manifest,
        versions::{install_version, switch_version},
    },
    run::sandbox::{ExitReason, Mount, Sandbox},
};

/// Starts a package from an entrypoint
///
/// # Errors
///
/// - Specified an entrypoint that doesn't exist
/// - Filesystem errors (Out of space, Permissions)
/// - Invalid Repository/Package manifest
/// - Package is not installed
pub fn start<S: AsRef<OsStr>>(
    repo_path: &Path,
    package_manifest: PackageManifest,
    entrypoint: &str,
    args: &[S],
) -> Result<ExitReason> {
    let installed_path = &repo_path.join("installed").join(&package_manifest.id);
    if !installed_path.try_exists()? {
        bail!(
            "Package is not installed. Try using flint install {}",
            package_manifest.id
        )
    }

    // Get all matching commands
    let matches: Vec<&PathBuf> = package_manifest
        .commands
        .iter()
        .filter(|command| command.ends_with(entrypoint))
        .collect();

    // Make sure theres at least a single match
    if let Some(entrypoint) = matches.first() {
        // Allow build_manifests to have a / at the start of entrypoints, eg: /bin/bash
        let entrypoint = entrypoint.to_string_lossy();
        let entrypoint: &str = entrypoint.trim_start_matches('/');

        let mut envs: HashMap<OsString, OsString> = package_manifest.env.unwrap_or_default();

        // Insert the INSTALL_PATH env.
        envs.insert("INSTALL_PATH".into(), installed_path.into());

        // Prepare the sandbox
        let mut sandbox = Sandbox::create()?;
        let exec = if let Some(root_path) = package_manifest.sandbox.root_dir {
            sandbox.add_mount(Mount {
                host_path: installed_path.to_owned(),
                sandbox_path: root_path.clone(),
            });
            root_path.join(entrypoint)
        } else {
            installed_path.join(entrypoint)
        };

        // Actually run the command
        let status = sandbox.run_sandboxed(exec.as_os_str(), args, &envs)?;

        Ok(status)
    } else {
        bail!("Entrypoint does not exist.")
    }
}

/// Installs the latest version of a package, assumes all chunks are available.
/// Will automatically autoclean.
///
/// # Errors
///
/// - Filesystem errors (Out of space, Permissions)
/// - Invalid Repository/Package manifest
/// - Network Errors (If network is enabled)
pub async fn install_package(
    repo_path: &Path,
    package_id: &str,
    chunk_store_path: &Path,
) -> Result<()> {
    let repo_manifest = read_manifest(repo_path)?;

    let package_manifest = get_package(&repo_manifest, package_id)
        .with_context(|| "Failed to get package from Repository.")?;
    // Don't just use an alias but actually resolve into a correct package id
    let package_id = &package_manifest.id;

    // Get any chunks that are not installed
    #[cfg(feature = "network")]
    install_tree(
        &package_manifest.chunks,
        chunk_store_path,
        &repo_manifest.mirrors,
        repo_manifest.hash_kind,
    )
    .await
    .with_context(|| "Failed to install package.")?;

    let hash = install_version(repo_path, package_id, chunk_store_path)?;

    switch_version(repo_path, &hash, package_id)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::save_tree;
    use crate::repo::{Metadata, SandboxConfig, create_repo, insert_package};
    use std::fs;
    use temp_dir::TempDir;

    #[tokio::test]
    async fn test_install() -> Result<()> {
        let repo_dir = TempDir::new()?;
        let repo_path = repo_dir.path();
        let chunks_dir = TempDir::new()?;
        let chunks_path = chunks_dir.path();

        create_repo(repo_path, Some(repo_path))?;

        // Create a temp tree
        let temp_tree = TempDir::new()?;
        fs::write(temp_tree.path().join("file1"), "content1")?;
        fs::create_dir(temp_tree.path().join("dir"))?;
        fs::write(temp_tree.path().join("dir/file2"), "content2")?;
        let chunks = save_tree(
            temp_tree.path(),
            chunks_path,
            crate::chunks::HashKind::Blake3,
        )?;

        let package = PackageManifest {
            id: "testpkg".to_string(),
            aliases: vec![],
            metadata: Metadata {
                title: Some("Test".to_string()),
                description: None,
                homepage_url: None,
                version: None,
                license: None,
            },
            chunks,
            commands: Vec::new(),
            env: None,
            // TODO!
            build_hash: "TODO".to_string(),
            sandbox: SandboxConfig::default(),
        };

        // Insert package
        insert_package(&package, repo_path, Some(repo_path))?;

        // Now install
        install_package(repo_path, "testpkg", chunks_path).await?;

        // Check installed
        let installed_path = repo_path.join("installed/testpkg");
        assert!(installed_path.exists());
        assert!(installed_path.join("file1").exists());
        assert!(installed_path.join("dir/file2").exists());
        assert!(installed_path.join("install.meta").exists());

        Ok(())
    }
}
