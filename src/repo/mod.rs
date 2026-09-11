mod io;
#[cfg(feature = "network")]
pub mod network;
mod types;
pub mod versions;
pub use io::{read_manifest, update_manifest};
pub use types::*;

use anyhow::{Result, bail};
use std::fs::create_dir_all;
use std::{fs, path::Path};

use crate::chunks::HashKind;
use crate::crypto::key::{get_private_key, serialize_verifying_key};
use crate::crypto::signing::sign;

/// Creates a repository at `repo_path`
///
/// # Errors
///
/// - File permission errors at `repo_path`
/// - Key generation errors (If you do not already have a key)
pub fn create_repo(repo_path: &Path, config_path: Option<&Path>) -> Result<()> {
    if repo_path.join("manifest.yml").exists() {
        bail!("Repository Already exists")
    }
    create_dir_all(repo_path)?;

    let manifest = RepoManifest {
        edition: "2025".into(),
        hash_kind: HashKind::Blake3,
        metadata: Metadata {
            title: None,
            description: None,
            homepage_url: None,
            version: None,
            license: None,
        },
        mirrors: Vec::new(),
        packages: Vec::new(),
        public_key: serialize_verifying_key(get_private_key(config_path)?.verifying_key())?,
    };

    let manifest_serialized = serde_yaml::to_string(&manifest)?;
    fs::write(repo_path.join("manifest.yml"), &manifest_serialized)?;

    sign(repo_path, &manifest_serialized, config_path)?;

    Ok(())
}

/// Inserts a package into a local repository.
///
/// # Errors
/// - Repo not signed with local signature
pub fn insert_package(
    package_manifest: &PackageManifest,
    repo_path: &Path,
    config_path: Option<&Path>,
) -> Result<()> {
    let _ = remove_package(&package_manifest.id, repo_path, config_path);

    let mut repo_manifest = read_manifest(repo_path)?;

    let mut packages: Vec<PackageManifest> = repo_manifest.packages;

    for package in &packages {
        if package.aliases.contains(&package_manifest.id) {
            bail!(
                "A package in this repo has an alias with that package id already: {}",
                package.id
            )
        }
        for alias in &package_manifest.aliases {
            if &package.id == alias || package.aliases.contains(alias) {
                bail!(
                    "A package in this repo has an alias with that package id already: {}",
                    package.id
                )
            }
        }
    }

    packages.push(package_manifest.clone());
    repo_manifest.packages = packages;

    let repo_manifest_serialized = serde_yaml::to_string(&repo_manifest)?;

    let signature = sign(repo_path, &repo_manifest_serialized, config_path)?;
    update_manifest(repo_path, &repo_manifest_serialized, &signature.to_bytes())?;

    Ok(())
}

/// Removes a package from a local repository.
///
/// # Errors
/// - Repo not signed with local signature
/// - Filesystem errors
pub fn remove_package(
    package_id: &str,
    repo_path: &Path,
    config_path: Option<&Path>,
) -> Result<()> {
    let mut repo_manifest = read_manifest(repo_path)?;

    repo_manifest
        .packages
        .retain(|package| package.id != package_id);

    let repo_manifest_serialized = serde_yaml::to_string(&repo_manifest)?;

    let signature = sign(repo_path, &repo_manifest_serialized, config_path)?;
    update_manifest(repo_path, &repo_manifest_serialized, &signature.to_bytes())?;

    Ok(())
}

/// Gets a package manifest from a repository.
///
/// # Errors
///
/// - Filesystem errors (Permissions most likely)
/// - Repository doesn't exist
/// - ID doesn't exist inside the Repository
pub fn get_package(repo_manifest: &RepoManifest, package_id: &str) -> Result<PackageManifest> {
    repo_manifest
        .packages
        .iter()
        .find(|package| package.matches_id(package_id))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!("Could not find package '{package_id}' found in Repository.")
        })
}

