use anyhow::Result;
use std::{fs, path::Path};

use crate::BundleCommands;
use flintpkg::{bundle::build::build_bundle, utils::resolve_repo};

pub fn bundle_commands(base_path: &Path, command: BundleCommands) -> Result<()> {
    match command {
        BundleCommands::Extract => todo!(),
        BundleCommands::Create {
            repo_name,
            bundle_path,
            header_path,
        } => {
            let bundle = build_bundle(&header_path, &resolve_repo(base_path, &repo_name)?)?;
            fs::write(bundle_path, bundle)?;
        }
    }

    Ok(())
}
