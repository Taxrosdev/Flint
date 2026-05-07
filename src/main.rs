mod commands;
mod log;

use anyhow::Result;
use clap::{Parser, Subcommand};
#[cfg(feature = "network")]
use std::path::Path;
use std::{env::var_os, path::PathBuf};

use crate::commands::main_commands;
use flintpkg::config::{
    get_system_chunks_dir, get_system_quicklaunch_dir, get_system_repos_dir, get_user_chunks_dir,
    get_user_quicklaunch_dir, get_user_repos_dir,
};

/// Simple program to greet a person
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Install system-wide (requires root)
    #[arg(long, conflicts_with = "user")]
    system: bool,

    /// Install for current user only
    #[arg(long, conflicts_with = "system")]
    user: bool,

    /// Override the flint root
    #[arg(long)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Interact with Repositories
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },
    /// Builds a package from a local manifest and directory
    Build {
        build_manifest_path: PathBuf,
        repo_name: String,
        #[arg(long, short)]
        force: bool,
    },
    /// Install a package
    Install {
        /// The Repository to install from
        #[arg(long)]
        repo_name: Option<String>,
        /// The package to install
        package: String,
    },
    /// Remove an installed package
    Remove {
        /// The Repository to remove from
        #[arg(long)]
        repo_name: Option<String>,
        package: String,
    },
    /// Interact with bundles
    Bundle {
        #[command(subcommand)]
        command: BundleCommands,
    },
    #[cfg(feature = "network")]
    /// Updates a repository and its packages
    Update,
    /// Run a package's entrypoint
    Run {
        /// The Repository the package is in
        #[arg(long)]
        repo_name: Option<String>,
        /// The package to install
        package: String,
        /// The entrypoint in question. Will default to the first entrypoint
        entrypoint: Option<String>,
        /// Extra arguments
        args: Option<Vec<String>>,
    },
    /// Verify all chunks in a repository
    VerifyChunks {
        /// The Repository to verify chunks for
        #[arg(long)]
        repo_name: String,
    },
    /// Removes all not currently installed chunks, even if they are still in the Repository
    Clean,
}

#[derive(Subcommand)]
enum RepoCommands {
    /// Creates a new Repository locally
    Create { repo_name: String },
    /// List all Repositories
    List,
    /// Add a Repository from a remote url
    #[cfg(feature = "network")]
    Add {
        repo_name: String,
        remote_url: String,
    },
    /// Remove a Repository
    Remove { repo_name: String },
    /// Update a Repositories Metadata
    Update {
        #[arg(long)]
        homepage_url: Option<String>,
        #[arg(long)]
        license: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        /// Comma seperated list of all mirrors
        mirrors: Option<String>,

        repo_name: String,
    },
    /// Remove a Package from this Repository
    RemovePackage {
        repo_name: String,
        package_id: String,
    },
}

#[derive(Subcommand)]
enum BundleCommands {
    /// Extract a bundle into a Repository
    Extract,
    /// Extract a package from a Repository into a bundle
    Create {
        repo_name: String,
        bundle_path: PathBuf,
        header_path: PathBuf,
    },
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Scope {
    User,
    System,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let scope = if args.system {
        Scope::System
    } else {
        Scope::User
    };

    let base_path = &if let Some(sysroot) = args.root {
        sysroot
    } else if scope == Scope::User {
        get_user_repos_dir()?
    } else {
        get_system_repos_dir()?
    };

    let quicklaunch_path = &if scope == Scope::User {
        get_user_quicklaunch_dir()?
    } else {
        get_system_quicklaunch_dir()?
    };

    let chunk_store_path = &if scope == Scope::User {
        get_user_chunks_dir()?
    } else {
        get_system_chunks_dir()?
    };

    main_commands(base_path, quicklaunch_path, chunk_store_path, args.command).await?;

    if let Some(path) = var_os("PATH")
        && !path
            .to_string_lossy()
            .contains(&*quicklaunch_path.to_string_lossy())
    {
        log::add_to_path_notice(quicklaunch_path);
    }

    Ok(())
}

#[cfg(feature = "network")]
async fn update_all_repos(base_path: &Path, chunk_store_path: &Path) -> Result<()> {
    use crate::log::{skipped_update_repo, updated_package, updated_repo};
    use flintpkg::repo::{
        get_all_installed_packages, get_package, network::update_repository, read_manifest,
        remove_package,
    };
    use flintpkg::run::install_package;

    for entry in base_path.read_dir()? {
        let repo = entry?;
        let repo_path = repo.path();
        let repo_name = repo.file_name();

        let has_changed = update_repository(&repo_path).await?;

        if has_changed {
            updated_repo(&repo_name);
        } else {
            skipped_update_repo(&repo_name);
        }

        let repo_manifest = read_manifest(&repo_path)?;

        for installed_package in get_all_installed_packages(&repo_path)? {
            if let Ok(repo_package) = get_package(&repo_manifest, &installed_package.id) {
                if installed_package != repo_package {
                    updated_package(&repo_package);

                    install_package(&repo_path, &repo_package.id, chunk_store_path).await?;
                }
            } else {
                remove_package(&installed_package.id, &repo_path, None)?;
            }
        }
    }

    Ok(())
}
