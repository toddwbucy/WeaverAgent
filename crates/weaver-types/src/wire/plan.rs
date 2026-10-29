//! Closed plan-control fields shared by the lifecycle and trace consumers.
use crate::state_flow::{
    DecimalU64, Digest, PlanRef, PlanUpdateCase, PlanUpdateRefused, ReadRefusalCode,
    SourcePosition, codec::deserialize_object,
};
use serde::Deserialize;

pub(super) fn directive<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(crate::Cause, Digest, String), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "deserialize_object")]
        cause: crate::Cause,
        document_sha256: Digest,
        document: String,
    }
    let fields: Fields = deserialize_object(deserializer)?;
    Ok((fields.cause, fields.document_sha256, fields.document))
}

pub(super) fn answer<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(PlanRef, SourcePosition), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        plan: PlanRef,
        source: SourcePosition,
    }
    let fields: Fields = deserialize_object(deserializer)?;
    Ok((fields.plan, fields.source))
}

type RefusalFields = (
    PlanUpdateCase,
    String,
    Vec<DecimalU64>,
    Option<ReadRefusalCode>,
);

pub(super) fn refusal<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RefusalFields, D::Error> {
    let fields = PlanUpdateRefused::deserialize(deserializer)?;
    Ok((
        fields.case,
        fields.detail,
        fields.rejected_blocks,
        fields.read_code,
    ))
}

pub(super) fn ask<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {}
    let _: Fields = deserialize_object(deserializer)?;
    Ok(())
}
