use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uuid([u8; 16]);

#[derive(Debug)]
pub enum UuidError {
    Rng(getrandom::Error),
}

impl fmt::Display for UuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UuidError::Rng(e) => write!(f, "failed to read random bytes from CSPRNG: {e}"),
        }
    }
}

impl std::error::Error for UuidError {}

impl Uuid {
    pub fn try_v4() -> Result<Uuid, UuidError> {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).map_err(UuidError::Rng)?;
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Ok(Uuid(bytes))
    }

    pub fn v4() -> Uuid {
        match Self::try_v4() {
            Ok(u) => u,
            Err(e) => crate::unrecoverable!("failed to get random bytes from CSPRNG", e),
        }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    #[must_use]
    pub fn short(&self) -> String {
        let b = &self.0;
        format!("{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3],
            b[4], b[5],
            b[6], b[7],
            b[8], b[9],
            b[10], b[11], b[12], b[13], b[14], b[15],
        )
    }
}
