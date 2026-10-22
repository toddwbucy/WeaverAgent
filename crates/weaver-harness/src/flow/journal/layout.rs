//! Canonical cursor and source arithmetic. No token positions are inferred here.

use super::*;
use crate::flow::material::SourceMapEntry;
use std::cmp::Ordering;
use weaver_traits::ContentBlock;
use weaver_types::state_flow::{Span, StepQueryRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlacedSource {
    pub material: MaterialRef,
    pub message_id: DecimalU64,
    pub block: DecimalU64,
    pub start: Option<DecimalU64>,
    pub end: Option<DecimalU64>,
}

#[derive(Debug, Clone)]
pub(super) struct RetainedMessage {
    pub id: DecimalU64,
    pub message: Message,
    pub sources: Vec<PlacedSource>,
    // Retain the immutable read proof even across a partial canonical rebuild.
    pub material: Option<Arc<SelectedUnit>>,
    pub occurrence: Option<UnitOccurrenceId>,
    pub step_queries: Vec<StepQueryRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Position {
    // Zero is head; otherwise the one-based current layout position, not ID.
    pub message: usize,
    block: usize,
    byte: usize,
}

pub(super) fn end(messages: &[RetainedMessage]) -> ArgumentCursor {
    messages
        .last()
        .map_or(ArgumentCursor::Head {}, |message| ArgumentCursor::Message {
            message_id: message.id,
            block_end: (message.message.content.len() as u64).into(),
            byte_end: None,
        })
}

pub(super) fn position(messages: &[RetainedMessage], cursor: &ArgumentCursor) -> Result<Position> {
    let ArgumentCursor::Message {
        message_id,
        block_end,
        byte_end,
    } = cursor
    else {
        return Ok(Position {
            message: 0,
            block: 0,
            byte: 0,
        });
    };
    let index = messages
        .iter()
        .position(|message| message.id == *message_id)
        .ok_or(JournalError::InvalidCursor)?;
    let content = &messages[index].message.content;
    let block = usize::try_from(block_end.0).map_err(|_| JournalError::InvalidCursor)?;
    if block > content.len() || (block == 0 && (!content.is_empty() || byte_end.is_some())) {
        return Err(JournalError::InvalidCursor);
    }
    let byte = if block == 0 {
        0
    } else {
        match (&content[block - 1], byte_end) {
            (ContentBlock::Text { text }, Some(end)) => {
                let end = usize::try_from(end.0).map_err(|_| JournalError::InvalidCursor)?;
                if !text.is_char_boundary(end) {
                    return Err(JournalError::InvalidCursor);
                }
                end
            }
            (ContentBlock::Text { text }, None) => text.len(),
            (_, None) => usize::MAX,
            (_, Some(_)) => return Err(JournalError::InvalidCursor),
        }
    };
    Ok(Position {
        message: index + 1,
        block,
        byte,
    })
}

pub(super) fn validate_source(message: &RetainedMessage, source: &PlacedSource) -> Result<()> {
    if message.id != source.message_id {
        return Err(JournalError::InvalidMap);
    }
    let block = usize::try_from(source.block.0)
        .ok()
        .and_then(|block| message.message.content.get(block))
        .ok_or(JournalError::InvalidMap)?;
    match (block, source.start, source.end, &source.material) {
        (ContentBlock::Text { text }, Some(start), Some(end), material) => {
            let local = Span { start, end };
            if start >= end || local.slice(text).is_err() {
                return Err(JournalError::InvalidMap);
            }
            match material {
                MaterialRef::Text { span, .. }
                    if span.start < span.end && span.end.0 - span.start.0 == end.0 - start.0 => {}
                MaterialRef::Request { .. } => {}
                _ => return Err(JournalError::InvalidMap),
            }
        }
        (ContentBlock::ToolCall(_), None, None, MaterialRef::Request { .. })
        | (ContentBlock::ToolResult(_), None, None, MaterialRef::Returned { .. }) => {}
        _ => return Err(JournalError::InvalidMap),
    }
    Ok(())
}

/// Pair locations are whole or absent. A cut may not keep half a request.
pub(super) fn location_kept(
    messages: &[RetainedMessage],
    source: &PlacedSource,
    cut: Position,
) -> Result<bool> {
    let message = messages
        .iter()
        .find(|message| message.id == source.message_id)
        .ok_or(JournalError::InvalidMap)?;
    validate_source(message, source)?;
    let block_end = source
        .block
        .0
        .checked_add(1)
        .ok_or(JournalError::Exhausted)?;
    let end = position(
        messages,
        &ArgumentCursor::Message {
            message_id: source.message_id,
            block_end: block_end.into(),
            byte_end: source.end,
        },
    )?;
    if end <= cut {
        return Ok(true);
    }
    if let Some(start) = source.start {
        let start = position(
            messages,
            &ArgumentCursor::Message {
                message_id: source.message_id,
                block_end: block_end.into(),
                byte_end: Some(start),
            },
        )?;
        if start < cut {
            return Err(JournalError::InvalidPair);
        }
    }
    Ok(false)
}

pub(super) fn prefix(
    messages: &[RetainedMessage],
    cursor: &ArgumentCursor,
) -> Result<Vec<RetainedMessage>> {
    let cut = position(messages, cursor)?;
    if cut.message == 0 {
        return Ok(vec![]);
    }
    let mut kept = messages[..cut.message].to_vec();
    let message = kept.last_mut().expect("non-head position");
    let original = message.message.clone();
    message.message.content.truncate(cut.block);
    if let Some(ContentBlock::Text { text }) = message.message.content.last_mut() {
        text.truncate(cut.byte);
    }
    if message.message != original {
        message.step_queries.clear();
    }
    let mut sources = vec![];
    for source in &message.sources {
        validate_source(&messages[cut.message - 1], source)?;
        let block = usize::try_from(source.block.0).map_err(|_| JournalError::InvalidMap)?;
        let Some(content) = message.message.content.get(block) else {
            continue;
        };
        let mut source = source.clone();
        if let (ContentBlock::Text { text }, Some(start), Some(end)) =
            (content, source.start, source.end)
        {
            let through = end.0.min(text.len() as u64);
            if through <= start.0 {
                continue;
            }
            if through < end.0 {
                let MaterialRef::Text { span, .. } = &mut source.material else {
                    return Err(JournalError::InvalidPair);
                };
                span.end = span
                    .start
                    .0
                    .checked_add(through - start.0)
                    .ok_or(JournalError::Exhausted)?
                    .into();
                source.end = Some(through.into());
            }
        }
        sources.push(source);
    }
    message.sources = sources;
    Ok(kept)
}

/// MESSAGE-MAP orders distinct materials by the unit's map, not by the Message
/// walk: a later recall request can map back into an earlier Text Message.
pub(super) fn validate_coverage(
    source_map: &[SourceMapEntry],
    sources: &[MaterialRef],
) -> Result<()> {
    let mut distinct = vec![];
    for mapping in source_map {
        if !distinct.contains(&mapping.material) {
            distinct.push(mapping.material.clone());
        }
    }
    if distinct != sources {
        return Err(JournalError::InvalidMap);
    }
    Ok(())
}

pub(super) fn coverage(messages: &[RetainedMessage]) -> Vec<MaterialRef> {
    let mut result = vec![];
    for source in messages.iter().flat_map(|message| &message.sources) {
        if !result.contains(&source.material) {
            result.push(source.material.clone());
        }
    }
    result
}

pub(super) fn covers(coverage: &[MaterialRef], wanted: &MaterialRef) -> bool {
    coverage.iter().any(|held| match (held, wanted) {
        (
            MaterialRef::Text {
                source: a,
                span: held,
            },
            MaterialRef::Text {
                source: b,
                span: wanted,
            },
        ) => a == b && held.start <= wanted.start && wanted.end <= held.end,
        _ => held == wanted,
    })
}

pub(super) fn compare(
    messages: &[RetainedMessage],
    left: &ArgumentCursor,
    right: &ArgumentCursor,
) -> Result<Ordering> {
    Ok(position(messages, left)?.cmp(&position(messages, right)?))
}
