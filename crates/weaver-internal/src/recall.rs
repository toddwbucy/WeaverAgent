//! The pure noun-history query constructor from INTERNAL-QUERY.
//!
//! Success proves only that the caller supplied a nonempty exact subject. The
//! caller establishes its qualified binding and owns store I/O, pair accounting
//! and placement. This helper performs no retrieval and holds no state.

/// The sole refusal: the supplied subject has zero bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptySubject;

/// A query borrowing the complete original subject without changing its bytes.
///
/// Construction goes through [`query`], so an empty subject cannot be inserted
/// through a public field:
///
/// ```compile_fail,E0451
/// use weaver_internal::recall::RecallQuery;
/// let empty = RecallQuery { subject: "" };
/// ```
///
/// A query cannot outlive the caller's subject:
///
/// ```compile_fail,E0597
/// use weaver_internal::recall::query;
/// let retained;
/// {
///     let subject = String::from("item");
///     retained = query(&subject).unwrap();
/// }
/// assert_eq!(retained.subject(), "item");
/// ```
#[derive(Debug)]
pub struct RecallQuery<'a> {
    subject: &'a str,
}

impl<'a> RecallQuery<'a> {
    /// Return the original borrow, with the input's lifetime.
    pub fn subject(&self) -> &'a str {
        self.subject
    }
}

/// Borrow a nonempty subject exactly as supplied.
///
/// Whitespace, case and Unicode remain unchanged. Success says nothing about
/// whether history exists or whether any store query has succeeded.
pub fn query(subject: &str) -> Result<RecallQuery<'_>, EmptySubject> {
    if subject.is_empty() {
        Err(EmptySubject)
    } else {
        Ok(RecallQuery { subject })
    }
}

// The error's trait contract is a type property, independent of a run.
const _: fn() = || {
    fn required_traits<T: std::fmt::Debug + Clone + Copy + PartialEq + Eq>() {}
    required_traits::<EmptySubject>();
};

#[cfg(test)]
mod tests;
