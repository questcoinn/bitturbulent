//! Bencode decoding and encoding.
//!
//! Decoding borrows the input: every [`Value`] keeps the exact bytes it was
//! read from ([`Value::raw`]), so an info-hash can be computed over the
//! original `info` dictionary even when the file is not in canonical form.
//! Encoding works on [`OwnedValue`], whose dictionaries are always sorted,
//! so its output is always canonical.

mod decode;
mod encode;

use std::collections::BTreeMap;

pub use decode::{DecodeError, DecodeErrorKind, Limits, decode, decode_with_limits};
pub use encode::{OwnedValue, encode};

/// A decoded bencode value together with the input bytes it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value<'a> {
    kind: Kind<'a>,
    raw: &'a [u8],
}

/// The shape of a decoded bencode value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind<'a> {
    /// `i<n>e`
    Int(i64),
    /// `<len>:<bytes>`; the bytes are not required to be UTF-8.
    Bytes(&'a [u8]),
    /// `l...e`
    List(Vec<Value<'a>>),
    /// `d...e`, in input order. Keys may be unsorted or repeated.
    Dict(Vec<(&'a [u8], Value<'a>)>),
}

impl<'a> Value<'a> {
    /// The decoded shape of this value.
    pub fn kind(&self) -> &Kind<'a> {
        &self.kind
    }

    /// The exact input bytes this value was decoded from.
    ///
    /// For the top-level value this is also how many bytes the decoder
    /// consumed: anything after `raw().len()` was ignored.
    pub fn raw(&self) -> &'a [u8] {
        self.raw
    }

    /// The integer, if this is an integer.
    pub fn as_int(&self) -> Option<i64> {
        match self.kind {
            Kind::Int(n) => Some(n),
            _ => None,
        }
    }

    /// The byte string, if this is a byte string.
    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match self.kind {
            Kind::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// The elements, if this is a list.
    pub fn as_list(&self) -> Option<&[Value<'a>]> {
        match &self.kind {
            Kind::List(items) => Some(items),
            _ => None,
        }
    }

    /// The entries in input order, if this is a dictionary.
    pub fn as_dict(&self) -> Option<&[(&'a [u8], Value<'a>)]> {
        match &self.kind {
            Kind::Dict(entries) => Some(entries),
            _ => None,
        }
    }

    /// Looks up `key` in a dictionary. When the key appears more than once
    /// the first occurrence wins, like libtorrent's `dict_find`.
    /// Returns `None` if this is not a dictionary or the key is absent.
    pub fn get(&self, key: &[u8]) -> Option<&Value<'a>> {
        self.as_dict()?
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    /// Copies this value into an [`OwnedValue`] that can be encoded.
    /// Repeated dictionary keys keep their first value.
    pub fn to_owned_value(&self) -> OwnedValue {
        match &self.kind {
            Kind::Int(n) => OwnedValue::Int(*n),
            Kind::Bytes(b) => OwnedValue::Bytes(b.to_vec()),
            Kind::List(items) => {
                OwnedValue::List(items.iter().map(Value::to_owned_value).collect())
            }
            Kind::Dict(entries) => {
                let mut map = BTreeMap::new();
                for (k, v) in entries {
                    map.entry(k.to_vec()).or_insert_with(|| v.to_owned_value());
                }
                OwnedValue::Dict(map)
            }
        }
    }
}
