use std::fmt;

/// WARNING: Only Blake3 is currently implemented for the time being.

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashKind {
    Blake3,
    Sha512,
    Sha256,
}

impl fmt::Display for HashKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Blake3 => write!(f, "Blake3"),
            Self::Sha512 => write!(f, "Sha512"),
            Self::Sha256 => write!(f, "Sha256"),
        }
    }
}

#[must_use]
pub fn hash(hash_kind: HashKind, data: &[u8]) -> String {
    match hash_kind {
        HashKind::Blake3 => blake3::hash(data).to_hex().to_string(),
        HashKind::Sha512 => todo!(),
        HashKind::Sha256 => todo!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_blake3() {
        let data = b"hello world";
        let hash = hash(HashKind::Blake3, data);
        // Blake3 hash of "hello world"
        assert_eq!(
            hash,
            "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );
    }
}
