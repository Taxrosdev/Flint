use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::Path;

use super::BuildManifest;
use crate::repo::{get_package, read_manifest};

impl BuildManifest {
    /// Requires all dependencies to be built and in the Repository beforehand.
    ///
    /// # Errors
    ///
    /// - Scripts do not exist
    /// - Invalid build manifest
    pub fn build_hash(&self, repo_path: &Path) -> Result<String> {
        let repo_manifest = read_manifest(repo_path)?;

        let mut hash = blake3::Hasher::new();

        // Raw serialized version of `BuildManifest`
        let raw = serde_yaml::to_string(&self).unwrap();
        hash.write_all(raw.as_bytes())?;

        // Hash the `includes`
        for dep in &self.include {
            let package = get_package(&repo_manifest, dep)?;
            hash.write_all(package.build_hash.as_bytes())?;
        }

        // Hash the `sdks`
        for dep in &self.sdks {
            let package = get_package(&repo_manifest, dep)?;
            hash.write_all(package.build_hash.as_bytes())?;
        }

        // Hash the `build_script`
        if let Some(build_script) = &self.build_script {
            let script = fs::read_to_string(build_script)?;
            hash.write_all(script.as_bytes())?;
        }

        // Hash the `post_script`
        if let Some(post_script) = &self.post_script {
            let script = fs::read_to_string(post_script)?;
            hash.write_all(script.as_bytes())?;
        }

        Ok(hash.finalize().to_string())
    }
}
