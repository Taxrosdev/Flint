pub mod hash;
mod sources;

use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use temp_dir::TempDir;

use crate::{
    chunks::{load_tree, save_tree},
    crypto::key::{get_private_key, serialize_verifying_key},
    repo::{Metadata, PackageManifest, get_package, insert_package, read_manifest},
};
use hash::calc_build_hash;
use sources::get_sources;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct BuildManifest {
    /// ID of this package, the main alias
    id: String,
    /// Aliases for installation
    #[serde(default)]
    aliases: Vec<String>,
    /// Package Metadata
    metadata: Metadata,
    /// A list of commands that this will give access to
    #[serde(default)]
    commands: Vec<PathBuf>,
    /// Directory relative to the manifest
    directory: PathBuf,
    /// Edition
    edition: String,
    /// Script to be run before packaging
    build_script: Option<PathBuf>,
    /// Script to be run after `build_script` but before packaging
    post_script: Option<PathBuf>,
    /// Sources to pull when building
    sources: Option<Vec<Source>>,
    /// ``SubPackages`` to be included directly into the output AND at build time.
    include: Option<Vec<String>>,
    /// ``SubPackages`` to be included directly ONLY at build time.
    /// Useful for SDKs.
    sdks: Option<Vec<String>>,
    /// RUNTIME environment variables
    env: Option<HashMap<String, String>>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct Source {
    /// Should either be git, tar or local
    kind: String,
    /// URL to the source.
    url: String,
    /// Path to extract.
    path: Option<String>,
    /// Git commit to use
    commit: Option<String>,
}

/// Builds and inserts a package into a Repository from a `build_manifest`
///
/// # Errors
///
/// - Filesystem (Out of Space, Permissions)
/// - Build Script Failure
pub async fn build(
    build_manifest_path: &Path,
    repo_path: &Path,
    config_path: Option<&Path>,
    chunk_store_path: &Path,
) -> Result<PackageManifest> {
    let repo = read_manifest(repo_path)?;
    let build_manifest: BuildManifest =
        serde_yaml::from_str(&fs::read_to_string(build_manifest_path)?)?;

    if let Ok(package) = get_package(&repo, &build_manifest.id) {
        let next_build_hash = calc_build_hash(build_manifest_path, repo_path)?;
        if package.build_hash == next_build_hash {
            return Ok(package);
        }
    }

    let our_public_key = serialize_verifying_key(get_private_key(None)?.verifying_key())?;
    if repo.public_key != our_public_key {
        bail!(
            "You do not have the correct signing key to resign this Repository.\nIf you are certain, use --force, but be aware you will not be able to update this Repository from the remote source again."
        );
    }

    force_build(
        build_manifest_path,
        repo_path,
        config_path,
        chunk_store_path,
    )
    .await
}

/// Builds and inserts a package into a Repository from a `build_manifest`
///
/// # Errors
///
/// - Filesystem (Out of Space, Permissions)
/// - Build Script Failure
pub async fn force_build(
    build_manifest_path: &Path,
    repo_path: &Path,
    config_path: Option<&Path>,
    chunk_store_path: &Path,
) -> Result<PackageManifest> {
    let build_dir = TempDir::new()?;
    let build_manifest_path = &build_manifest_path.canonicalize()?;

    let build_manifest: BuildManifest =
        serde_yaml::from_str(&fs::read_to_string(build_manifest_path)?)?;

    let repo_manifest =
        read_manifest(repo_path).with_context(|| "The target Repostiory does not exist")?;

    let search_path = &build_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("/"));

    if let Some(sources) = build_manifest.sources {
        get_sources(build_dir.path(), search_path, &sources).await?;
    }

    let mut envs = build_manifest.env.unwrap_or_default();

    if let Some(packages) = &build_manifest.include {
        include_all(
            packages,
            search_path,
            build_dir.path(),
            repo_path,
            chunk_store_path,
            &mut envs,
        )?;
    }

    if let Some(packages) = &build_manifest.sdks {
        include_all(
            packages,
            search_path,
            build_dir.path(),
            repo_path,
            chunk_store_path,
            &mut envs,
        )?;
    }

    if let Some(script) = build_manifest.build_script {
        run_script(build_dir.path(), search_path, &script).with_context(|| "build_script")?;
    }

    let out_dir = build_dir.path().join(&build_manifest.directory);

    if let Some(script) = build_manifest.post_script {
        run_script(&out_dir, search_path, &script).with_context(|| "post_script")?;
    }

    let mut included_chunks = Vec::new();
    if let Some(packages) = &build_manifest.include {
        for dependency in packages {
            include(
                search_path,
                dependency,
                &out_dir,
                repo_path,
                chunk_store_path,
            )?;
        }
    }

    let chunks = save_tree(&out_dir, chunk_store_path, repo_manifest.hash_kind)?;

    included_chunks.extend(chunks);

    let mut package_manifest = PackageManifest {
        aliases: build_manifest.aliases,
        commands: build_manifest.commands,
        id: build_manifest.id,
        metadata: build_manifest.metadata,
        chunks: included_chunks,
        env: None,
        build_hash: calc_build_hash(build_manifest_path, repo_path)?,
    };

    if !envs.is_empty() {
        package_manifest.env = Some(envs);
    }

    insert_package(&package_manifest, repo_path, config_path)?;

    Ok(package_manifest)
}

fn include_all(
    packages: &Vec<String>,
    search_path: &Path,
    build_dir: &Path,
    repo_path: &Path,
    chunk_store_path: &Path,
    envs: &mut HashMap<String, String>,
) -> Result<()> {
    for dependency in packages {
        let result = include(
            search_path,
            dependency,
            build_dir,
            repo_path,
            chunk_store_path,
        )?;

        envs.extend(result);
    }

    Ok(())
}

/// This requires the dependency to be build first
// Perhaps a future improvement would be to recursively build if not already built? (TODO)
fn include(
    search_path: &Path,
    dependency: &str,
    path_to_include_at: &Path,
    repo_path: &Path,
    chunk_store_path: &Path,
) -> Result<HashMap<String, String>> {
    let dependency_build_manifest_path = search_path.join(dependency);
    let dependency_build_manifest: BuildManifest =
        serde_yaml::from_str(&fs::read_to_string(dependency_build_manifest_path)?)?;
    let repo_manifest = read_manifest(repo_path)?;
    let dependency_manifest = get_package(&repo_manifest, &dependency_build_manifest.id)?;

    load_tree(
        path_to_include_at,
        chunk_store_path,
        &dependency_manifest.chunks,
    )?;

    Ok(dependency_manifest.env.unwrap_or_default())
}

/// Runs a script (typically `post_script` or `build_script`)
fn run_script(cwd: &Path, search_path: &Path, script: &Path) -> Result<()> {
    let script_path = search_path.join(script);

    let result = Command::new("sh")
        .arg("-c")
        .arg(script_path)
        .current_dir(cwd)
        .status()?;

    if !result.success() {
        bail!("Build script failed.")
    }

    Ok(())
}
