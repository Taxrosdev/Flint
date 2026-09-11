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

