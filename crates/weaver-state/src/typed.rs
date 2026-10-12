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
mod tests;
