//! The typed landing, per `weaver-state-Spec` section 3: the members the
//! seam's vocabulary names land as typed rows, and every other pair lands as
//! the canonical JSON it crossed as.
//!
//! Two sources name what is typed, per `weaver-harness-state-contract`'s
//! Vocabulary. The message kinds' `role` and `content` are the floor's
//! message model, decoded through the `weaver-traits` types the harness
//! rendered them from. The measurement's `perplexity`, `entropies` and
//! `surprisals` are the SPU's, read member by member with `weaver-spu-Spec`
//! section 6 authoritative, and this crate defines no measurement type.
//!
//! **A member lands typed only where its rows render back to the bytes that
//! crossed, and where every engine can hold them.** The split renders each
//! typed fragment through [`render`], the function every answer uses, and
//! compares it with the pair's value, so a typed row can never serve a
//! spelling the record did not hold. A value that does not decode, decodes
//! and renders differently, or decodes to a string carrying U+0000, lands
//! verbatim instead, a rule the retired service engine's `TEXT` forced and
//! that stands until the loop's schema re-elects it, and custody keeps it
//! whole either way.

use weaver_traits::{ContentBlock, Role, ToolCall, ToolResultBlock};

/// The kinds whose `role` and `content` land typed: the conversation
/// message kinds, the four the recall serves and a restored one.
pub(crate) const MESSAGE_KINDS: [&str; 5] = [
    "message.system",
    "message.user",
    "message.assistant",
    "message.tool_result",
    "message.restored",
];

/// The kind whose named readings land typed.
pub(crate) const MEASUREMENT_KIND: &str = "model.measurement";

/// The two series members, each an array of readings in decode order.
const SERIES: [&str; 2] = ["entropies", "surprisals"];

/// One distillate's typed rows, as an engine lands them and reads them back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Typed {
    pub message: Option<MessageRow>,
    pub parts: Vec<PartRow>,
    pub measurement: Option<MeasurementRow>,
    pub series: Vec<SeriesRow>,
}

/// A message's typed members. `None` is a member that did not land typed,
/// which is not the same fact as an empty one: `parts` of zero is a content
/// member holding no block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageRow {
    /// The role as the floor spells it, `system` through `tool_result`.
    pub role: Option<String>,
    /// How many part rows the content holds.
    pub parts: Option<i64>,
}

/// One content block. `block` is the floor's tag, and the columns it does
/// not use are `None`: `text` for a text block, `name` and `arguments` for a
/// tool call, `content` for a tool result.
#[derive(Debug, Clone, PartialEq)]
pub struct PartRow {
    pub ordinal: i64,
    pub block: String,
    pub text: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
    pub content: Option<String>,
}

/// A measurement's typed readings. **An absent reading is `None` and never
/// zero**, the SPU rendering an unproduced reading as no member at all, per
/// `weaver-spu-Spec` section 6. A series column holds its length, its values
/// being [`SeriesRow`]s.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeasurementRow {
    pub perplexity: Option<f64>,
    pub entropies: Option<i64>,
    pub surprisals: Option<i64>,
}

/// One reading of a series member, in decode order.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesRow {
    pub member: String,
    pub ordinal: i64,
    pub value: f64,
}

/// Split one distillate's pairs into the typed rows and the pairs that land
/// verbatim, per the module's rule: typed where the vocabulary names the
/// member, the rows render back exactly, and every engine can hold them,
/// verbatim otherwise.
pub fn split(kind: &str, pairs: &[(String, String)]) -> (Typed, Vec<(String, String)>) {
    let mut typed = Typed::default();
    let mut verbatim = Vec::new();
    for (key, value) in pairs {
        match fragment(kind, key, value) {
            Some(piece)
                if piece.holdable()
                    && render_member(key, &piece).ok().flatten().as_deref() == Some(value) =>
            {
                typed.absorb(piece)
            }
            _ => verbatim.push((key.clone(), value.clone())),
        }
    }
    (typed, verbatim)
}

/// Render the typed rows back into the pairs they landed from, sorted by
/// key. An error is rows that cannot be read as the members they hold, a
/// part or a reading missing from its count, which a store that landed them
/// whole does not produce.
pub fn render(typed: &Typed) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for key in ["content", "entropies", "perplexity", "role", "surprisals"] {
        if let Some(value) = render_member(key, typed)? {
            pairs.push((key.to_string(), value));
        }
    }
    Ok(pairs)
}

/// The pairs one event serves: the verbatim pairs and the typed members
/// rendered back, sorted by key, which is the order the frame the pairs
/// crossed in held them.
pub fn served(
    mut verbatim: Vec<(String, String)>,
    typed: &Typed,
) -> Result<Vec<(String, String)>, String> {
    verbatim.extend(render(typed)?);
    verbatim.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(verbatim)
}

