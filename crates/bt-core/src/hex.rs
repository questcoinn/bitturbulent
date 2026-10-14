//! Lowercase hex encoding and case-insensitive decoding for fixed-size digests.

use std::fmt;

/// Error returned when a hex string cannot be parsed into a digest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseHashError {
    /// The string does not have exactly `expected` characters.
    #[error("expected {expected} hex characters, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    /// The character at `index` (counted in characters) is not a hex digit.
    #[error("invalid hex character at index {index}")]
    InvalidChar { index: usize },
}

/// Writes `bytes` as lowercase hex.
pub(crate) fn encode(bytes: &[u8], f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for b in bytes {
        write!(f, "{b:02x}")?;
    }
    Ok(())
}

/// Parses exactly `2 * N` hex characters (either case) into `N` bytes.
pub(crate) fn decode<const N: usize>(s: &str) -> Result<[u8; N], ParseHashError> {
    let expected = N * 2;
    let actual = s.chars().count();
    if actual != expected {
        return Err(ParseHashError::InvalidLength { expected, actual });
    }

    let mut out = [0u8; N];
    for (index, c) in s.chars().enumerate() {
        let nibble = c
            .to_digit(16)
            .ok_or(ParseHashError::InvalidChar { index })? as u8;
        let byte = &mut out[index / 2];
        *byte = (*byte << 4) | nibble;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_mixed_case() {
        assert_eq!(decode::<2>("aBcD"), Ok([0xab, 0xcd]));
    }

    #[test]
    fn rejects_short_and_long_input() {
        assert_eq!(
            decode::<20>(&"0".repeat(39)),
            Err(ParseHashError::InvalidLength {
                expected: 40,
                actual: 39
            })
        );
        assert_eq!(
            decode::<20>(&"0".repeat(41)),
            Err(ParseHashError::InvalidLength {
                expected: 40,
                actual: 41
            })
        );
    }

    #[test]
    fn rejects_non_hex_char() {
        let mut s = "0".repeat(40);
        s.replace_range(7..8, "g");
        assert_eq!(
            decode::<20>(&s),
            Err(ParseHashError::InvalidChar { index: 7 })
        );
    }

    #[test]
    fn rejects_non_ascii_char_without_panicking() {
        let s = format!("{}é", "0".repeat(39));
        assert_eq!(
            decode::<20>(&s),
            Err(ParseHashError::InvalidChar { index: 39 })
        );
    }
}
