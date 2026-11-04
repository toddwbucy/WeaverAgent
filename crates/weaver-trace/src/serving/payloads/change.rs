//! TRACE-EMISSION-CHANGE mirrors. Serialized values confer no change authority.
//!
//! ```compile_fail,E0277
//! fn reads<T: for<'a> serde::Deserialize<'a>>() {}
//! reads::<weaver_trace::serving::payloads::EmissionChangeRecord>();
//! ```

use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThesaurusMatch {
    pub source: SourceBlockId,
    pub previous_change: Option<DecimalU64>,
    pub sentence_span: Span,
    pub word_span: Span,
    pub scheme_id: String,
    pub thesaurus_sha256: Digest,
    pub entry_word: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmissionChangeRecord {
    pub matched: ThesaurusMatch,
    pub span: Span,
    pub replaced: String,
    pub replacement: String,
    pub method: EmissionChangeMethod,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmissionChangeMethod {
    Splice {},
    Offer {
        instruction: SourceBlockId,
        reemission: SourceBlockId,
        reemission_span: Span,
    },
}