impl Typed {
    /// Whether any typed row stands, so an engine skips the inserts of an
    /// event that landed verbatim whole.
    pub fn is_empty(&self) -> bool {
        self.message.is_none() && self.measurement.is_none()
    }

    /// Whether every engine can hold these rows. A decoded string carrying
    /// U+0000 lands verbatim, where JSON escapes it, the rule the retired
    /// service engine's `TEXT` forced, per `weaver-state-Spec` section 3.
    fn holdable(&self) -> bool {
        let clean = |text: &Option<String>| text.as_deref().is_none_or(|t| !t.contains('\0'));
        self.message.as_ref().is_none_or(|m| clean(&m.role))
            && self.parts.iter().all(|part| {
                !part.block.contains('\0')
                    && clean(&part.text)
                    && clean(&part.name)
                    && clean(&part.arguments)
                    && clean(&part.content)
            })
            && self
                .series
                .iter()
                .all(|reading| !reading.member.contains('\0'))
    }

    fn absorb(&mut self, piece: Typed) {
        if let Some(message) = piece.message {
            let held = self.message.get_or_insert_with(MessageRow::default);
            held.role = held.role.take().or(message.role);
            held.parts = held.parts.take().or(message.parts);
        }
        if let Some(measurement) = piece.measurement {
            let held = self.measurement.get_or_insert_with(MeasurementRow::default);
            held.perplexity = held.perplexity.take().or(measurement.perplexity);
            held.entropies = held.entropies.take().or(measurement.entropies);
            held.surprisals = held.surprisals.take().or(measurement.surprisals);
        }
        self.parts.extend(piece.parts);
        self.series.extend(piece.series);
    }
}

/// The typed rows of one member, or `None` where the vocabulary does not name
/// it for this kind or its value does not decode as the named type.
fn fragment(kind: &str, key: &str, value: &str) -> Option<Typed> {
    let message = MESSAGE_KINDS.contains(&kind);
    let measurement = kind == MEASUREMENT_KIND;
    match key {
        "role" if message => {
            let role: Role = serde_json::from_str(value).ok()?;
            let name = serde_json::to_value(role).ok()?.as_str()?.to_string();
            Some(Typed {
                message: Some(MessageRow {
                    role: Some(name),
                    parts: None,
                }),
                ..Typed::default()
            })
        }
        "content" if message => {
            let blocks: Vec<ContentBlock> = serde_json::from_str(value).ok()?;
            let parts = blocks
                .into_iter()
                .enumerate()
                .map(|(ordinal, block)| part_row(ordinal as i64, block))
                .collect::<Option<Vec<_>>>()?;
            Some(Typed {
                message: Some(MessageRow {
                    role: None,
                    parts: Some(parts.len() as i64),
                }),
                parts,
                ..Typed::default()
            })
        }
        "perplexity" if measurement => Some(Typed {
            measurement: Some(MeasurementRow {
                perplexity: Some(serde_json::from_str(value).ok()?),
                ..MeasurementRow::default()
            }),
            ..Typed::default()
        }),
        member if measurement && SERIES.contains(&member) => {
            let readings: Vec<f64> = serde_json::from_str(value).ok()?;
            let length = Some(readings.len() as i64);
            let mut row = MeasurementRow::default();
            if member == "entropies" {
                row.entropies = length;
            } else {
                row.surprisals = length;
            }
            Some(Typed {
                measurement: Some(row),
                series: readings
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, value)| SeriesRow {
                        member: member.to_string(),
                        ordinal: ordinal as i64,
                        value,
                    })
                    .collect(),
                ..Typed::default()
            })
        }
        _ => None,
    }
}

/// One block as a part row, or `None` for a block the floor added after this
/// crate was built, which then lands verbatim rather than typed in part.
fn part_row(ordinal: i64, block: ContentBlock) -> Option<PartRow> {
    let mut row = PartRow {
        ordinal,
        block: String::new(),
        text: None,
        name: None,
        arguments: None,
        content: None,
    };
    match block {
        ContentBlock::Text { text } => {
            row.block = "text".into();
            row.text = Some(text);
        }
        ContentBlock::ToolCall(ToolCall { name, arguments }) => {
            row.block = "tool_call".into();
            row.name = Some(name);
            row.arguments = Some(arguments);
        }
        ContentBlock::ToolResult(ToolResultBlock { content }) => {
            row.block = "tool_result".into();
            row.content = Some(content);
        }
        _ => return None,
    }
    Some(row)
}

