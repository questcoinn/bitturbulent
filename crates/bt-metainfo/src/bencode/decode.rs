//! Recursive-descent bencode decoder with input limits.

use super::{Kind, Value};

/// Upper bounds applied while decoding.
///
/// The defaults match qBittorrent's preferences (`preferences.cpp:323-351`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest accepted input, in bytes. Default: 100 MiB.
    pub max_size: usize,
    /// Deepest accepted list/dictionary nesting. Default: 100.
    ///
    /// The decoder recurses once per nesting level, so a very large value
    /// (tens of thousands) can overflow the thread's stack.
    pub max_depth: usize,
    /// Most values (integers, strings, lists, dictionaries and dictionary
    /// keys) accepted in one input. Default: 10,000,000.
    pub max_tokens: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_size: 100 * 1024 * 1024,
            max_depth: 100,
            max_tokens: 10_000_000,
        }
    }
}

/// Error returned when input is not valid bencode or exceeds a [`Limits`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at byte {offset}")]
pub struct DecodeError {
    offset: usize,
    kind: DecodeErrorKind,
}

impl DecodeError {
    pub(crate) fn new(offset: usize, kind: DecodeErrorKind) -> Self {
        Self { offset, kind }
    }

    /// Byte offset in the input where the problem was found.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// What went wrong.
    pub fn kind(&self) -> DecodeErrorKind {
        self.kind
    }
}

/// The reason a [`DecodeError`] was returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeErrorKind {
    /// The input ended in the middle of a value, or a string claims more
    /// bytes than remain.
    #[error("unexpected end of input")]
    UnexpectedEof,
    /// A byte that cannot start or continue a value at this position.
    #[error("invalid byte")]
    InvalidByte,
    /// An integer outside the signed 64-bit range.
    #[error("integer out of range")]
    IntOverflow,
    /// A dictionary key that is not a byte string.
    #[error("dictionary key is not a byte string")]
    NonStringKey,
    /// Nesting deeper than [`Limits::max_depth`].
    #[error("nesting depth limit exceeded")]
    DepthLimit,
    /// More values than [`Limits::max_tokens`].
    #[error("token limit exceeded")]
    TokenLimit,
    /// Input larger than [`Limits::max_size`].
    #[error("input size limit exceeded")]
    SizeLimit,
}

/// Decodes one bencode value from the start of `input` with default
/// [`Limits`]. Bytes after the value are ignored.
pub fn decode(input: &[u8]) -> Result<Value<'_>, DecodeError> {
    decode_with_limits(input, Limits::default())
}

/// Decodes one bencode value from the start of `input`. Bytes after the
/// value are ignored; [`Value::raw`] tells how many were consumed.
pub fn decode_with_limits(input: &[u8], limits: Limits) -> Result<Value<'_>, DecodeError> {
    if input.len() > limits.max_size {
        return Err(DecodeError {
            offset: 0,
            kind: DecodeErrorKind::SizeLimit,
        });
    }
    Decoder {
        input,
        pos: 0,
        tokens: 0,
        limits,
    }
    .value(0)
}

struct Decoder<'a> {
    input: &'a [u8],
    pos: usize,
    tokens: usize,
    limits: Limits,
}

impl<'a> Decoder<'a> {
    fn error_at(offset: usize, kind: DecodeErrorKind) -> DecodeError {
        DecodeError { offset, kind }
    }

    fn err<T>(&self, kind: DecodeErrorKind) -> Result<T, DecodeError> {
        Err(Self::error_at(self.pos, kind))
    }

    fn peek(&self) -> Result<u8, DecodeError> {
        match self.input.get(self.pos) {
            Some(&b) => Ok(b),
            None => self.err(DecodeErrorKind::UnexpectedEof),
        }
    }

    fn count_token(&mut self) -> Result<(), DecodeError> {
        self.tokens += 1;
        if self.tokens > self.limits.max_tokens {
            return self.err(DecodeErrorKind::TokenLimit);
        }
        Ok(())
    }

    /// Depth of the children of a container found at `depth`.
    fn enter(&self, depth: usize) -> Result<usize, DecodeError> {
        let depth = depth + 1;
        if depth > self.limits.max_depth {
            return self.err(DecodeErrorKind::DepthLimit);
        }
        Ok(depth)
    }

