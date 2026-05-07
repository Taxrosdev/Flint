pub mod build;

use anyhow::{Result, bail};
use std::{
    fs,
    io::{Cursor, Read},
    os::unix::fs::PermissionsExt,
    path::Path,
};

/// How big of "chunks" do we search for a tar?
/// Likely Tunable.
/// Standard/Recommended: 64kb
const CHUNK_SIZE: usize = 64 * 1024;
/// Should be about 2MB
/// To get this number, (Intended max chunk size) / `CHUNK_SIZE`
const MAX_CHUNKS: usize = 32;

/// Rips the tar from the header
///
/// # Errors
///
/// - Header got too large and gave up
pub fn get_tar(data: &[u8]) -> Result<Vec<u8>> {
    for idx in 0..MAX_CHUNKS {
        let initial_idx = idx * CHUNK_SIZE;
        // 5 is the length of 'ustar', 257 is a magic ustar appearance index for some reason.
        if let Some(slice) = data.get(initial_idx + 257..initial_idx + (257 + 5))
            && slice == b"ustar"
        {
            return Ok(data[initial_idx..].to_vec());
        }
    }

    bail!("Could not find chunk, are you running a raw header?")
}

/// Pads the header during buildtime
///
/// # Errors
///
/// - Header got too large and gave up
pub fn pad_header(mut header_data: Vec<u8>) -> Result<Vec<u8>> {
    for idx in 0..MAX_CHUNKS {
        if header_data.len() < idx * CHUNK_SIZE {
            header_data.resize(idx * CHUNK_SIZE, 4);

            println!("Padded header to size: {}kb", header_data.len() / 1024);

            return Ok(header_data);
        }
    }

    bail!("Header too large.")
}

/// Extract the bundle at `bundle_path` to `extract_path`, with `extract_path` as the `repo_path`
///
/// # Errors
///
/// - Invalid TAR
/// - Filesystem errors
///
/// # Panics
///
/// - Malformed TAR
/// - You've managed to use this really **really** badly.
pub fn extract_bundle(bundle_path: &Path, extract_path: &Path) -> Result<()> {
    let data = fs::read(bundle_path)?;
    let tar = get_tar(&data)?;
    let file = Cursor::new(tar);

    let mut archive = tar::Archive::new(file);

    for entry in archive.entries()? {
        let mut file = entry?;

        let header = file.header().clone();
        let path = header.path().unwrap();
        let mode = header.mode().unwrap();

        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;

        let extracted_path = extract_path.join(path);
        fs::create_dir_all(extracted_path.parent().unwrap())?;
        fs::write(&extracted_path, contents)?;

        let mut perms = fs::metadata(&extracted_path)?.permissions();
        perms.set_mode(mode & 0o777);
        fs::set_permissions(&extracted_path, perms)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_header() -> Result<()> {
        let data = vec![1, 2, 3];
        let padded = pad_header(data)?;
        assert_eq!(padded.len(), CHUNK_SIZE);
        assert_eq!(&padded[0..3], &[1, 2, 3]);
        assert!(padded[3..].iter().all(|&x| x == 4));
        Ok(())
    }
}
