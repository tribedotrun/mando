use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uuid([u8; 16]);

#[derive(Debug)]
pub enum UuidError {
    Rng(getrandom::Error),
    #[cfg(test)]
    InvalidLength,
    #[cfg(test)]
    InvalidFormat,
    #[cfg(test)]
    InvalidHex,
}

impl fmt::Display for UuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UuidError::Rng(e) => write!(f, "failed to read random bytes from CSPRNG: {e}"),
            #[cfg(test)]
            UuidError::InvalidLength => write!(f, "invalid UUID length"),
            #[cfg(test)]
            UuidError::InvalidFormat => write!(f, "invalid UUID format (expected 8-4-4-4-12)"),
            #[cfg(test)]
            UuidError::InvalidHex => write!(f, "invalid hex character in UUID"),
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

    #[cfg(test)]
    pub fn parse(s: &str) -> Result<Uuid, UuidError> {
        if s.len() != 36 {
            return Err(UuidError::InvalidLength);
        }
        let b = s.as_bytes();
        if b[8] != b'-' || b[13] != b'-' || b[18] != b'-' || b[23] != b'-' {
            return Err(UuidError::InvalidFormat);
        }
        let hex: String = s.chars().filter(|c| *c != '-').collect();
        if hex.len() != 32 {
            return Err(UuidError::InvalidFormat);
        }
        let mut bytes = [0u8; 16];
        for i in 0..16 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|_| UuidError::InvalidHex)?;
        }
        Ok(Uuid(bytes))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_format_is_valid() {
        let id = Uuid::v4();
        let s = id.to_string();
        assert_eq!(s.len(), 36);
        let parts: Vec<&str> = s.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn v4_version_nibble_is_4() {
        let id = Uuid::v4();
        assert_eq!(id.as_bytes()[6] >> 4, 4);
    }

    #[test]
    fn v4_variant_bits_are_10xx() {
        let id = Uuid::v4();
        assert_eq!(id.as_bytes()[8] >> 6, 0b10);
    }

    #[test]
    fn parse_round_trip() {
        let id = Uuid::v4();
        let s = id.to_string();
        let parsed = Uuid::parse(&s).unwrap();
        assert_eq!(id, parsed);
        assert_eq!(s, parsed.to_string());
    }

    #[test]
    fn two_v4_are_different() {
        let a = Uuid::v4();
        let b = Uuid::v4();
        assert_ne!(a, b);
    }

    #[test]
    fn parse_invalid_length() {
        let err = Uuid::parse("too-short").unwrap_err();
        assert!(matches!(err, UuidError::InvalidLength));
    }

    #[test]
    fn parse_invalid_format_no_hyphens() {
        let err = Uuid::parse("1234567890123456789012345678901234ab").unwrap_err();
        assert!(matches!(err, UuidError::InvalidFormat));
    }

    #[test]
    fn parse_invalid_hex() {
        let err = Uuid::parse("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz").unwrap_err();
        assert!(matches!(err, UuidError::InvalidHex));
    }

    #[test]
    fn display_is_lowercase() {
        let id = Uuid::parse("AABBCCDD-1122-4334-8556-667788990011").unwrap();
        let s = id.to_string();
        assert_eq!(s, "aabbccdd-1122-4334-8556-667788990011");
    }

    #[test]
    fn short_is_8_hex_chars() {
        let id = Uuid::v4();
        let s = id.short();
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
        let full = id.to_string().replace('-', "");
        assert_eq!(s, &full[..8]);
    }

    #[test]
    fn as_bytes_returns_inner() {
        let id = Uuid::v4();
        let bytes = id.as_bytes();
        assert_eq!(bytes.len(), 16);
        assert_eq!(bytes, &id.0);
    }

    #[test]
    fn error_display_messages() {
        assert_eq!(UuidError::InvalidLength.to_string(), "invalid UUID length");
        assert_eq!(
            UuidError::InvalidFormat.to_string(),
            "invalid UUID format (expected 8-4-4-4-12)"
        );
        assert_eq!(
            UuidError::InvalidHex.to_string(),
            "invalid hex character in UUID"
        );
    }
}
