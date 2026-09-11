use anyhow::{Context, Result};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use crate::chunks::{Chunk, HashKind, get_chunk_filename, hash::hash};

/// Turns a filesystem tree into a list of chunks
///
/// # Errors
///
/// - Filesystem out of space (Very likely)
///
/// # Panics
///
/// - If `tree_path` points to a file, but the file somehow has no parent (eg: is root).
pub fn save_tree(
    tree_path: &Path,
    chunk_store_path: &Path,
    hash_kind: HashKind,
) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();

    if !chunk_store_path.exists() {
        fs::create_dir_all(chunk_store_path)?;
    }

    if tree_path.is_file() {
        let path: PathBuf = tree_path.file_name().unwrap().into();
        let contents = fs::read(tree_path)?;
        let size = (contents.len() as u64) / 1024;
        let hash = hash(hash_kind, &contents);
        let mode = fs::metadata(tree_path)?.permissions().mode() & 0o777;

        let chunk_path = &chunk_store_path.join(get_chunk_filename(&hash, mode));
        if fs::hard_link(tree_path, chunk_path).is_err() {
            fs::write(chunk_path, contents)?;
        }

        chunks.push(Chunk {
            hash,
            path,
            size,
            permissions: mode,
        });
    } else {
        for entry in WalkDir::new(tree_path) {
            let file = entry?;

            if !file.file_type().is_file() {
                continue;
            }

            let path = file.path().strip_prefix(tree_path)?.to_path_buf();
            let contents = fs::read(file.path())?;
            let size = (contents.len() as u64) / 1024;
            let hash = hash(hash_kind, &contents);
            let mode = file.metadata()?.permissions().mode() & 0o777;

            let chunk_path = &chunk_store_path.join(get_chunk_filename(&hash, mode));
            if fs::hard_link(file.path(), chunk_path).is_err() {
                fs::write(chunk_path, contents)?;
            }

            chunks.push(Chunk {
                hash,
                path,
                size,
                permissions: mode,
            });
        }
    }

    Ok(chunks)
}

/// Turns a list of chunks into a filesystem tree
/// Will delete the tree on failure, preventing a partially installed state to persist.
///
/// # Errors
///
/// - Filesystem (Out of space, Permissions)
pub fn load_tree(load_path: &Path, chunk_store_path: &Path, chunks: &[Chunk]) -> Result<()> {
    let result = load_tree_unsafe(load_path, chunk_store_path, chunks);

    if result.is_err() {
        fs::remove_dir_all(load_path)?;
    }

    result
}

/// Turns a list of chunks into a filesystem tree
///
/// # Errors
///
/// - Filesystem (Out of space, Permissions)
pub fn load_tree_unsafe(load_path: &Path, chunk_store_path: &Path, chunks: &[Chunk]) -> Result<()> {
    for chunk in chunks {
        let extracted_path = load_path.join(&chunk.path);
        let chunk_path = chunk_store_path.join(get_chunk_filename(&chunk.hash, chunk.permissions));

        // Create parent path
        if let Some(parent) = extracted_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if extracted_path.exists() {
            fs::remove_file(&extracted_path)?;
        }

        fs::hard_link(&chunk_path, &extracted_path)
            .or_else(|_| fs::copy(&chunk_path, &extracted_path).map(|_| ()))
            .with_context(|| "Could not copy data while extracting")?;

        let mut perms = fs::metadata(&extracted_path)?.permissions();
        perms.set_mode(chunk.permissions & 0o777);
        fs::set_permissions(&extracted_path, perms)?;
    }

    Ok(())
}

/// Installs all chunks in a tree
///
/// # Errors
///
/// - The network returns invalid/malicious data
/// - Filesystem errors (Out of space, Permissions)
#[cfg(feature = "network")]
pub async fn install_tree(
    chunks: &[Chunk],
    chunk_store_path: &Path,
    mirrors: &[String],
    hash_kind: HashKind,
) -> Result<()> {
    use crate::chunks::network::install_chunks;

    let mut not_installed_chunks = Vec::new();

    for chunk in chunks {
        let chunk_path = chunk_store_path.join(get_chunk_filename(&chunk.hash, chunk.permissions));
        if !chunk_path.exists() {
            not_installed_chunks.push(chunk);
        }
    }

    install_chunks(&not_installed_chunks, mirrors, hash_kind, chunk_store_path).await?;

    Ok(())
}

