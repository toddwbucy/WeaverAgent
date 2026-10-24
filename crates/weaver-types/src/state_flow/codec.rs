//! Complete-line codecs with duplicate-key refusal before typed interpretation.

use std::collections::BTreeSet;
use std::fmt;
use std::io::{self, Write};

use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::{DecodedStateFrame, SeedFrame, StateFrame};

pub const STATE_REQUEST_LINE_BOUND: usize = 8 * 1024 * 1024;
pub const STATE_ANSWER_LINE_BOUND: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDirection {
    Request,
    Response,
}

impl FrameDirection {
    pub fn line_bound(self) -> usize {
        match self {
            Self::Request => STATE_REQUEST_LINE_BOUND,
            Self::Response => STATE_ANSWER_LINE_BOUND,
        }
    }
}

#[derive(Debug)]
pub enum FrameCodecError {
    FrameTooLarge,
    MissingNewline,
    EmbeddedNewline,
    WrongDirection,
    Malformed(serde_json::Error),
}

impl fmt::Display for FrameCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooLarge => f.write_str("frame_too_large"),
            Self::MissingNewline => f.write_str("frame lacks its final LF"),
            Self::EmbeddedNewline => f.write_str("more than one physical line in frame"),
            Self::WrongDirection => f.write_str("frame has the wrong direction"),
            Self::Malformed(error) => write!(f, "malformed frame: {error}"),
        }
    }
}

impl std::error::Error for FrameCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(error) => Some(error),
            _ => None,
        }
    }
}

/// Read the original JSON without constructing a Value or changing any raw spelling.
pub fn reject_duplicate_keys(json: &str) -> Result<(), serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    UniqueJson::deserialize(&mut deserializer)?;
    deserializer.end()
}

/// Used for complete results as well as frames. RawValue fields retain their bytes.
pub fn decode_strict<T: DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
    reject_duplicate_keys(json)?;
    serde_json::from_str(json)
}

/// Delegate a typed field map without accepting serde's struct-as-array form.
pub(crate) fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Object<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(std::marker::PhantomData))
}

