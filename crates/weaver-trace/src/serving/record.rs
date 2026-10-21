use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{MonotonicNs, Sequence};

use super::{Channel, Kind, Origin, SourceEvent, TurnRule, turn_rule};

/// Strict object structure with borrowed value spellings. It does not decode
/// event-specific metadata, which belongs to the authoring and consumer codecs.
pub(super) struct Object<'a>(pub Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for Object<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Object<'de>;
            fn expecting(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                out.write_str("an object with unique field names")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut fields = Vec::new();
                let mut seen = BTreeSet::new();
                while let Some((key, value)) = map.next_entry::<String, &'de RawValue>()? {
                    if !seen.insert(key.clone()) {
                        return Err(serde::de::Error::custom("duplicate object member"));
                    }
                    fields.push((key, value));
                }
                Ok(Object(fields))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

impl<'a> Object<'a> {
    pub fn parse(raw: &'a str) -> Result<Self, ()> {
        serde_json::from_str(raw).map_err(|_| ())
    }

    pub fn get(&self, key: &str) -> Option<&'a RawValue> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
    }

    pub fn exact(&self, keys: &[&str]) -> bool {
        self.0.len() == keys.len() && keys.iter().all(|key| self.get(key).is_some())
    }

    pub fn string(&self, key: &str) -> Result<String, ()> {
        serde_json::from_str(self.get(key).ok_or(())?.get()).map_err(|_| ())
    }

    fn optional_string(&self, key: &str) -> Result<Option<String>, ()> {
        self.get(key)
            .map(|raw| serde_json::from_str(raw.get()).map_err(|_| ()))
            .transpose()
    }

    fn counter(&self, key: &str) -> Result<u64, ()> {
        decimal(&self.string(key)?).ok_or(())
    }
}

pub(super) fn decimal(value: &str) -> Option<u64> {
    if value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
}

pub(super) struct Record<'a> {
    pub kind: Kind,
    pub channel: Channel,
    pub origin: Origin,
    pub envelope: String,
    pub payload: Option<&'a RawValue>,
}

#[derive(Serialize)]
struct Envelope<'a> {
    session: &'a str,
    run: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn: Option<&'a str>,
    sequence: Sequence,
    kind: Kind,
    channel: Channel,
    origin: Origin,
    subsystem: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    causal_parent: Option<Sequence>,
    wall_ms: u64,
    monotonic_ns: MonotonicNs,
}

impl<'a> Record<'a> {
    pub fn parse(source: &SourceEvent, line: &'a str) -> Result<Self, ()> {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if body.contains(['\r', '\n']) {
            return Err(());
        }
        let object = Object::parse(body)?;
        let session = object.string("session")?;
        let run = object.string("run")?;
        let turn = object.optional_string("turn")?;
        let sequence = Sequence(object.counter("sequence")?);
        if run != source.run || sequence != source.sequence {
            return Err(());
        }
        let kind = Kind::from_name(&object.string("kind")?).ok_or(())?;
        if matches!(
            (turn_rule(kind), turn.is_some()),
            (TurnRule::Required, false) | (TurnRule::Forbidden, true)
        ) {
            return Err(());
        }
        let channel = Channel::from_name(&object.string("channel")?).ok_or(())?;
        let origin = Origin::from_name(&object.string("origin")?).ok_or(())?;
        let subsystem = object.string("subsystem")?;
        if !matches!(
            subsystem.as_str(),
            "admin" | "harness" | "spu" | "spu_decoder" | "gate" | "tool"
        ) {
            return Err(());
        }
        let causal_parent = object
            .optional_string("causal_parent")?
            .map(|value| decimal(&value).map(Sequence).ok_or(()))
            .transpose()?;
        let wall_ms =
            serde_json::from_str(object.get("wall_ms").ok_or(())?.get()).map_err(|_| ())?;
        let monotonic_ns = MonotonicNs(object.counter("monotonic_ns")?);
        let envelope = serde_json::to_string(&Envelope {
            session: &session,
            run: &run,
            turn: turn.as_deref(),
            sequence,
            kind,
            channel,
            origin,
            subsystem: &subsystem,
            causal_parent,
            wall_ms,
            monotonic_ns,
        })
        .map_err(|_| ())?;
        let payload = object.get("payload");
        // Compare the canonical header without normalizing or reserializing any
        // payload value. This also refuses unknown keys, reordered headers, null
        // omitted fields and alternative numeric spellings.
        match payload {
            Some(raw) => {
                let prefix = &envelope[..envelope.len() - 1];
                let suffix = body
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_prefix(",\"payload\":"));
                if suffix.and_then(|s| s.strip_suffix('}')) != Some(raw.get()) {
                    return Err(());
                }
            }
            None if body != envelope => return Err(()),
            None => {}
        }
        Ok(Self {
            kind,
            channel,
            origin,
            envelope,
            payload,
        })
    }
}