/// Returns the tree's estimated size in kilobytes.
#[must_use]
pub fn estimate_tree_size(chunks: &[Chunk]) -> u64 {
    let mut size: u64 = 0;

    for chunk in chunks {
        size += chunk.size;
    }

    size
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;

    use temp_dir::TempDir;

    #[test]
    fn test_save_tree() -> Result<()> {
        let initial_tree_path = TempDir::new()?;
        let chunk_store_path = TempDir::new()?;
        let hash_kind = HashKind::Blake3;

        // Create example tree
        fs::write(initial_tree_path.path().join("file"), "Example")?;
        fs::create_dir(initial_tree_path.path().join("path"))?;
        fs::write(initial_tree_path.path().join("path/file"), "Example2")?;

        let chunks = save_tree(initial_tree_path.path(), chunk_store_path.path(), hash_kind)?;

        // Check that the correct number of chunks were created
        assert_eq!(chunks.len(), 2);

        // Check that the chunk hashes exist in the chunk store
        for chunk in &chunks {
            let chunk_path = chunk_store_path
                .path()
                .join(get_chunk_filename(&chunk.hash, chunk.permissions));
            assert!(
                chunk_path.exists(),
                "Chunk file does not exist: {chunk_path:?}"
            );
        }

        // Check that the chunk paths are correct
        let chunk_paths: Vec<_> = chunks
            .iter()
            .map(|c| c.path.to_string_lossy().to_string())
            .collect();
        assert!(chunk_paths.contains(&"file".to_string()));
        assert!(chunk_paths.contains(&"path/file".to_string()));

        // Check that the estimated size is correct (in KB)
        let expected_size = (b"Example".len() as u64) / 1024 + (b"Example2".len() as u64) / 1024;
        assert_eq!(estimate_tree_size(&chunks), expected_size);

        Ok(())
    }

    #[test]
    fn test_load_tree() -> Result<()> {
        let initial_tree_path = TempDir::new()?;
        let loaded_tree_path = TempDir::new()?;
        let chunk_store_path = TempDir::new()?;
        let hash_kind = HashKind::Blake3;

        // Create example tree
        fs::write(initial_tree_path.path().join("file"), "Example")?;
        fs::create_dir(initial_tree_path.path().join("path"))?;
        fs::write(initial_tree_path.path().join("path/file"), "Example2")?;

        let chunks = save_tree(initial_tree_path.path(), chunk_store_path.path(), hash_kind)?;

        load_tree(loaded_tree_path.path(), chunk_store_path.path(), &chunks)?;

        assert_eq!(
            fs::read_to_string(loaded_tree_path.path().join("file"))?,
            "Example"
        );

        assert_eq!(
            fs::read_to_string(loaded_tree_path.path().join("path/file"))?,
            "Example2"
        );

        Ok(())
    }

    #[test]
    fn test_permissions() -> Result<()> {
        let initial_tree_path = TempDir::new()?;
        let loaded_tree_path = TempDir::new()?;
        let chunk_store_path = TempDir::new()?;
        let hash_kind = HashKind::Blake3;

        // Create example tree
        let file_path = initial_tree_path.path().join("file");
        fs::write(&file_path, "Example")?;
        let mut perms = fs::metadata(&file_path)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&file_path, perms)?;

        let file_path = initial_tree_path.path().join("file2");
        fs::write(&file_path, "Example")?;
        let mut perms2 = fs::metadata(&file_path)?.permissions();
        perms2.set_mode(0o600);
        fs::set_permissions(&file_path, perms2)?;

        let chunks = save_tree(initial_tree_path.path(), chunk_store_path.path(), hash_kind)?;

        load_tree(loaded_tree_path.path(), chunk_store_path.path(), &chunks)?;

        assert_eq!(
            fs::metadata(loaded_tree_path.path().join("file"))?.mode() & 0o777,
            0o700
        );

        assert_eq!(
            fs::metadata(loaded_tree_path.path().join("file2"))?.mode() & 0o777,
            0o600
        );

        Ok(())
    }

    #[test]
    fn test_tree_size() -> Result<()> {
        let initial_tree_path = TempDir::new()?;
        let chunk_store_path = TempDir::new()?;
        let hash_kind = HashKind::Blake3;

        let kb1 = vec![0; 1024];
        let kb4 = vec![0; 4096];

        // Create example tree
        fs::write(initial_tree_path.path().join("file"), kb1)?;
        fs::create_dir(initial_tree_path.path().join("path"))?;
        fs::write(initial_tree_path.path().join("path/file"), kb4)?;

        let chunks = save_tree(initial_tree_path.path(), chunk_store_path.path(), hash_kind)?;

        // Check that the estimated size is correct (in KB)
        assert_eq!(estimate_tree_size(&chunks), 5);

        Ok(())
    }
}
