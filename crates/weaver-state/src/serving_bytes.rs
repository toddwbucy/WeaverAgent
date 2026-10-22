//! Checked logical buffer bytes for state-observation/2, not allocator usage.
//! Each owner counts actual materialized bytes or live lengths. Missing coverage
//! and overflow are sticky; a measured subtotal cannot fill an unavailable total.

use weaver_types::state_flow::ObservationUnavailable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferBytes(Result<u64, ObservationUnavailable>);

impl Default for BufferBytes {
    fn default() -> Self {
        Self(Ok(0))
    }
}

impl BufferBytes {
    pub const fn unavailable(reason: ObservationUnavailable) -> Self {
        Self(Err(reason))
    }

    pub const fn measured(value: u64) -> Self {
        Self(Ok(value))
    }

    pub fn add(&mut self, bytes: usize) {
        self.merge(match u64::try_from(bytes) {
            Ok(bytes) => Self::measured(bytes),
            Err(_) => Self::unavailable(ObservationUnavailable::Overflow),
        });
    }

    pub fn merge(&mut self, other: Self) {
        self.0 = self.0.and_then(|sum| {
            other.0.and_then(|bytes| {
                sum.checked_add(bytes)
                    .ok_or(ObservationUnavailable::Overflow)
            })
        });
    }

    pub const fn value(self) -> Result<u64, ObservationUnavailable> {
        self.0
    }
}

impl From<Option<u64>> for BufferBytes {
    fn from(value: Option<u64>) -> Self {
        Self(value.ok_or(ObservationUnavailable::Uninstrumented))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_and_missing_coverage_never_expose_a_partial_count() {
        let mut count = BufferBytes::measured(u64::MAX);
        count.add(1);
        count.add(3);
        assert_eq!(count.value(), Err(ObservationUnavailable::Overflow));
        let mut missing = BufferBytes::unavailable(ObservationUnavailable::Uninstrumented);
        missing.add(900);
        assert_eq!(missing.value(), Err(ObservationUnavailable::Uninstrumented));
        let mut measured = BufferBytes::measured(2);
        measured.merge(missing);
        assert_eq!(
            measured.value(),
            Err(ObservationUnavailable::Uninstrumented)
        );
    }
}