/// Gets an installed package manifest from a repository.
///
/// # Errors
///
/// - Filesystem errors (Permissions most likely)
/// - Repository doesn't exist
/// - ID doesn't exist inside the Repository
/// - Package isn't installed
pub fn get_installed_package(repo_path: &Path, id: &str) -> Result<PackageManifest> {
    let repo_manifest = read_manifest(repo_path)?;

    // Check ID's and aliases
    for package in repo_manifest.packages {
        if package.matches_id(id) {
            let installed_path = repo_path
                .join("installed")
                .join(&package.id)
                .join("install.meta");
            if !installed_path.exists() {
                bail!("Package '{}' is not installed.", package.id)
            }

            let package_manifest_serialized = fs::read_to_string(installed_path)?;
            let package_manifest: PackageManifest =
                serde_yaml::from_str(&package_manifest_serialized)?;

            return Ok(package_manifest);
        }
    }

    bail!("No package found in Repository.");
}

/// Lists all packages from a repository.
///
/// # Errors
///
/// - Filesystem errors (Permissions most likely)
/// - Repository doesn't exist
pub fn get_all_packages(repo_path: &Path) -> Result<Vec<PackageManifest>> {
    let repo_manifest = read_manifest(repo_path)?;

    Ok(repo_manifest.packages)
}

/// Lists all installed packages from a repository.
///
/// # Errors
///
/// - Filesystem errors (Permissions most likely)
/// - Repository doesn't exist
pub fn get_all_installed_packages(repo_path: &Path) -> Result<Vec<PackageManifest>> {
    let mut packages = Vec::new();
    let installed_path = &repo_path.join("installed");

    if installed_path.exists() {
        // Check ID's and aliases
        for entry in fs::read_dir(installed_path)? {
            let file = entry?.path();
            let package = serde_yaml::from_str(&fs::read_to_string(file.join("install.meta"))?)?;

            packages.push(package);
        }

        Ok(packages)
    } else {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use temp_dir::TempDir;

    use super::*;

    #[test]
    fn insert_and_get_and_remove_package() -> Result<()> {
        // Create repo
        let repo = TempDir::new()?;
        let repo_path = repo.path();
        create_repo(repo_path, Some(repo_path))?;

        // Make sure errors on no package
        assert!(get_package(&read_manifest(repo_path)?, "test").is_err());

        let package_manifest = PackageManifest {
            aliases: vec!["example_alias".into()],
            id: "test".into(),
            chunks: vec![],
            commands: vec![],
            metadata: Metadata {
                title: None,
                description: None,
                homepage_url: None,
                version: None,
                license: None,
            },
            env: None,
            build_hash: "Example Build Hash".to_string(),
            sandbox: SandboxConfig::default(),
        };

        insert_package(&package_manifest, repo_path, Some(repo_path))?;
        assert!(get_package(&read_manifest(repo_path)?, "test").is_ok());
        assert!(insert_package(&package_manifest, repo_path, Some(repo_path)).is_ok());

        remove_package(&package_manifest.id, repo_path, Some(repo_path))?;
        assert!(get_package(&read_manifest(repo_path)?, "test").is_err());

        Ok(())
    }

    #[test]
    fn test_create_and_read() {
        let tmp = TempDir::new().unwrap();
        let repo_path = tmp.path();

        // Create repo
        create_repo(repo_path, Some(repo_path)).unwrap();

        // Read unsigned manifest
        let manifest = read_manifest(repo_path).unwrap();
        assert_eq!(manifest.edition, "2025");
        assert!(manifest.public_key.len() > 10);
        assert!(manifest.packages.is_empty());

        // Should have manifest.yml + .sig
        assert!(repo_path.join("manifest.yml").exists());
        assert!(repo_path.join("manifest.yml.sig").exists());
    }

    #[test]
    fn test_tampered_manifest_fails() -> Result<()> {
        let tmp = TempDir::new()?;
        let repo_path = tmp.path();
        create_repo(repo_path, Some(repo_path))?;

        // Tamper with manifest.yml
        let mut contents = fs::read_to_string(repo_path.join("manifest.yml"))?;
        contents.push_str("\n# sneaky hacker change");
        fs::write(repo_path.join("manifest.yml"), contents)?;

        let result = read_manifest(repo_path);

        assert!(
            result.is_err(),
            "tampered manifest should fail verification"
        );

        Ok(())
    }
}
