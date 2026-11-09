//! The replaced decode and its marks, and the stream's pending buffer, per
//! `weaver-spu-Spec` sections 6 and 9 (#493). No model reaches the serve
//! path on a host, so these drive the two pieces of it that judge bytes:
//! [`replaced`], which `Resident::emission` applies to the engine's bytes,
//! and [`PieceBuffer`], which the stream feeds one token's piece at a time.

use super::*;

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

/// The record's text with each mark's bytes put back: the model's bytes.
fn restored(text: &str, marks: &[InvalidUtf8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut from = 0;
    for mark in marks {
        let at = mark.at as usize;
        out.extend_from_slice(&text.as_bytes()[from..at]);
        assert_eq!(&text[at..at + 3], "\u{fffd}", "each mark names a U+FFFD");
        out.extend_from_slice(&bytes(&mark.bytes));
        from = at + 3;
    }
    out.extend_from_slice(&text.as_bytes()[from..]);
    out
}

/// What Granite 4.2 30B emitted on held-out t004 (2026-11-08): "Perfect! ",
/// a lead byte with no continuation, "(", a lone continuation byte, "89". Before #493
/// this faulted the turn and lost the generation.
const T004: &str = "506572666563742120e228a13839";

#[test]
fn an_emission_that_is_not_utf8_is_kept_replaced_and_marked() {
    let raw = bytes(T004);
    let (text, marks) = replaced(&raw);
    assert_eq!(text, String::from_utf8_lossy(&raw));
    assert_eq!(text, "Perfect! \u{fffd}(\u{fffd}89");
    assert_eq!(
        marks,
        vec![
            InvalidUtf8 {
                at: 9,
                bytes: "e2".into()
            },
            InvalidUtf8 {
                at: 13,
                bytes: "a1".into()
            },
        ]
    );
    assert_eq!(restored(&text, &marks), raw, "the mark is lossless");
}

#[test]
fn a_utf8_emission_carries_no_mark() {
    let raw = "caf\u{e9} \u{1f600}".as_bytes();
    assert_eq!(replaced(raw), (String::from("caf\u{e9} \u{1f600}"), vec![]));
}

#[test]
fn a_replacement_character_the_model_drew_is_not_marked() {
    let mut raw = "a\u{fffd}b".as_bytes().to_vec();
    raw.push(0xff);
    let (text, marks) = replaced(&raw);
    assert_eq!(text, "a\u{fffd}b\u{fffd}");
    assert_eq!(
        marks,
        vec![InvalidUtf8 {
            at: 5,
            bytes: "ff".into()
        }]
    );
    assert_eq!(restored(&text, &marks), raw);
}

#[test]
fn a_trailing_incomplete_sequence_is_replaced_and_marked() {
    let raw = bytes("6869e282");
    let (text, marks) = replaced(&raw);
    assert_eq!(text, "hi\u{fffd}");
    assert_eq!(
        marks,
        vec![InvalidUtf8 {
            at: 2,
            bytes: "e282".into()
        }]
    );
}

#[test]
fn a_character_split_across_pieces_crosses_whole_once_complete() {
    let mut buffer = PieceBuffer::default();
    // U+20AC, the euro sign, split one byte per token.
    assert_eq!(buffer.push(&[0xe2]), None, "an incomplete tail waits");
    assert_eq!(buffer.push(&[0x82]), None);
    assert_eq!(buffer.push(&[0xac]).as_deref(), Some("\u{20ac}"));
}

#[test]
fn a_decided_prefix_crosses_and_the_incomplete_tail_waits() {
    let mut buffer = PieceBuffer::default();
    assert_eq!(buffer.push(&bytes("6869e282")).as_deref(), Some("hi"));
    assert_eq!(buffer.push(&[0xac]).as_deref(), Some("\u{20ac}"));
}

#[test]
fn a_sequence_no_byte_can_complete_crosses_replaced() {
    let mut buffer = PieceBuffer::default();
    assert_eq!(buffer.push(&[0xff]).as_deref(), Some("\u{fffd}"));
    // The stream goes on after it: before #493 every later piece waited.
    assert_eq!(buffer.push(b"ok").as_deref(), Some("ok"));
}

#[test]
fn an_empty_piece_crosses_as_an_empty_piece() {
    let mut buffer = PieceBuffer::default();
    assert_eq!(buffer.push(b"").as_deref(), Some(""));
    assert_eq!(buffer.push(&[0xe2]), None);
    assert_eq!(buffer.push(b""), None, "nothing decided while a tail waits");
}

/// The stream and the emission agree: the pieces crossed, then the tail
/// still undecided at the close (which never crosses), make the emission.
#[test]
fn the_streamed_pieces_are_the_emission_up_to_the_undecided_tail() {
    let raw = bytes(&format!("{T004}e2"));
    let mut buffer = PieceBuffer::default();
    let streamed: String = raw.iter().filter_map(|b| buffer.push(&[*b])).collect();
    let (emission, marks) = replaced(&raw);
    assert_eq!(format!("{streamed}\u{fffd}"), emission);
    assert_eq!(marks.last().map(|m| m.bytes.as_str()), Some("e2"));
    // Whole pieces decide the same as single bytes.
    let mut whole = PieceBuffer::default();
    assert_eq!(whole.push(&raw).as_deref(), Some(streamed.as_str()));
}

/// **An engine fault on a piece faults; it is not held as a tail**, the
/// second behaviour #493 changes. The stream's site in `main.rs` takes this
/// step for every drawn token, and the phase ends on `Step::Fault`.
///
/// Perturbation: map `Err` to `Step::Wait`, the strict conversion's old
/// silence, and this fails.
#[test]
fn an_engine_fault_on_a_piece_faults_rather_than_waiting() {
    let mut buffer = PieceBuffer::default();
    assert_eq!(buffer.step(Ok(vec![0xe2])), Step::Wait);
    let fault = DecodeFault::Engine {
        detail: "piece not rendered".into(),
    };
    assert_eq!(buffer.step(Err(fault.clone())), Step::Fault(fault));
    assert_eq!(
        buffer.step(Ok(b"ok".to_vec())),
        Step::Cross("\u{fffd}ok".into())
    );
}

/// **The stream's rule and the emission's are one rule**: `push` judges
/// with `from_utf8`'s errors and `replaced` with `utf8_chunks`, and over
/// every chunking of every byte string the pieces streamed, closed by a
/// byte that completes no sequence, are the emission of the same bytes.
/// Fixed-seed generation over lead, continuation, surrogate-range and
/// never-valid bytes, so a divergence in either is caught here.
#[test]
fn the_stream_and_the_emission_decide_alike_over_any_chunking() {
    const ALPHABET: [u8; 15] = [
        b'A', 0xc3, 0xa9, 0xe2, 0x82, 0xac, 0xf0, 0x9f, 0x98, 0x80, 0xff, 0xed, 0xa0, 0xc0, 0xbf,
    ];
    let mut state: u64 = 0x2026_1108;
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % bound
    };
    for _ in 0..4000 {
        let mut raw: Vec<u8> = (0..next(14))
            .map(|_| ALPHABET[next(ALPHABET.len())])
            .collect();
        let mut buffer = PieceBuffer::default();
        let mut streamed = String::new();
        let mut at = 0;
        while at < raw.len() {
            let take = 1 + next(4).min(raw.len() - at - 1);
            if let Some(piece) = buffer.push(&raw[at..at + take]) {
                streamed.push_str(&piece);
            }
            at += take;
        }
        streamed.push_str(&buffer.push(b"\n").expect("a newline decides every tail"));
        raw.push(b'\n');
        assert_eq!(streamed, replaced(&raw).0, "bytes {raw:02x?}");
    }
}
