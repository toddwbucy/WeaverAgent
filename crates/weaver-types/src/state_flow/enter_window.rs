//! The original root load interval. A codec validates its shape, never its age.
use super::{DecimalU64, ValidationError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EnterWindow {
    pub began_ns: DecimalU64,
    pub deadline_ns: DecimalU64,
}
impl EnterWindow {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.began_ns.0 >= self.deadline_ns.0 {
            return Err(ValidationError(
                "enter window requires began_ns < deadline_ns",
            ));
        }
        Ok(())
    }
}
impl<'de> Deserialize<'de> for EnterWindow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        super::codec::closed_object! {
            #[derive(Serialize)]
            struct Fields {
                began_ns: DecimalU64,
                deadline_ns: DecimalU64,
            }
        }
        let Fields {
            began_ns,
            deadline_ns,
        } = Fields::deserialize(deserializer)?;
        let value = Self {
            began_ns,
            deadline_ns,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_enter_window_is_canonical_closed_and_positive_without_clock_policy() {
        for json in [
            r#"{"began_ns":"0","deadline_ns":"1"}"#,
            r#"{"began_ns":"1","deadline_ns":"18446744073709551615"}"#,
        ] {
            let value: EnterWindow = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
        }
        for json in [
            r#"{"began_ns":"1","deadline_ns":"1"}"#,
            r#"{"began_ns":"2","deadline_ns":"1"}"#,
            r#"{"began_ns":"0","deadline_ns":"18446744073709551616"}"#,
            r#"{"began_ns":0,"deadline_ns":"1"}"#,
            r#"{"began_ns":"00","deadline_ns":"1"}"#,
            r#"{"began_ns":"-1","deadline_ns":"1"}"#,
            r#"{"began_ns":"0","deadline_ns":"+1"}"#,
            r#"{"began_ns":"0"}"#,
            r#"{"deadline_ns":"1"}"#,
            r#"{"began_ns":"0","deadline_ns":"1","extra":false}"#,
            r#"{"began_ns":"0","began_ns":"0","deadline_ns":"1"}"#,
            r#"["0","1"]"#,
            "null",
        ] {
            assert!(serde_json::from_str::<EnterWindow>(json).is_err(), "{json}");
        }
    }
}
