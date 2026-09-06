use anyhow::Result;
use std::path::Path;
use temp_dir::TempDir;

use flintpkg::{
    build::BuildManifest,
    repo::{create_repo, get_installed_package},
    run::{install_package, start},
};

#[tokio::test]
#[ignore = "Really expensive, requires cloning from github and rebuilding flint."]
async fn full_workflow_test() -> Result<()> {
    let repo_dir = TempDir::new()?;
    let repo_path = repo_dir.path();
    let chunks_dir = TempDir::new()?;
    let chunks_path = chunks_dir.path();

    create_repo(repo_path, None)?;

    let build_manifest_path = Path::new("build_manifest.yml");
    let build_manifest = BuildManifest::load(build_manifest_path)?;
    build_manifest
        .build(build_manifest_path, repo_path, None, chunks_path)
        .await?;

    install_package(repo_path, "example", chunks_path).await?;

    let manifest = get_installed_package(repo_path, "example")?;

    let args: Vec<&str> = vec!["--help"];
    let result = start(repo_path, manifest.clone(), "flint", args)?;
    assert!(result.success());

    let args: Vec<&str> = vec![];
    let result = start(repo_path, manifest, "flint", args)?;
    assert!(!result.success());

    Ok(())
}
