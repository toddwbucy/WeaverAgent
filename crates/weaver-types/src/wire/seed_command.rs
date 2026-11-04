//! TYPES-PERSIST root seed command representations. No command executes here.

use crate::state_flow::{StartIdentity, codec::deserialize_object};
use serde::Deserialize;

pub(super) fn directive<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<crate::AgentName, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        agent: crate::AgentName,
    }
    let fields: Fields = deserialize_object(deserializer)?;
    Ok(fields.agent)
}

pub(super) fn answer<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<StartIdentity, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        start: StartIdentity,
    }
    let fields: Fields = deserialize_object(deserializer)?;
    Ok(fields.start)
}

#[cfg(test)]
mod tests {
    use crate::{AgentName, LifecycleAnswer, LifecycleDirective};

    #[test]
    fn seed_command_keeps_the_agent_only_and_refuses_extra_inputs() {
        let seed = LifecycleDirective::Seed {
            agent: AgentName("alpha".into()),
        };
        let text = r#"{"kind":"seed","agent":"alpha"}"#;
        assert_eq!(serde_json::to_string(&seed).unwrap(), text);
        assert_eq!(
            serde_json::from_str::<LifecycleDirective>(text).unwrap(),
            seed
        );
        for text in [
            r#"{"kind":"seed"}"#,
            r#"{"kind":"seed","agent":null}"#,
            r#"{"kind":"seed","kind":"seed","agent":"alpha"}"#,
            r#"{"kind":"seed","agent":"alpha","agent":"beta"}"#,
            r#"{"kind":"seed","agent":"alpha","path":"/tmp/prompt"}"#,
            r#"{"kind":"seed","agent":"alpha","content":"text"}"#,
            r#"{"kind":"seed","agent":"alpha","destination":"other"}"#,
        ] {
            assert!(
                serde_json::from_str::<LifecycleDirective>(text).is_err(),
                "{text}"
            );
        }
    }

    #[test]
    fn seeded_answer_keeps_the_complete_canonical_start_identity() {
        let value = serde_json::json!({
            "kind": "seeded",
            "start": {
                "seed_id": "seed", "artifact_id": "artifact",
                "dump_sha256": "a".repeat(64), "config_sha256": "b".repeat(64),
                "parent_artifact_id": null, "position": null
            }
        });
        let answer: LifecycleAnswer = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(answer).unwrap(), value);
        let start = serde_json::to_string(&value["start"]).unwrap();
        for text in [
            r#"{"kind":"seeded"}"#.to_owned(),
            r#"{"kind":"seeded","start":null}"#.to_owned(),
            r#"{"kind":"seeded","start":[]}"#.to_owned(),
            format!(r#"{{"kind":"seeded","start":{start},"start":{start}}}"#),
            format!(r#"{{"kind":"seeded","kind":"seeded","start":{start}}}"#),
            format!(
                r#"{{"kind":"seeded","start":{}}}"#,
                start.replacen(
                    "\"seed_id\":\"seed\"",
                    "\"seed_id\":\"seed\",\"seed_id\":\"other\"",
                    1
                )
            ),
        ] {
            assert!(
                serde_json::from_str::<LifecycleAnswer>(&text).is_err(),
                "{text}"
            );
        }
        for path in [
            "seed_id",
            "artifact_id",
            "dump_sha256",
            "config_sha256",
            "parent_artifact_id",
            "position",
        ] {
            let mut missing = value.clone();
            missing["start"].as_object_mut().unwrap().remove(path);
            assert!(
                serde_json::from_value::<LifecycleAnswer>(missing).is_err(),
                "{path}"
            );
        }
        let mut extra = value.clone();
        extra["extra"] = true.into();
        assert!(serde_json::from_value::<LifecycleAnswer>(extra).is_err());
        let mut extra = value;
        extra["start"]["extra"] = true.into();
        assert!(serde_json::from_value::<LifecycleAnswer>(extra).is_err());
    }
}
