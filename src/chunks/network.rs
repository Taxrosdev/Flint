use crate::chunks::{Chunk, HashKind, get_chunk_filename, hash::hash};
use anyhow::{Result, bail};
use futures_util::{StreamExt, TryStreamExt};
use reqwest;
use std::{fs, path::Path};

/// Installs a particular chunk from a particular mirror
///
/// # Errors
///
/// - The internet sent back corrupt/malicious data, timed out, or is blatently not working.
/// - Filesystem out of space
pub async fn install_chunk(
    chunk: &Chunk,
    mirror: &str,
    hash_kind: HashKind,
    chunk_store_path: &Path,
) -> Result<()> {
    let chunk_name = get_chunk_filename(&chunk.hash, chunk.permissions);
    let chunk_path = chunk_store_path.join(&chunk_name);
    let tmp_chunk_path = chunk_store_path.join(format!("{chunk_name}.tmp"));

    if chunk_path.exists() {
        return Ok(());
    }

    let url = format!("{mirror}/chunks/{chunk_name}");
    let request = reqwest::get(url).await?;
    let body = request.bytes().await?;

    let hash = hash(hash_kind, &body);

    if hash == chunk.hash {
        // TODO: POTENTIAL ISSUE IF MULTIPLE PROCESSES TRY INSTALLING SAME CHUNK!
        if tmp_chunk_path.exists() {
            fs::remove_file(&tmp_chunk_path)?;
        }
        fs::write(&tmp_chunk_path, body)?;
        fs::rename(&tmp_chunk_path, &chunk_path)?;

        Ok(())
    } else {
        bail!("Invalid chunk data returned.")
    }
}

/// Installs all chunks from a list of mirrors
/// NOTE: Chunks will be installed out of order, and any mirror potentially.
///
/// # Errors
///
/// - The internet sent back corrupt/malicious data, timed out, or is blatently not working.
/// - Filesystem out of space
pub async fn install_chunks(
    chunks: &[&Chunk],
    mirrors: &[String],
    hash_kind: HashKind,
    chunk_store_path: &Path,
) -> Result<()> {
    fs::create_dir_all(chunk_store_path)?;

    tokio_stream::iter(chunks.iter()) // clone so each task owns its Chunk
        .map(|chunk| {
            let mirrors = mirrors.to_vec();
            let chunk_store_path = chunk_store_path.to_path_buf();

            async move {
                println!("Downloading chunk {}", chunk.hash);

                for mirror in mirrors {
                    match install_chunk(chunk, &mirror, hash_kind, &chunk_store_path).await {
                        Ok(()) => {
                            println!("Downloaded chunk {}", chunk.hash);
                            return Ok(());
                        }
                        Err(err) => {
                            eprintln!(
                                "Failed to fetch chunk {} from mirror {mirror}: {err}",
                                chunk.hash
                            );
                        }
                    }
                }

                bail!("All mirrors failed for chunk {}", chunk.hash);
            }
        })
        .buffer_unordered(8) // run up to 8 downloads at once
        .try_collect::<()>() // fail-fast on first error
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::path::PathBuf;
    use temp_dir::TempDir;
    use tokio::runtime::Runtime;

    fn run_async_test<F: std::future::Future<Output = ()>>(f: F) {
        let rt = Runtime::new().unwrap();
        rt.block_on(f);
    }

    #[test]
    fn test_install_chunk_success() {
        run_async_test(async {
            let temp_dir = TempDir::new().unwrap();
            let chunk_store_path = temp_dir.path();

            let data = b"hello world";
            let hash_kind = HashKind::Blake3;
            let hash = hash(hash_kind, data);

            let chunk = Chunk {
                hash,
                path: PathBuf::new(),
                size: 1,
                permissions: 0o644,
            };

            // Mock server
            let server = MockServer::start();
            let _mock = server.mock(|when, then| {
                when.path(format!(
                    "/chunks/{}",
                    get_chunk_filename(&chunk.hash, chunk.permissions)
                ));
                then.status(200).body(data);
            });

            // Run function
            install_chunk(&chunk, &server.base_url(), hash_kind, chunk_store_path)
                .await
                .unwrap();

            // Verify file exists
            let path = chunk_store_path.join(get_chunk_filename(&chunk.hash, chunk.permissions));
            let saved = fs::read(path).unwrap();
            assert_eq!(saved, data);
        });
    }

    #[test]
    fn test_install_chunk_corrupt_data() {
        run_async_test(async {
            let temp_dir = TempDir::new().unwrap();
            let chunk_store_path = temp_dir.path();

            let good_data = b"hello world";
            let bad_data = b"garbage";

            let hash_kind = HashKind::Blake3;
            let hash = hash(hash_kind, good_data);

            let chunk = Chunk {
                hash,
                path: PathBuf::new(),
                size: 1,
                permissions: 0o644,
            };

            let server = MockServer::start();
            let _mock = server.mock(|when, then| {
                when.path(format!(
                    "/chunks/{}",
                    get_chunk_filename(&chunk.hash, chunk.permissions)
                ));
                then.status(200).body(bad_data);
            });

            let result =
                install_chunk(&chunk, &server.base_url(), hash_kind, chunk_store_path).await;

            assert!(result.is_err(), "Expected corrupt data to fail");
        });
    }

    #[test]
    fn test_install_chunks_with_fallback_mirrors() {
        run_async_test(async {
            let temp_dir = TempDir::new().unwrap();
            let chunk_store_path = temp_dir.path();

            let data = b"mirror test";
            let hash_kind = HashKind::Blake3;
            let hash = hash(hash_kind, data);

            let chunk = Chunk {
                hash,
                path: PathBuf::new(),
                size: 1,
                permissions: 0o644,
            };

            // Bad mirror (returns nonsense)
            let bad_server = MockServer::start();
            let _bad_mock = bad_server.mock(|when, then| {
                when.any_request();
                then.status(200).body("garbage");
            });

            // Good mirror
            let good_server = MockServer::start();
            let _good_mock = good_server.mock(|when, then| {
                when.path(format!(
                    "/chunks/{}",
                    get_chunk_filename(&chunk.hash, chunk.permissions)
                ));
                then.status(200).body(data);
            });

            // Run function
            install_chunks(
                std::slice::from_ref(&&chunk),
                &[bad_server.base_url(), good_server.base_url()],
                hash_kind,
                chunk_store_path,
            )
            .await
            .unwrap();

            // Verify saved
            let path = chunk_store_path.join(get_chunk_filename(&chunk.hash, chunk.permissions));
            let saved = fs::read(path).unwrap();
            assert_eq!(saved, data);
        });
    }
}
