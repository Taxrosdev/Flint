use crate::build::{Source, SourceKind};
use anyhow::Context;
use anyhow::Result;
use std::fs;
use std::path::Path;
#[cfg(feature = "network")]
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;

pub async fn get_sources(path: &Path, source_path: &Path, sources: &[Source]) -> Result<()> {
    for source in sources {
        match source.kind {
            SourceKind::Git => pull_git(source, path).with_context(|| {
                format!("Failed to pull git repo from {}", source_path.display())
            })?,

            #[cfg(feature = "network")]
            SourceKind::Tar => pull_tar(source, path).await.with_context(|| {
                format!(
                    "Failed to extract tar archive from {}",
                    source_path.display()
                )
            })?,

            SourceKind::Local => pull_local(source_path, path).with_context(|| {
                format!("Failed to copy local source from {}", source_path.display())
            })?,
        }
    }

    Ok(())
}

/// `move_files` will just move instead of copying.
fn copy_tree(source: &Path, target: &Path, move_files: bool) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let relative_path = entry.path().strip_prefix(source)?;
        let output = if relative_path.as_os_str().is_empty() {
            target.to_path_buf()
        } else {
            target.join(relative_path)
        };

        match entry.file_type() {
            file_type if file_type.is_dir() => fs::create_dir_all(output)?,
            file_type if file_type.is_file() || file_type.is_symlink() => {
                fs::create_dir_all(output.parent().context("File has no parent")?)?;
                if move_files {
                    fs::rename(entry.path(), output)?;
                } else {
                    fs::copy(entry.path(), output)?;
                }
            }
            _ => {} // Impossible
        }
    }

    Ok(())
}

/// Just copy files from a local path into the target.
/// If target already exists, nuke it first.
fn pull_local(source_path: &Path, target_path: &Path) -> Result<()> {
    // Remove target if it already exists
    if target_path.exists() {
        fs::remove_dir_all(target_path)
            .with_context(|| format!("Failed to remove existing dir {}", target_path.display()))?;
    }

    fs::create_dir_all(target_path)
        .with_context(|| format!("Failed to create target dir {}", target_path.display()))?;

    copy_tree(source_path, target_path, false)
}

/// Clone or pull a git repo depending on whether it already exists.
fn pull_git(source: &Source, target_path: &Path) -> Result<()> {
    // Clone fresh
    let status = Command::new("git")
        .arg("clone")
        .arg(&source.url)
        .arg(target_path)
        .status()
        .with_context(|| "Failed to run git clone")?;
    if !status.success() {
        anyhow::bail!("git clone failed");
    }

    if let Some(commit) = &source.commit {
        let status = Command::new("git")
            .arg("checkout")
            .arg(commit)
            .current_dir(target_path)
            .status()
            .with_context(|| "Failed to run git clone")?;
        if !status.success() {
            anyhow::bail!("git checkout failed");
        }
    }

    Ok(())
}

/// Extract tar contents to target dir without the toplevel dir
fn unwrap_tar_contents(temp_dir: &Path, target_path: &Path) -> Result<()> {
    let entries: Vec<_> = fs::read_dir(temp_dir)?.collect::<std::io::Result<_>>()?;

    match entries.as_slice() {
        [entry] if entry.file_type()?.is_dir() => copy_tree(&entry.path(), target_path, true),
        [entry] => {
            let source_file = entry.path();
            let file_name = source_file.file_name().unwrap();

            copy_tree(&source_file, &target_path.join(file_name), false)
        }
        entries => {
            for entry in entries {
                let source_path = entry.path();
                let file_name = source_path.file_name().unwrap();
                let destination_path = target_path.join(file_name);

                copy_tree(&source_path, &destination_path, false)?;
            }
            Ok(())
        }
    }
}

#[cfg(feature = "network")]
async fn pull_tar(source: &Source, target_path: &Path) -> Result<()> {
    use anyhow::bail;
    use bzip2::read::BzDecoder;
    use flate2::read::GzDecoder;
    use liblzma::read::XzDecoder;
    use std::fs::File;
    use std::io::Read;
    use tar::Archive;
    use temp_dir::TempDir;

    // downloads/gets the cache
    let get_cache_path = try_pull_cache(&source.url).await?;
    let get_cache = File::open(get_cache_path)?;

    // make sure nothings already there
    // incase extracting to mpc in gcc for example
    if target_path.exists() {
        fs::remove_dir_all(target_path)?;
    }

    fs::create_dir_all(target_path)?;

    // Create a temporary directory to unpack the tar
    let temp_dir = TempDir::new()?;

    if let Some(extension) = Path::new(&source.url).extension() {
        // Detect compression by extension
        let tar: Box<dyn Read> =
            if extension.eq_ignore_ascii_case("gz") || extension.eq_ignore_ascii_case("tgz") {
                Box::new(GzDecoder::new(get_cache))
            } else if extension.eq_ignore_ascii_case("xz") {
                Box::new(XzDecoder::new(get_cache))
            } else if extension.eq_ignore_ascii_case("bz2") {
                Box::new(BzDecoder::new(get_cache))
            } else {
                Box::new(get_cache)
            };

        let mut archive = Archive::new(tar);
        archive
            .unpack(temp_dir.path())
            .with_context(|| format!("Failed to unpack gzip tar from {}", source.url))?;

        // Extract and handle the tar contents
        unwrap_tar_contents(temp_dir.path(), target_path)?;

        // Autotools is very annoying.
        fix_dir_times(target_path)?;

        Ok(())
    } else {
        bail!("No extension on tar source url.")
    }
}

