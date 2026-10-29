//! The root file's operation bodies are closed maps, never tuple alternatives.
use super::{Category, GroupDraft, PlanReplacement, ReviewSetupRef, StepDraft};
use crate::state_flow::codec::deserialize_object;
use serde::Deserialize;

type Create = (
    String,
    Vec<StepDraft>,
    Vec<Category>,
    Vec<GroupDraft>,
    ReviewSetupRef,
);
type Authorize = (String, Vec<Category>, Vec<GroupDraft>, ReviewSetupRef);

pub(super) fn create<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Create, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        task: String,
        steps: Vec<StepDraft>,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    }
    let f: Fields = deserialize_object(d)?;
    Ok((f.task, f.steps, f.categories, f.groups, f.review))
}

pub(super) fn authorize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Authorize, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        task: String,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    }
    let f: Fields = deserialize_object(d)?;
    Ok((f.task, f.categories, f.groups, f.review))
}

pub(super) fn answer_break<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<PlanReplacement, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        replacement: PlanReplacement,
    }
    let f: Fields = deserialize_object(d)?;
    Ok(f.replacement)
}

pub(super) fn named_act<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        name: String,
    }
    let f: Fields = deserialize_object(d)?;
    Ok(f.name)
}

pub(super) fn recall_act<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        verb: String,
    }
    let f: Fields = deserialize_object(d)?;
    Ok(f.verb)
}
