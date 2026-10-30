//! Canonical bencode encoder.

use std::collections::BTreeMap;

/// An owned bencode value for encoding.
///
/// Dictionaries are a [`BTreeMap`], so keys are always in byte order and
/// [`encode`] always produces canonical bencode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedValue {
    /// `i<n>e`
    Int(i64),
    /// `<len>:<bytes>`
    Bytes(Vec<u8>),
    /// `l...e`
    List(Vec<OwnedValue>),
    /// `d...e`
    Dict(BTreeMap<Vec<u8>, OwnedValue>),
}

/// Encodes `value` as canonical bencode: sorted keys, no leading zeros.
pub fn encode(value: &OwnedValue) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(value, &mut out);
    out
}

fn encode_into(value: &OwnedValue, out: &mut Vec<u8>) {
    match value {
        OwnedValue::Int(n) => {
            out.push(b'i');
            out.extend_from_slice(n.to_string().as_bytes());
            out.push(b'e');
        }
        OwnedValue::Bytes(b) => encode_bytes(b, out),
        OwnedValue::List(items) => {
            out.push(b'l');
            for item in items {
                encode_into(item, out);
            }
            out.push(b'e');
        }
        OwnedValue::Dict(map) => {
            out.push(b'd');
            for (k, v) in map {
                encode_bytes(k, out);
                encode_into(v, out);
            }
            out.push(b'e');
        }
    }
}

fn encode_bytes(b: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b.len().to_string().as_bytes());
    out.push(b':');
    out.extend_from_slice(b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::decode;

    #[test]
    fn sorts_dictionary_keys() {
        let mut map = BTreeMap::new();
        map.insert(b"b".to_vec(), OwnedValue::Int(2));
        map.insert(b"a".to_vec(), OwnedValue::Int(1));
        assert_eq!(encode(&OwnedValue::Dict(map)), b"d1:ai1e1:bi2ee");
    }

    #[test]
    fn encodes_negative_and_zero() {
        assert_eq!(encode(&OwnedValue::Int(-5)), b"i-5e");
        assert_eq!(encode(&OwnedValue::Int(0)), b"i0e");
    }

    #[test]
    fn encodes_bytes_and_lists() {
        let v = OwnedValue::List(vec![
            OwnedValue::Bytes(b"spam".to_vec()),
            OwnedValue::Bytes(Vec::new()),
            OwnedValue::List(Vec::new()),
        ]);
        assert_eq!(encode(&v), b"l4:spam0:lee");
    }

    #[test]
    fn non_canonical_input_is_canonicalized() {
        let v = decode(b"d1:bi02e1:ai1e1:ai9ee").unwrap();
        assert_eq!(encode(&v.to_owned_value()), b"d1:ai1e1:bi2ee");
    }

    #[test]
    fn canonical_inputs_round_trip() {
        let inputs: &[&[u8]] = &[
            b"i0e",
            b"i-9223372036854775808e",
            b"0:",
            b"4:spam",
            b"le",
            b"de",
            b"l4:spami42ee",
            b"d3:agei30e4:name3:bobe",
            b"d4:infod6:lengthi1000e4:name5:a.txt12:piece lengthi16384e6:pieces20:aaaaaaaaaaaaaaaaaaaaee",
            b"d1:ald1:bld1:ci-1eeeeee",
            b"2:\xff\xfe",
        ];
        for input in inputs {
            let v = decode(input).unwrap();
            assert_eq!(&encode(&v.to_owned_value()), input, "{input:?}");
        }
    }
}