#[cfg(feature = "network")]
async fn try_pull_cache(url: &str) -> Result<PathBuf> {
    use crate::config::get_build_cache_dir;
    use blake3::hash;

    // example path: $HOME/.cache/flint/0823unrb98e7f8972b958573129v857hn92385
    let cache_str = hash(url.as_bytes()).to_string();
    let cache_path = get_build_cache_dir()?.join(cache_str);

    // Download it
    if !cache_path.exists() {
        let res = reqwest::get(url)
            .await
            .with_context(|| format!("Failed to fetch tarball from {url}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error fetching {url}"))?;

        let bytes = res
            .bytes()
            .await
            .with_context(|| "Failed to read response body")?;

        fs::write(&cache_path, bytes)?;
    }

    Ok(cache_path)
}

fn fix_dir_times(path: &Path) -> std::io::Result<()> {
    use filetime::{FileTime, set_file_mtime};
    for entry in fs::read_dir(path)? {
        let file = entry?;
        let path = file.path();

        if path.is_dir() {
            fix_dir_times(&path)?;

            // find latest child mtime
            let mut latest = FileTime::from_unix_time(0, 0);
            for child in fs::read_dir(&path)? {
                let child = child?;
                let meta = child.metadata()?;
                let mtime = FileTime::from_last_modification_time(&meta);

                if mtime > latest {
                    latest = mtime;
                }
            }

            set_file_mtime(&path, latest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use temp_dir::TempDir;

    #[test]
    fn test_pull_local() -> Result<()> {
        let source_temp = TempDir::new()?;
        let target_temp = TempDir::new()?;

        // Create source files
        fs::write(source_temp.path().join("file1.txt"), "content1")?;
        fs::create_dir(source_temp.path().join("subdir"))?;
        fs::write(source_temp.path().join("subdir/file2.txt"), "content2")?;

        pull_local(source_temp.path(), target_temp.path())?;

        // Check copied
        assert!(target_temp.path().join("file1.txt").exists());
        assert!(target_temp.path().join("subdir/file2.txt").exists());
        assert_eq!(
            fs::read_to_string(target_temp.path().join("file1.txt"))?,
            "content1"
        );
        assert_eq!(
            fs::read_to_string(target_temp.path().join("subdir/file2.txt"))?,
            "content2"
        );

        Ok(())
    }

    #[test]
    fn test_extract_tar_contents_single_directory_unwrap() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let target_dir = TempDir::new()?;

        // One top-level dir
        let single_dir = &temp_dir.path().join("project");
        fs::create_dir(single_dir)?;
        fs::write(single_dir.join("file1.txt"), "content1")?;
        fs::create_dir(single_dir.join("subdir"))?;
        fs::write(single_dir.join("subdir/file2.txt"), "content2")?;

        unwrap_tar_contents(temp_dir.path(), target_dir.path())?;

        // Should unwrap the single directory
        assert!(target_dir.path().join("file1.txt").exists());
        assert!(target_dir.path().join("subdir/file2.txt").exists());
        assert!(!target_dir.path().join("project").exists());

        assert_eq!(
            fs::read_to_string(target_dir.path().join("file1.txt"))?,
            "content1"
        );
        assert_eq!(
            fs::read_to_string(target_dir.path().join("subdir/file2.txt"))?,
            "content2"
        );

        Ok(())
    }

    #[test]
    // I hate any reason why this may be required, but unfortunately it may be.
    fn test_extract_tar_contents_single_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let target_dir = TempDir::new()?;

        fs::write(temp_dir.path().join("single"), "content")?;

        unwrap_tar_contents(temp_dir.path(), target_dir.path())?;

        // Should copy the single file directly
        assert!(target_dir.path().join("single").exists());
        assert_eq!(
            fs::read_to_string(target_dir.path().join("single"))?,
            "content"
        );

        Ok(())
    }

    #[test]
    fn test_extract_tar_contents_multiple_entries_no_unwrap() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let target_dir = TempDir::new()?;

        fs::write(temp_dir.path().join("file1.txt"), "content1")?;
        fs::create_dir(temp_dir.path().join("dir1"))?;
        fs::write(temp_dir.path().join("dir1/file2.txt"), "content2")?;
        fs::write(temp_dir.path().join("file3.txt"), "content3")?;

        unwrap_tar_contents(temp_dir.path(), target_dir.path())?;

        // Should copy all files without unwrapping
        assert!(target_dir.path().join("file1.txt").exists());
        assert!(target_dir.path().join("dir1/file2.txt").exists());
        assert!(target_dir.path().join("file3.txt").exists());

        assert_eq!(
            fs::read_to_string(target_dir.path().join("file1.txt"))?,
            "content1"
        );
        assert_eq!(
            fs::read_to_string(target_dir.path().join("dir1/file2.txt"))?,
            "content2"
        );
        assert_eq!(
            fs::read_to_string(target_dir.path().join("file3.txt"))?,
            "content3"
        );

        Ok(())
    }

}
