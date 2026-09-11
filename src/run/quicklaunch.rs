use anyhow::{Context, Result, anyhow};
use std::{
    collections::HashSet,
    env::{current_exe, var_os},
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::repo::read_manifest;

/// Removes all nonexistant Quicklaunch items, and adds any missing ones.
///
/// # Errors
///
/// - Filesystem
/// - Bad Repositories
pub fn update_quicklaunch(repos_path: &Path, quicklaunch_path: &Path) -> Result<()> {
    let mut allowed = HashSet::new();
    let executable_path = if let Some(env) = var_os("INSTALL_PATH") {
        PathBuf::from(env).join("flint")
    } else {
        current_exe()
            .with_context(|| "Could not get current executable path")?
            .canonicalize()?
    };

    for entry in repos_path.read_dir()? {
        let repo_path = entry?.path();

        let manifest = read_manifest(&repo_path)?;

        // Write all quicklaunch scripts. This has the unintended sideffect of rewriting existing
        // quicklaunch scripts.
        for package in manifest.packages {
            for entrypoint in package.commands {
                let command = entrypoint
                    .file_name()
                    .ok_or_else(|| anyhow!("Could not get entrypoint name"))?;

                allowed.insert(command.to_owned());

                let mut tmp_filename = PathBuf::from(command);
                tmp_filename.set_extension("new");
                let tmp_path = &quicklaunch_path.join(tmp_filename);
                let path = quicklaunch_path.join(command);

                // generate quicklaunch script
                let quicklaunch_script = format!(
                    "#!/bin/bash\n{} run {} -- {} $@",
                    executable_path.display(),
                    package.id,
                    command.display()
                );

                fs::write(tmp_path, quicklaunch_script)?;
                fs::rename(tmp_path, &path)?;

                // chmod 755
                let mut perms = fs::metadata(&path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&path, perms)?;
            }
        }
    }

    // delete quicklaunch scripts for removed packages
    for entry in quicklaunch_path.read_dir()? {
        let file = entry?;

        if !allowed.contains(&file.file_name()) {
            fs::remove_file(file.path())?;
        }
    }

    println!("Updated quicklaunch");

    Ok(())
}
