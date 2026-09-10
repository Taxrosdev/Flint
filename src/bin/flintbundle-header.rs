use anyhow::{Context, Result};
use flintpkg::{
    bundle::extract_bundle,
    repo::read_manifest,
    run::{sandbox::ExitReason, start},
};
use std::{
    env::{self, current_exe},
    process::exit,
};
use temp_dir::TempDir;

fn main() -> Result<()> {
    println!("BUNDLE");
    let bundle_path = current_exe()?;
    let extract_path = TempDir::new()?;
    let repo_path = extract_path.path();
    extract_bundle(&bundle_path, repo_path)
        .with_context(|| "Could not read bundles tar contents")?;

    let manifest = read_manifest(repo_path)?;
    // These have been validated to be there by the builder
    let package_manifest = manifest.packages.first().unwrap();
    let entrypoint = package_manifest.commands.first().unwrap();

    let exit_code = start(
        repo_path,
        package_manifest.clone(),
        entrypoint.to_str().unwrap(),
        &env::args().collect::<Vec<_>>(),
    )
    .with_context(|| "Could not run bundle")?;

    match exit_code {
        ExitReason::Code(code) => exit(code),
        ExitReason::Signal(sig) => exit(sig),
    }
}