// One field list defines the public carrier and its private closed map decoder.
// The private helper cannot expose a second public deserialization entry point.
macro_rules! closed_object {
    (@attributes [$($public:tt)*] [$($decoder:tt)*]
        #[serde(rename_all = $rename:literal)] $($rest:tt)*) => {
        $crate::state_flow::codec::closed_object! {
            @attributes [$($public)* #[serde(rename_all = $rename)]]
            [$($decoder)* #[serde(rename_all = $rename)]] $($rest)*
        }
    };
    (@attributes [$($public:tt)*] [$($decoder:tt)*]
        #[serde($($unsupported:tt)*)] $($rest:tt)*) => {
        compile_error!("closed_object supports only rename_all as a container serde attribute");
    };
    (@attributes [$($public:tt)*] [$($decoder:tt)*]
        #[$attribute:meta] $($rest:tt)*) => {
        $crate::state_flow::codec::closed_object! {
            @attributes [$($public)* #[$attribute]] [$($decoder)*] $($rest)*
        }
    };
    (@attributes [$($public:tt)*] [$($decoder:tt)*]
        @struct $visibility:vis struct $name:ident {
            $($(#[$field_attribute:meta])* $field_visibility:vis $field:ident: $kind:ty),* $(,)?
        }) => {
        $($public)*
        $visibility struct $name {
            $($(#[$field_attribute])* $field_visibility $field: $kind,)*
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                $($decoder)*
                struct Fields {
                    $($(#[$field_attribute])* $field: $kind,)*
                }
                let Fields { $($field,)* } =
                    $crate::state_flow::codec::deserialize_object(deserializer)?;
                Ok(Self { $($field,)* })
            }
        }
    };
    ($(#[$($attribute:tt)*])* $visibility:vis struct $name:ident {
        $($(#[$field_attribute:meta])* $field_visibility:vis $field:ident: $kind:ty),* $(,)?
    }) => {
        $crate::state_flow::codec::closed_object! {
            @attributes [] [] $(#[$($attribute)*])* @struct $visibility struct $name {
                $($(#[$field_attribute])* $field_visibility $field: $kind,)*
            }
        }
    };
}
pub(crate) use closed_object;

struct UniqueJson;

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                while sequence.next_element::<UniqueJson>()?.is_some() {}
                Ok(UniqueJson)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut keys = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(serde::de::Error::custom("duplicate JSON object key"));
                    }
                    map.next_value::<UniqueJson>()?;
                }
                Ok(UniqueJson)
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

fn line_body(line: &[u8], direction: FrameDirection) -> Result<&str, FrameCodecError> {
    if line.len() > direction.line_bound() {
        return Err(FrameCodecError::FrameTooLarge);
    }
    let body = line
        .strip_suffix(b"\n")
        .ok_or(FrameCodecError::MissingNewline)?;
    if body.contains(&b'\n') {
        return Err(FrameCodecError::EmbeddedNewline);
    }
    std::str::from_utf8(body).map_err(|error| {
        FrameCodecError::Malformed(<serde_json::Error as serde::de::Error>::custom(error))
    })
}

/// The same strict codec, retaining its actual outer raw-buffer copy evidence.
/// Deeper Text/context decoding belongs to the caller's separate coverage audit.
pub fn decode_state_line_observed(
    line: &[u8],
    direction: FrameDirection,
) -> Result<DecodedStateFrame, FrameCodecError> {
    decode_state_line_as(line, direction, |decoded: &DecodedStateFrame| {
        &decoded.frame
    })
}

pub fn decode_state_line(
    line: &[u8],
    direction: FrameDirection,
) -> Result<StateFrame, FrameCodecError> {
    decode_state_line_as(line, direction, |frame| frame)
}

fn decode_state_line_as<T: DeserializeOwned>(
    line: &[u8],
    direction: FrameDirection,
    frame: fn(&T) -> &StateFrame,
) -> Result<T, FrameCodecError> {
    let decoded = decode_strict(line_body(line, direction)?).map_err(FrameCodecError::Malformed)?;
    if frame(&decoded).is_request() != (direction == FrameDirection::Request) {
        return Err(FrameCodecError::WrongDirection);
    }
    Ok(decoded)
}

pub fn decode_seed_line(
    line: &[u8],
    direction: FrameDirection,
) -> Result<SeedFrame, FrameCodecError> {
    let frame: SeedFrame =
        decode_strict(line_body(line, direction)?).map_err(FrameCodecError::Malformed)?;
    if frame.is_request() != (direction == FrameDirection::Request) {
        return Err(FrameCodecError::WrongDirection);
    }
    Ok(frame)
}

pub fn encode_state_line(
    frame: &StateFrame,
    direction: FrameDirection,
) -> Result<Vec<u8>, FrameCodecError> {
    if frame.is_request() != (direction == FrameDirection::Request) {
        return Err(FrameCodecError::WrongDirection);
    }
    encode_line(frame, direction.line_bound())
}

pub fn encode_seed_line(
    frame: &SeedFrame,
    direction: FrameDirection,
) -> Result<Vec<u8>, FrameCodecError> {
    if frame.is_request() != (direction == FrameDirection::Request) {
        return Err(FrameCodecError::WrongDirection);
    }
    encode_line(frame, direction.line_bound())
}

fn encode_line<T: Serialize>(value: &T, line_bound: usize) -> Result<Vec<u8>, FrameCodecError> {
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        bound: line_bound - 1,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, value) {
        return Err(if writer.exceeded {
            FrameCodecError::FrameTooLarge
        } else {
            FrameCodecError::Malformed(error)
        });
    }
    if writer.bytes.contains(&b'\n') {
        return Err(FrameCodecError::EmbeddedNewline);
    }
    let json = std::str::from_utf8(&writer.bytes).map_err(|error| {
        FrameCodecError::Malformed(<serde_json::Error as serde::de::Error>::custom(error))
    })?;
    reject_duplicate_keys(json).map_err(FrameCodecError::Malformed)?;
    writer.bytes.push(b'\n');
    Ok(writer.bytes)
}

struct LimitedWriter {
    bytes: Vec<u8>,
    bound: usize,
    exceeded: bool,
}

impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.bound.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("complete frame exceeds its bound"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_detects_decoded_key_aliases_at_every_depth() {
        for bad in [
            r#"{"x":1,"x":2}"#,
            r#"{"outer":[{"x":1,"\u0078":2}]}"#,
            r#"{"raw":{"nested":{"x":1,"x":2}}}"#,
        ] {
            assert!(reject_duplicate_keys(bad).is_err(), "{bad}");
        }
        assert!(reject_duplicate_keys(r#"{"x":{"x":1},"y":{"x":2}}"#).is_ok());
        assert!(reject_duplicate_keys("{} {}").is_err());
    }

    #[test]
    fn bounded_writer_counts_lf_and_never_returns_a_prefix() {
        assert_eq!(encode_line(&"a", 4).unwrap(), b"\"a\"\n");
        assert!(matches!(
            encode_line(&"aa", 4),
            Err(FrameCodecError::FrameTooLarge)
        ));
        assert!(matches!(
            line_body(b"{}", FrameDirection::Request),
            Err(FrameCodecError::MissingNewline)
        ));
        assert!(matches!(
            line_body(b"{}\n{}\n", FrameDirection::Request),
            Err(FrameCodecError::EmbeddedNewline)
        ));
    }
}
