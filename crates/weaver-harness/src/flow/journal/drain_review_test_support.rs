//! Defensive corruption of an already genuine frozen projection, never a grant.
use super::*;
use crate::flow::place::PreparedPrefix;

impl PreparedPrefix {
    pub(in crate::flow) fn replace_saved_n_for_drain_review(&mut self, n: u64) {
        assert_ne!(self.projection.n, n);
        assert!(n <= self.projection.resident_before);
        self.projection.n = n;
    }

    pub(in crate::flow) fn add_saved_source_for_drain_review(&mut self) -> MaterialRef {
        let source = self
            .projection
            .kept
            .iter()
            .flat_map(|message| &message.sources)
            .next()
            .cloned()
            .expect("actual placed source before the drain");
        let target = self
            .projection
            .kept
            .iter_mut()
            .find(|message| !message.sources.contains(&source))
            .expect("another genuine retained message has no such interval");
        // Keep the actual source value. Only the saved projection's assertion
        // that this other Message held it is corrupted; no journal, read proof,
        // placement capability, source lease or decoder state is manufactured.
        target.sources.push(source.clone());
        source.material
    }
}