/// The block a part row holds, or the reason it holds none.
fn block_of(row: &PartRow) -> Result<ContentBlock, String> {
    let missing = |column: &str| format!("part {} ({}) has no {column}", row.ordinal, row.block);
    Ok(match row.block.as_str() {
        "text" => ContentBlock::Text {
            text: row.text.clone().ok_or_else(|| missing("text"))?,
        },
        "tool_call" => ContentBlock::ToolCall(ToolCall {
            name: row.name.clone().ok_or_else(|| missing("name"))?,
            arguments: row.arguments.clone().ok_or_else(|| missing("arguments"))?,
        }),
        "tool_result" => ContentBlock::ToolResult(ToolResultBlock {
            content: row.content.clone().ok_or_else(|| missing("content"))?,
        }),
        other => return Err(format!("part {} names no block {other:?}", row.ordinal)),
    })
}

/// One member rendered from the typed rows, `None` where it did not land
/// typed. Shared by the split's exactness check and every answer, so the
/// spelling checked at landing is the spelling served.
fn render_member(key: &str, typed: &Typed) -> Result<Option<String>, String> {
    let json = |r: serde_json::Result<String>| r.map_err(|e| e.to_string());
    match key {
        "role" => match typed.message.as_ref().and_then(|m| m.role.clone()) {
            Some(name) => {
                let role: Role = serde_json::from_value(serde_json::Value::String(name))
                    .map_err(|e| e.to_string())?;
                json(serde_json::to_string(&role)).map(Some)
            }
            None => Ok(None),
        },
        "content" => match typed.message.as_ref().and_then(|m| m.parts) {
            Some(count) => {
                let mut rows: Vec<&PartRow> = typed.parts.iter().collect();
                rows.sort_by_key(|row| row.ordinal);
                if rows.len() as i64 != count
                    || rows
                        .iter()
                        .enumerate()
                        .any(|(i, row)| row.ordinal != i as i64)
                {
                    return Err(format!(
                        "content holds {count} parts and {} stand",
                        rows.len()
                    ));
                }
                let blocks = rows
                    .into_iter()
                    .map(block_of)
                    .collect::<Result<Vec<_>, _>>()?;
                json(serde_json::to_string(&blocks)).map(Some)
            }
            None => Ok(None),
        },
        "perplexity" => match typed.measurement.as_ref().and_then(|m| m.perplexity) {
            Some(perplexity) => json(serde_json::to_string(&perplexity)).map(Some),
            None => Ok(None),
        },
        member if SERIES.contains(&member) => {
            let length = typed.measurement.as_ref().and_then(|m| {
                if member == "entropies" {
                    m.entropies
                } else {
                    m.surprisals
                }
            });
            match length {
                Some(count) => {
                    let mut rows: Vec<&SeriesRow> =
                        typed.series.iter().filter(|r| r.member == member).collect();
                    rows.sort_by_key(|row| row.ordinal);
                    if rows.len() as i64 != count
                        || rows
                            .iter()
                            .enumerate()
                            .any(|(i, row)| row.ordinal != i as i64)
                    {
                        return Err(format!(
                            "{member} holds {count} readings and {} stand",
                            rows.len()
                        ));
                    }
                    let readings: Vec<f64> = rows.into_iter().map(|row| row.value).collect();
                    json(serde_json::to_string(&readings)).map(Some)
                }
                None => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weaver_traits::Message;

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    /// A message's role and content land typed, one part row per block, and
    /// render back to the bytes the floor type spells.
    #[test]
    fn a_message_lands_typed_and_renders_back_exactly() {
        let message = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "four \"quoted\" words\n".into(),
                },
                ContentBlock::ToolCall(ToolCall {
                    name: "calculator".into(),
                    arguments: r#"{"a":1}"#.into(),
                }),
                ContentBlock::ToolResult(ToolResultBlock {
                    content: "2".into(),
                }),
            ],
        };
        let role = serde_json::to_string(&message.role).unwrap();
        let content = serde_json::to_string(&message.content).unwrap();
        let pairs = vec![pair("content", &content), pair("role", &role)];
        let (typed, verbatim) = split("message.assistant", &pairs);
        assert!(verbatim.is_empty(), "{verbatim:?}");
        assert_eq!(
            typed.message.as_ref().unwrap().role.as_deref(),
            Some("assistant")
        );
        assert_eq!(typed.parts.len(), 3);
        assert_eq!(typed.parts[1].name.as_deref(), Some("calculator"));
        assert_eq!(render(&typed).unwrap(), pairs);
    }

    /// An empty content is a member holding no block, not an absent member.
    #[test]
    fn an_empty_content_is_held_rather_than_absent() {
        let (typed, verbatim) = split("message.user", &[pair("content", "[]")]);
        assert!(verbatim.is_empty());
        assert_eq!(typed.message.as_ref().unwrap().parts, Some(0));
        assert_eq!(render(&typed).unwrap(), [pair("content", "[]")]);
    }

    /// A value the vocabulary does not name, or names and cannot type
    /// exactly, lands verbatim: a non-message kind's content, a content that
    /// is not the floor's shape, and a spelling the floor type would change.
    #[test]
    fn what_cannot_land_typed_exactly_lands_verbatim() {
        for (kind, key, value) in [
            ("model.output", "content", "[]"),
            ("message.user", "content", r#"{"z": 1.00}"#),
            ("message.user", "content", r#"[{"text":"a","type":"text"}]"#),
            (
                "message.user",
                "content",
                r#"[ {"type":"text","text":"a"} ]"#,
            ),
            ("message.user", "role", r#""narrator""#),
            ("message.user", "turn_note", r#""x""#),
            ("model.measurement", "perplexity", "1.50"),
            ("model.measurement", "entropies", "[1e0]"),
            ("model.measurement", "input_tokens", "[1,2]"),
        ] {
            let pairs = [pair(key, value)];
            let (typed, verbatim) = split(kind, &pairs);
            assert!(typed.is_empty(), "{kind} {key} {value} typed");
            assert_eq!(verbatim, pairs, "{kind} {key} {value}");
        }
    }

    /// **The measurement's absent readings stay absent**: a measurement that
    /// crossed with a perplexity and no series holds no series length and
    /// renders no series member, and one with no perplexity holds none
    /// rather than zero.
    #[test]
    fn an_absent_reading_is_absent_and_not_zero() {
        let (typed, _) = split(
            "model.measurement",
            &[pair("perplexity", "1.503386244415742")],
        );
        let row = typed.measurement.as_ref().unwrap();
        assert_eq!(row.perplexity, Some(1.503386244415742));
        assert_eq!((row.entropies, row.surprisals), (None, None));
        assert_eq!(
            render(&typed).unwrap(),
            [pair("perplexity", "1.503386244415742")]
        );

        let pairs = [pair("entropies", "[0.5,1.25]")];
        let (typed, verbatim) = split("model.measurement", &pairs);
        assert!(verbatim.is_empty());
        let row = typed.measurement.as_ref().unwrap();
        assert_eq!(row.perplexity, None, "an absent perplexity is not zero");
        assert_eq!(row.entropies, Some(2));
        assert_eq!(render(&typed).unwrap(), pairs);
    }

    /// **A string carrying U+0000 lands verbatim**, because the service
    /// engine's `TEXT` cannot hold it, and the rule is one rule for both
    /// engines. The escaped JSON holds it, so custody keeps the pair whole.
    #[test]
    fn a_nul_in_a_string_lands_verbatim() {
        for (key, value) in [
            ("content", r#"[{"type":"text","text":"a\u0000b"}]"#),
            (
                "content",
                r#"[{"type":"tool_call","name":"n\u0000","arguments":"{}"}]"#,
            ),
            ("content", r#"[{"type":"tool_result","content":"\u0000"}]"#),
        ] {
            let pairs = [pair(key, value)];
            let (typed, verbatim) = split("message.user", &pairs);
            assert!(typed.is_empty(), "{value} typed");
            assert_eq!(verbatim, pairs, "{value}");
        }
    }

    /// **A reading the SPU rendered lands typed and renders back exactly**,
    /// which holds only because the manifest turns on serde_json's
    /// `float_roundtrip`. The value is a perplexity from the HeroBench run of
    /// 2026-09-29, one of 780 readings that landed verbatim under the default
    /// parse, which reads it as a neighbouring double that renders as
    /// `1.4226275797516237`. Perturbation: drop the feature and this reading
    /// and the series beside it land verbatim, and the test fails on typed.
    #[test]
    fn a_rendered_reading_lands_typed_and_renders_back() {
        let pairs = [
            pair("perplexity", "1.4226275797516235"),
            pair("entropies", "[1.4226275797516235,0.000011356166396581102]"),
        ];
        let (typed, verbatim) = split("model.measurement", &pairs);
        assert!(verbatim.is_empty(), "landed verbatim: {verbatim:?}");
        let row = typed.measurement.as_ref().expect("typed");
        assert_eq!(row.perplexity, Some(1.4226275797516235));
        assert_eq!(row.entropies, Some(2));
        let mut served = render(&typed).unwrap();
        served.sort();
        let mut sent = pairs.to_vec();
        sent.sort();
        assert_eq!(served, sent);
    }

    /// Rows missing from their count are refused at read rather than served
    /// short, the one error the renderer has.
    #[test]
    fn a_count_its_rows_do_not_meet_is_refused() {
        let (mut typed, _) = split(
            "message.user",
            &[pair("content", r#"[{"type":"text","text":"a"}]"#)],
        );
        typed.parts.clear();
        assert!(render(&typed).is_err());
    }
}
