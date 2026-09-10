use std::{collections::HashMap, path::PathBuf};

use crate::chunks::{Chunk, HashKind};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct RepoManifest {
    pub metadata: Metadata,
    pub packages: Vec<PackageManifest>,
    pub public_key: String,
    pub mirrors: Vec<String>,
    pub edition: String,
    pub hash_kind: HashKind,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub metadata: Metadata,
    pub id: String,
    pub aliases: Vec<String>,
    pub chunks: Vec<Chunk>,
    pub commands: Vec<PathBuf>,
    /// Runtime environment variables
    pub env: Option<HashMap<String, String>>,
    #[serde(default = "build_hash_default")]
    pub build_hash: String,

    #[serde(default)]
    pub sandbox: SandboxConfig,
}

/// All of these are user visible, and should carry no actual weight.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub homepage_url: Option<String>,
    /// User visible, not actually used to compare versions
    pub version: Option<String>,
    /// SPDX Identifier
    pub license: Option<String>,
}

/// For optional (but recommended) sandboxxing.
///
/// Even projects that do not want to limit their software in any way will find this useful, as it
/// will allow mounting user installed dirs to stable paths (ie: /app) and allow the linker to run.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct SandboxConfig {
    /// Where to mount the installed package.
    /// Examples include /usr, /usr/local, /app, etc.
    pub root_dir: Option<PathBuf>,
}

fn build_hash_default() -> String {
    "uninitialized".to_string()
}
