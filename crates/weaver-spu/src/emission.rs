//! The emission's text from its bytes, per `weaver-spu-Spec` section 6 on the
//! operator's ruling of 2026-11-08 (#493): the replaced decode, with a mark
//! naming each sequence that was not UTF-8, and the stream's pending buffer
//! that crosses each piece once it is decided.
//!
//! **An emission that is not UTF-8 is not a fault** (section 9). The model
//! drew the tokens and their identifiers are the record, so the text keeps
//! them: each maximal subpart that is not UTF-8 becomes U+FFFD, the
//! conversion `String::from_utf8_lossy` makes, and the mark keeps the bytes
//! it replaced so the record loses nothing.

use weaver_types::InvalidUtf8;

use crate::decoder::backend::DecodeFault;

/// The replaced decode of `bytes` and its marks: the text
/// `String::from_utf8_lossy` gives, and one [`InvalidUtf8`] per U+FFFD this
/// conversion put in, naming that U+FFFD's byte offset in the text and the
/// bytes it replaced. The marks are empty exactly when `bytes` were UTF-8. A
/// U+FFFD the model drew itself is UTF-8 and carries no mark, which is why
/// the offsets are recorded rather than inferred from the text.
pub fn replaced(bytes: &[u8]) -> (String, Vec<InvalidUtf8>) {
    let mut text = String::with_capacity(bytes.len());
    let mut marks = Vec::new();
    for chunk in bytes.utf8_chunks() {
        text.push_str(chunk.valid());
        let invalid = chunk.invalid();
        if !invalid.is_empty() {
            marks.push(InvalidUtf8 {
                at: text.len() as u64,
                bytes: hex(invalid),
            });
            text.push(char::REPLACEMENT_CHARACTER);
        }
    }
    (text, marks)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        write!(out, "{byte:02x}").expect("writing to a String does not fail");
    }
    out
}

/// The stream's pending buffer, per the decode contract's section 2: each
/// drawn token's piece is appended as bytes, and the longest decided prefix
/// crosses. A complete character crosses as itself, a sequence no later byte
/// can complete crosses as U+FFFD, and a tail a later token may still
/// complete waits. A tail still undecided when the generation closes never
/// crosses: the emission, converted whole by [`replaced`], is the authority
/// and the stream is its rhythm. **On the Native container each piece is the
/// tokenizer's own rendering of one token, already replaced**, so a
/// character split across tokens streams there as U+FFFD halves while the
/// emission, decoded whole, holds it.
#[derive(Debug, Default)]
pub struct PieceBuffer {
    pending: Vec<u8>,
}

/// What one drawn token does to the stream, per [`PieceBuffer::step`].
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A decided piece crosses with this token.
    Cross(String),
    /// Only an incomplete tail is held: nothing crosses yet.
    Wait,
    /// The engine could not render the token's piece: a decode fault, which
    /// ends the phase as one rather than holding the stream.
    Fault(DecodeFault),
}

impl PieceBuffer {
    /// One drawn token's turn at the stream: the engine's rendering of its
    /// piece, or the engine's failure to render it. **A failure faults and
    /// is never held as a tail**: before #493 the stream converted strictly
    /// and waited on any failure, so an engine fault and an incomplete
    /// character both waited, silently, to the end of the generation.
    pub fn step(&mut self, piece: Result<Vec<u8>, DecodeFault>) -> Step {
        match piece {
            Err(fault) => Step::Fault(fault),
            Ok(bytes) => match self.push(&bytes) {
                Some(decided) => Step::Cross(decided),
                None => Step::Wait,
            },
        }
    }

    /// Append one token's piece and return what became emittable at it:
    /// `Some` with the decided text when anything was decided or the buffer
    /// is now empty (an empty piece crosses as an empty piece, as a token
    /// that renders nothing always did), and `None` while only an
    /// incomplete tail is held.
    pub fn push(&mut self, piece: &[u8]) -> Option<String> {
        self.pending.extend_from_slice(piece);
        let mut decided = String::new();
        let mut at = 0;
        loop {
            match std::str::from_utf8(&self.pending[at..]) {
                Ok(text) => {
                    decided.push_str(text);
                    at = self.pending.len();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // The prefix was just judged UTF-8 by the same call.
                    decided.push_str(
                        std::str::from_utf8(&self.pending[at..at + valid])
                            .expect("valid_up_to bounds a UTF-8 prefix"),
                    );
                    at += valid;
                    match error.error_len() {
                        // No later byte can complete it: decided, replaced.
                        Some(length) => {
                            decided.push(char::REPLACEMENT_CHARACTER);
                            at += length;
                        }
                        // The input ended inside a sequence: it waits.
                        None => break,
                    }
                }
            }
        }
        self.pending.drain(..at);
        (!decided.is_empty() || self.pending.is_empty()).then_some(decided)
    }
}

#[cfg(test)]
mod tests;
