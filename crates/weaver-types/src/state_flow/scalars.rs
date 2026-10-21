use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A structural contract violation, not a language-conformance verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationError(pub &'static str);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ValidationError {}

/// An unsigned counter whose only wire representation is a canonical decimal string.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecimalU64(pub u64);

impl From<u64> for DecimalU64 {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for DecimalU64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for DecimalU64 {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || !value.bytes().all(|byte| byte.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return Err(ValidationError("counter is not canonical unsigned decimal"));
        }
        value
            .parse()
            .map(Self)
            .map_err(|_| ValidationError("counter exceeds u64"))
    }
}

impl Serialize for DecimalU64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for DecimalU64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CounterVisitor;

        impl de::Visitor<'_> for CounterVisitor {
            type Value = DecimalU64;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a canonical unsigned decimal string within u64")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(CounterVisitor)
    }
}

/// A SHA-256 identity. Construction never normalizes an invalid spelling.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Digest(String);

impl Digest {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ValidationError(
                "digest must be exactly 64 lowercase hexadecimal bytes",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Digest {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// A measured model-blocking bound, in elapsed nanoseconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MeasuredWait {
    pub elapsed_ns: DecimalU64,
    pub measurement_ref: String,
}

/// A measured observation cadence. It grants no timeout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MeasuredInterval {
    pub elapsed_ns: DecimalU64,
    pub measurement_ref: String,
}

macro_rules! measured {
    ($name:ident) => {
        impl $name {
            pub fn validate(&self) -> Result<(), ValidationError> {
                if self.elapsed_ns.0 == 0 {
                    return Err(ValidationError("elapsed_ns must be positive"));
                }
                if self.measurement_ref.is_empty() {
                    return Err(ValidationError("measurement_ref must be nonempty"));
                }
                Ok(())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields {
                    elapsed_ns: DecimalU64,
                    measurement_ref: String,
                }
                let fields = Fields::deserialize(deserializer)?;
                let result = Self {
                    elapsed_ns: fields.elapsed_ns,
                    measurement_ref: fields.measurement_ref,
                };
                result.validate().map_err(de::Error::custom)?;
                Ok(result)
            }
        }
    };
}

measured!(MeasuredWait);
measured!(MeasuredInterval);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_keep_every_bit_and_canonical_spelling() {
        for n in [0, 1, (1_u64 << 53) + 1, i64::MAX as u64 + 1, u64::MAX] {
            let bytes = serde_json::to_string(&DecimalU64(n)).unwrap();
            assert_eq!(bytes, format!("\"{n}\""));
            assert_eq!(serde_json::from_str::<DecimalU64>(&bytes).unwrap().0, n);
        }
        for value in [
            "",
            "00",
            "01",
            "+1",
            "-1",
            " 1",
            "1 ",
            "1.0",
            "1e0",
            "\u{661}",
            "18446744073709551616",
        ] {
            assert!(value.parse::<DecimalU64>().is_err(), "{value:?}");
            assert!(
                serde_json::from_str::<DecimalU64>(&serde_json::to_string(value).unwrap()).is_err()
            );
        }
        for json in ["0", "1", "null", "true", "[]", "{}"] {
            assert!(serde_json::from_str::<DecimalU64>(json).is_err(), "{json}");
        }
    }

    #[test]
    fn digest_constructor_and_decoder_reject_without_normalizing() {
        let valid = "0123456789abcdef".repeat(4);
        assert_eq!(Digest::new(&valid).unwrap().as_str(), valid);
        assert_eq!(
            serde_json::to_string(&Digest::new(&valid).unwrap()).unwrap(),
            format!("\"{valid}\"")
        );
        for invalid in [
            String::new(),
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
            format!(" {}", "a".repeat(63)),
        ] {
            assert!(Digest::new(&invalid).is_err());
            assert!(
                serde_json::from_str::<Digest>(&serde_json::to_string(&invalid).unwrap()).is_err()
            );
        }
        assert!(serde_json::from_str::<Digest>("null").is_err());
    }

    #[test]
    fn waits_and_intervals_refuse_invalid_or_ambiguous_evidence() {
        let good = r#"{"elapsed_ns":"1","measurement_ref":"tuple/run"}"#;
        assert_eq!(
            serde_json::from_str::<MeasuredWait>(good)
                .unwrap()
                .elapsed_ns
                .0,
            1
        );
        assert_eq!(
            serde_json::from_str::<MeasuredInterval>(good)
                .unwrap()
                .elapsed_ns
                .0,
            1
        );
        for bad in [
            r#"{"elapsed_ns":"0","measurement_ref":"tuple/run"}"#,
            r#"{"elapsed_ns":"1","measurement_ref":""}"#,
            r#"{"elapsed_ns":"1"}"#,
            r#"{"measurement_ref":"tuple/run"}"#,
            r#"{"elapsed_ns":"1","measurement_ref":"tuple/run","extra":0}"#,
            r#"{"elapsed_ns":"1","elapsed_ns":"2","measurement_ref":"tuple/run"}"#,
        ] {
            assert!(serde_json::from_str::<MeasuredWait>(bad).is_err(), "{bad}");
            assert!(
                serde_json::from_str::<MeasuredInterval>(bad).is_err(),
                "{bad}"
            );
        }
    }
}
