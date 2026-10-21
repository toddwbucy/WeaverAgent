//! The member's single immutable result, held until an explicit release.

use weaver_types::state_flow::{DecimalU64, ReadPage, ReadRefusalCode};

const PAGE_BYTES: usize = 524_288;

#[derive(Default)]
pub(super) struct Pages {
    next_id: u64,
    held: Option<Held>,
}

struct Held {
    id: DecimalU64,
    bytes: Vec<u8>,
    offset: usize,
    pending_end: Option<usize>,
}

impl Pages {
    pub(super) fn busy(&self) -> bool {
        self.held.is_some()
    }
    #[cfg(test)]
    pub(super) fn held_bytes(&self) -> usize {
        self.held.as_ref().map_or(0, |h| h.bytes.len())
    }

    /// Prepare and validate the first page before publishing any cursor identity.
    pub(super) fn start(
        &mut self,
        bytes: Vec<u8>,
        limit: DecimalU64,
        page_budget: usize,
    ) -> Result<ReadPage, ReadRefusalCode> {
        if self.busy() {
            return Err(ReadRefusalCode::ReadBusy);
        }
        if bytes.len() as u64 > limit.0 {
            return Err(ReadRefusalCode::ResultTooLarge);
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or(ReadRefusalCode::CounterExhausted)?;
        let mut held = Held {
            id: id.into(),
            bytes,
            offset: 0,
            pending_end: None,
        };
        let page = held.page(page_budget)?;
        self.next_id = id;
        self.held = Some(held);
        Ok(page)
    }

    pub(super) fn next(
        &mut self,
        id: DecimalU64,
        offset: DecimalU64,
        page_budget: usize,
    ) -> Result<ReadPage, ReadRefusalCode> {
        let held = self.held.as_mut().ok_or(ReadRefusalCode::UnknownSnapshot)?;
        if held.id != id {
            return Err(ReadRefusalCode::UnknownSnapshot);
        }
        if held.pending_end.is_some()
            || held.offset as u64 != offset.0
            || held.offset == held.bytes.len()
        {
            return Err(ReadRefusalCode::WrongOffset);
        }
        held.page(page_budget)
    }

    pub(super) fn release(&mut self, id: DecimalU64) -> Result<(), ReadRefusalCode> {
        if self.held.as_ref().is_none_or(|h| h.id != id) {
            return Err(ReadRefusalCode::UnknownSnapshot);
        }
        self.held = None;
        Ok(())
    }

    /// Used only if the first complete enclosing answer cannot be encoded.
    pub(super) fn discard(&mut self) {
        self.held = None;
    }

    /// Publication is after the complete enclosing answer encodes successfully.
    /// Before then a refusal may not consume bytes the caller has never seen.
    pub(super) fn publish_page(&mut self) {
        if let Some(held) = self.held.as_mut()
            && let Some(end) = held.pending_end.take()
        {
            held.offset = end;
        }
    }

    pub(super) fn refuse_page(&mut self, initial: bool) {
        if initial {
            self.discard();
        } else if let Some(held) = self.held.as_mut() {
            held.pending_end = None;
        }
    }
}

impl Held {
    fn page(&mut self, page_budget: usize) -> Result<ReadPage, ReadRefusalCode> {
        // Three raw octets require four output octets. The caller reserves the
        // complete wrapper and worst-case observation before lending this budget.
        let count = (page_budget / 4 * 3)
            .min(PAGE_BYTES)
            .min(self.bytes.len() - self.offset);
        if count == 0 {
            return Err(ReadRefusalCode::ResultUnavailable);
        }
        let end = self.offset + count;
        let page = ReadPage {
            snapshot_id: self.id,
            offset: (self.offset as u64).into(),
            total_bytes: (self.bytes.len() as u64).into(),
            octets: base64(&self.bytes[self.offset..end])?,
            done: end == self.bytes.len(),
        };
        self.pending_end = Some(end);
        Ok(page)
    }
}

fn base64(bytes: &[u8]) -> Result<String, ReadRefusalCode> {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    out.try_reserve(bytes.len().div_ceil(3) * 4)
        .map_err(|_| ReadRefusalCode::ResultUnavailable)?;
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(DIGITS[(a >> 2) as usize] as char);
        out.push(DIGITS[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            DIGITS[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            DIGITS[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_immutable_and_final_page_requires_release() {
        let mut pages = Pages::default();
        let raw = "\u{1f980}".repeat(PAGE_BYTES / 4 + 2).into_bytes();
        let limit = DecimalU64(raw.len() as u64);
        let first = pages.start(raw.clone(), limit, 4 * 100).unwrap();
        pages.publish_page();
        assert!(!first.done);
        assert_eq!(
            pages.start(b"x".to_vec(), limit, 100),
            Err(ReadRefusalCode::ReadBusy)
        );
        let mut got = first.decode_octets(limit).unwrap();
        assert_eq!(
            pages.next(first.snapshot_id, 1.into(), 100),
            Err(ReadRefusalCode::WrongOffset)
        );
        loop {
            let next = pages
                .next(first.snapshot_id, (got.len() as u64).into(), usize::MAX / 2)
                .unwrap();
            pages.publish_page();
            got.extend(next.decode_octets(limit).unwrap());
            if next.done {
                break;
            }
        }
        assert_eq!(got, raw);
        assert!(pages.busy());
        assert_eq!(
            pages.next(first.snapshot_id, limit, 100),
            Err(ReadRefusalCode::WrongOffset)
        );
        assert_eq!(
            pages.release(999.into()),
            Err(ReadRefusalCode::UnknownSnapshot)
        );
        pages.release(first.snapshot_id).unwrap();
        assert!(!pages.busy());
    }

    #[test]
    fn first_page_failures_leave_no_hidden_cursor() {
        let mut pages = Pages::default();
        assert_eq!(
            pages.start(b"too big".to_vec(), 2.into(), 100),
            Err(ReadRefusalCode::ResultTooLarge)
        );
        assert_eq!(
            pages.start(b"valid".to_vec(), 10.into(), 0),
            Err(ReadRefusalCode::ResultUnavailable)
        );
        assert_eq!(pages.held_bytes(), 0);
        let first = pages.start(b"valid".to_vec(), 10.into(), 100).unwrap();
        assert_eq!(first.snapshot_id, 1.into());
        assert!(first.done);
    }

    #[test]
    fn failed_next_preserves_cursor_and_counter_exhaustion_never_wraps() {
        let mut pages = Pages::default();
        let first = pages.start(vec![0; 10], 10.into(), 4).unwrap();
        pages.publish_page();
        assert_eq!(
            pages.next(first.snapshot_id, 3.into(), 0),
            Err(ReadRefusalCode::ResultUnavailable)
        );
        assert_eq!(
            pages.next(first.snapshot_id, 3.into(), 20).unwrap().offset,
            3.into()
        );
        pages.discard();
        pages.next_id = u64::MAX;
        assert_eq!(
            pages.start(vec![1], 1.into(), 100),
            Err(ReadRefusalCode::CounterExhausted)
        );
        assert!(!pages.busy());
    }

    #[test]
    fn enclosing_next_refusal_does_not_consume_unpublished_bytes() {
        let mut pages = Pages::default();
        let first = pages.start(b"abcdefghij".to_vec(), 10.into(), 4).unwrap();
        pages.publish_page();
        let unpublished = pages.next(first.snapshot_id, 3.into(), 4).unwrap();
        assert_eq!(unpublished.decode_octets(10.into()).unwrap(), b"def");
        // The complete enclosing answer fails after this page was prepared.
        pages.refuse_page(false);
        let retry = pages.next(first.snapshot_id, 3.into(), 4).unwrap();
        assert_eq!(retry, unpublished);
        pages.publish_page();
        assert_eq!(
            pages
                .next(first.snapshot_id, 6.into(), 4)
                .unwrap()
                .decode_octets(10.into())
                .unwrap(),
            b"ghi"
        );
    }
}
