//! Byte codecs that device files name in `codec = "..."`. Families add their own.

use crate::action::Action;

type Result<T> = std::result::Result<T, String>;
type PairDecode = fn(&[u8]) -> Result<(u16, u16)>;
type PairEncode = fn((u16, u16), &mut [u8]) -> Result<()>;
/// Action, button index, main location bytes, extra location bytes.
type ActionEncode = fn(&Action, usize, &mut [u8], &mut [u8]) -> Result<()>;

#[derive(Clone, Copy)]
pub enum Codec {
    Scalar {
        len: usize,
        decode: fn(&[u8]) -> Result<i64>,
        encode: fn(i64, &mut [u8]) -> Result<()>,
    },
    /// One location holding both X and Y DPI.
    Pair {
        len: usize,
        decode: PairDecode,
        encode: PairEncode,
    },
    /// A button binding; `extra` is the optional second region (empty when absent).
    Action {
        len: usize,
        extra: usize,
        decode: fn(&[u8], &[u8]) -> Result<Action>,
        encode: ActionEncode,
    },
}

impl Codec {
    pub fn len(&self) -> usize {
        match self {
            Self::Scalar { len, .. } | Self::Pair { len, .. } | Self::Action { len, .. } => *len,
        }
    }
}

fn range<T: TryFrom<i64>>(value: i64) -> Result<T> {
    T::try_from(value).map_err(|_| format!("{value} does not fit its location."))
}

pub fn generic(name: &str) -> Option<Codec> {
    Some(match name {
        "u8" => Codec::Scalar {
            len: 1,
            decode: |b| Ok(i64::from(b[0])),
            encode: |v, b| {
                b[0] = range(v)?;
                Ok(())
            },
        },
        "i8" => Codec::Scalar {
            len: 1,
            decode: |b| Ok(i64::from(b[0] as i8)),
            encode: |v, b| {
                b[0] = range::<i8>(v)? as u8;
                Ok(())
            },
        },
        "u16le" => Codec::Scalar {
            len: 2,
            decode: |b| Ok(i64::from(u16::from_le_bytes([b[0], b[1]]))),
            encode: |v, b| {
                b.copy_from_slice(&range::<u16>(v)?.to_le_bytes());
                Ok(())
            },
        },
        "u16be" => Codec::Scalar {
            len: 2,
            decode: |b| Ok(i64::from(u16::from_be_bytes([b[0], b[1]]))),
            encode: |v, b| {
                b.copy_from_slice(&range::<u16>(v)?.to_be_bytes());
                Ok(())
            },
        },
        _ => return None,
    })
}