    /// `depth` is the number of containers enclosing this value.
    fn value(&mut self, depth: usize) -> Result<Value<'a>, DecodeError> {
        self.count_token()?;
        let start = self.pos;
        let kind = match self.peek()? {
            b'i' => {
                self.pos += 1;
                Kind::Int(self.int()?)
            }
            b'0'..=b'9' => Kind::Bytes(self.bytes()?),
            b'l' => {
                let inner = self.enter(depth)?;
                self.pos += 1;
                let mut items = Vec::new();
                while self.peek()? != b'e' {
                    items.push(self.value(inner)?);
                }
                self.pos += 1;
                Kind::List(items)
            }
            b'd' => {
                let inner = self.enter(depth)?;
                self.pos += 1;
                let mut entries = Vec::new();
                while self.peek()? != b'e' {
                    if !self.peek()?.is_ascii_digit() {
                        return self.err(DecodeErrorKind::NonStringKey);
                    }
                    self.count_token()?;
                    let key = self.bytes()?;
                    entries.push((key, self.value(inner)?));
                }
                self.pos += 1;
                Kind::Dict(entries)
            }
            _ => return self.err(DecodeErrorKind::InvalidByte),
        };
        Ok(Value {
            kind,
            raw: &self.input[start..self.pos],
        })
    }

    /// Reads `[-]digits e` after the `i`. Leading zeros and `-0` are accepted.
    fn int(&mut self) -> Result<i64, DecodeError> {
        let negative = self.peek()? == b'-';
        if negative {
            self.pos += 1;
        }
        if !self.peek()?.is_ascii_digit() {
            return self.err(DecodeErrorKind::InvalidByte);
        }
        let mut n: i64 = 0;
        loop {
            let b = self.peek()?;
            if b == b'e' {
                self.pos += 1;
                return Ok(n);
            }
            if !b.is_ascii_digit() {
                return self.err(DecodeErrorKind::InvalidByte);
            }
            let digit = i64::from(b - b'0');
            // Accumulate toward the sign so that i64::MIN is representable.
            n = n
                .checked_mul(10)
                .and_then(|n| {
                    if negative {
                        n.checked_sub(digit)
                    } else {
                        n.checked_add(digit)
                    }
                })
                .ok_or_else(|| Self::error_at(self.pos, DecodeErrorKind::IntOverflow))?;
            self.pos += 1;
        }
    }

    /// Reads `digits : bytes`. The caller has checked the first byte is a digit.
    fn bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let eof = Self::error_at(self.input.len(), DecodeErrorKind::UnexpectedEof);
        let mut len: usize = 0;
        loop {
            let b = self.peek()?;
            if b == b':' {
                self.pos += 1;
                break;
            }
            if !b.is_ascii_digit() {
                return self.err(DecodeErrorKind::InvalidByte);
            }
            // A length that overflows usize can never fit in the input.
            len = len
                .checked_mul(10)
                .and_then(|n| n.checked_add(usize::from(b - b'0')))
                .ok_or_else(|| eof.clone())?;
            self.pos += 1;
        }
        // The input is borrowed, so a bogus length costs no allocation.
        if len > self.input.len() - self.pos {
            return Err(eof);
        }
        let bytes = &self.input[self.pos..self.pos + len];
        self.pos += len;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(input: &[u8]) -> DecodeErrorKind {
        decode(input).unwrap_err().kind()
    }

    #[test]
    fn decodes_integers() {
        assert_eq!(decode(b"i42e").unwrap().as_int(), Some(42));
        assert_eq!(decode(b"i-7e").unwrap().as_int(), Some(-7));
        assert_eq!(decode(b"i0e").unwrap().as_int(), Some(0));
    }

    #[test]
    fn decodes_i64_extremes() {
        assert_eq!(
            decode(b"i9223372036854775807e").unwrap().as_int(),
            Some(i64::MAX)
        );
        assert_eq!(
            decode(b"i-9223372036854775808e").unwrap().as_int(),
            Some(i64::MIN)
        );
    }

    #[test]
    fn decodes_non_utf8_bytes() {
        let v = decode(b"2:\xff\xfe").unwrap();
        assert_eq!(v.as_bytes(), Some(&b"\xff\xfe"[..]));
    }

    #[test]
    fn decodes_empty_string_list_and_dict() {
        assert_eq!(decode(b"0:").unwrap().as_bytes(), Some(&b""[..]));
        assert_eq!(decode(b"le").unwrap().as_list().map(<[_]>::len), Some(0));
        assert_eq!(decode(b"de").unwrap().as_dict().map(<[_]>::len), Some(0));
    }

    #[test]
    fn decodes_nested_structure() {
        let v = decode(b"d4:listli1ei2ee4:name3:bobe").unwrap();
        let list: Vec<_> = v
            .get(b"list")
            .unwrap()
            .as_list()
            .unwrap()
            .iter()
            .map(|x| x.as_int().unwrap())
            .collect();
        assert_eq!(list, [1, 2]);
        assert_eq!(v.get(b"name").unwrap().as_bytes(), Some(&b"bob"[..]));
    }

    #[test]
    fn ignores_trailing_bytes() {
        let v = decode(b"i1exyz").unwrap();
        assert_eq!(v.as_int(), Some(1));
        assert_eq!(v.raw(), b"i1e");
    }

    #[test]
    fn accepts_unsorted_keys() {
        let v = decode(b"d1:bi2e1:ai1ee").unwrap();
        assert_eq!(v.get(b"a").unwrap().as_int(), Some(1));
        assert_eq!(v.get(b"b").unwrap().as_int(), Some(2));
    }

    #[test]
    fn accepts_leading_zeros_and_negative_zero() {
        assert_eq!(decode(b"i042e").unwrap().as_int(), Some(42));
        assert_eq!(decode(b"i-0e").unwrap().as_int(), Some(0));
        assert_eq!(decode(b"03:abc").unwrap().as_bytes(), Some(&b"abc"[..]));
    }

    #[test]
    fn duplicate_key_returns_first_value() {
        let v = decode(b"d1:ai1e1:ai2ee").unwrap();
        assert_eq!(v.get(b"a").unwrap().as_int(), Some(1));
        assert_eq!(v.as_dict().unwrap().len(), 2);
    }

    #[test]
    fn get_on_non_dict_is_none() {
        assert!(decode(b"i1e").unwrap().get(b"a").is_none());
    }

    #[test]
    fn raw_span_of_non_canonical_dict_is_original_bytes() {
        let input = b"d4:infod1:bi2e1:ai1eee";
        let v = decode(input).unwrap();
        let info = v.get(b"info").unwrap();
        assert_eq!(info.raw(), b"d1:bi2e1:ai1ee");
        // It is a sub-slice of the input, not a re-encoding.
        assert_eq!(info.raw().as_ptr(), input[7..].as_ptr());
    }

    #[test]
    fn truncated_input_reports_eof_and_offset() {
        let e = decode(b"l1:a").unwrap_err();
        assert_eq!((e.kind(), e.offset()), (DecodeErrorKind::UnexpectedEof, 4));
        assert_eq!(kind_of(b""), DecodeErrorKind::UnexpectedEof);
        assert_eq!(kind_of(b"i12"), DecodeErrorKind::UnexpectedEof);
        assert_eq!(kind_of(b"3:ab"), DecodeErrorKind::UnexpectedEof);
        assert_eq!(kind_of(b"d1:a"), DecodeErrorKind::UnexpectedEof);
    }

    #[test]
    fn rejects_invalid_bytes() {
        let e = decode(b"x").unwrap_err();
        assert_eq!((e.kind(), e.offset()), (DecodeErrorKind::InvalidByte, 0));
        assert_eq!(kind_of(b"i1x2e"), DecodeErrorKind::InvalidByte);
        assert_eq!(kind_of(b"ie"), DecodeErrorKind::InvalidByte);
        assert_eq!(kind_of(b"i-e"), DecodeErrorKind::InvalidByte);
        assert_eq!(kind_of(b"1x:a"), DecodeErrorKind::InvalidByte);
    }

    #[test]
    fn rejects_integer_overflow() {
        assert_eq!(
            kind_of(b"i99999999999999999999e"),
            DecodeErrorKind::IntOverflow
        );
        assert_eq!(
            kind_of(b"i9223372036854775808e"),
            DecodeErrorKind::IntOverflow
        );
    }

    #[test]
    fn rejects_non_string_key() {
        let e = decode(b"di1ei2ee").unwrap_err();
        assert_eq!((e.kind(), e.offset()), (DecodeErrorKind::NonStringKey, 1));
    }

    fn nested_lists(depth: usize) -> Vec<u8> {
        let mut v = vec![b'l'; depth];
        v.extend(std::iter::repeat_n(b'e', depth));
        v
    }

    #[test]
    fn depth_limit() {
        assert!(decode(&nested_lists(100)).is_ok());
        assert_eq!(kind_of(&nested_lists(101)), DecodeErrorKind::DepthLimit);
        // Far beyond the limit: rejected before recursing, so no stack overflow.
        assert_eq!(
            kind_of(&nested_lists(1_000_000)),
            DecodeErrorKind::DepthLimit
        );
    }

    #[test]
    fn token_limit() {
        let limits = Limits {
            max_tokens: 10,
            ..Limits::default()
        };
        let mut input = b"l".to_vec();
        for _ in 0..20 {
            input.extend_from_slice(b"i1e");
        }
        input.push(b'e');
        let e = decode_with_limits(&input, limits).unwrap_err();
        assert_eq!(e.kind(), DecodeErrorKind::TokenLimit);

        // Dictionary keys count as tokens too: 1 dict + 5 keys + 5 values.
        let dict = b"d1:ai1e1:bi1e1:ci1e1:di1e1:ei1ee";
        let with = |max_tokens| Limits {
            max_tokens,
            ..Limits::default()
        };
        assert!(decode_with_limits(dict, with(11)).is_ok());
        assert!(decode_with_limits(dict, with(10)).is_err());
    }

    #[test]
    fn size_limit_is_checked_before_parsing() {
        let limits = Limits {
            max_size: 1024,
            ..Limits::default()
        };
        let input = vec![b'x'; 2048]; // not valid bencode either
        let e = decode_with_limits(&input, limits).unwrap_err();
        assert_eq!((e.kind(), e.offset()), (DecodeErrorKind::SizeLimit, 0));
    }

    #[test]
    fn huge_claimed_length_is_eof() {
        assert_eq!(kind_of(b"999999999:abc"), DecodeErrorKind::UnexpectedEof);
        assert_eq!(
            kind_of(b"99999999999999999999999999:abc"),
            DecodeErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn default_limits_match_qbittorrent() {
        let l = Limits::default();
        assert_eq!(
            (l.max_size, l.max_depth, l.max_tokens),
            (100 * 1024 * 1024, 100, 10_000_000)
        );
    }
}
