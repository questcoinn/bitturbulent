//! Index newtypes for pieces and files inside a torrent.

use std::fmt;

// `as_usize` relies on every `u32` fitting in `usize`.
const _: () = assert!(usize::BITS >= 32);

/// Error returned when a `usize` does not fit in a 32-bit index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("index {0} does not fit in 32 bits")]
pub struct IndexOutOfRange(pub usize);

macro_rules! index_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        pub struct $name(u32);

        impl $name {
            /// Wraps a raw index.
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            /// Returns the raw index.
            pub const fn get(self) -> u32 {
                self.0
            }

            /// Returns the index as a `usize`, for indexing into slices.
            pub const fn as_usize(self) -> usize {
                self.0 as usize
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u32 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl TryFrom<usize> for $name {
            type Error = IndexOutOfRange;

            fn try_from(value: usize) -> Result<Self, Self::Error> {
                u32::try_from(value).map(Self).map_err(|_| IndexOutOfRange(value))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

index_type!(
    /// Zero-based index of a piece. Same range as the wire protocol's
    /// 4-byte piece index.
    ///
    /// Not interchangeable with [`FileIndex`]:
    ///
    /// ```compile_fail,E0308
    /// use bt_core::{FileIndex, PieceIndex};
    /// fn takes_piece(_: PieceIndex) {}
    /// takes_piece(FileIndex::new(0));
    /// ```
    PieceIndex
);

index_type!(
    /// Zero-based index of a file in a torrent's file list.
    ///
    /// Not interchangeable with [`PieceIndex`] or a bare integer:
    ///
    /// ```compile_fail,E0308
    /// use bt_core::FileIndex;
    /// fn takes_file(_: FileIndex) {}
    /// takes_file(3u32);
    /// ```
    FileIndex
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_u32_max() {
        assert_eq!(PieceIndex::from(u32::MAX).get(), 4_294_967_295);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn rejects_usize_above_u32() {
        let too_big = u32::MAX as usize + 1;
        assert_eq!(PieceIndex::try_from(too_big), Err(IndexOutOfRange(too_big)));
    }

    #[test]
    fn accepts_usize_in_range() {
        assert_eq!(FileIndex::try_from(7usize), Ok(FileIndex::new(7)));
    }

    #[test]
    fn displays_decimal() {
        assert_eq!(FileIndex::new(42).to_string(), "42");
    }

    #[test]
    fn sorts_by_value() {
        let mut v = [PieceIndex::new(3), PieceIndex::new(1), PieceIndex::new(2)];
        v.sort();
        assert_eq!(
            v,
            [PieceIndex::new(1), PieceIndex::new(2), PieceIndex::new(3)]
        );
    }

    #[test]
    fn converts_to_usize() {
        assert_eq!(PieceIndex::new(5).as_usize(), 5);
    }
}
