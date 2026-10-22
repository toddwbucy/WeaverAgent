//! Inspection copies from actual committed terminal receipts pin defensive
//! associations. No copied value replaces a receipt-bearing owner.

use super::*;

pub(in crate::flow::context) fn receipt_associations(filing: &FilingCompletion) {
    let SourceDisposition::Committed(source) = filing.receipts().dispositions().next().unwrap()
    else {
        panic!("real boot terminal was not committed");
    };
    let LedgerFile::Pair(file) = source.file() else {
        panic!("real boot terminal was not a pair");
    };
    let PairRecord::Ended(expected) = &file.pair else {
        panic!("real boot terminal was not ended");
    };
    assert!(boot_pair_source_matches(
        source.source(),
        &source.common().run_id,
        filing.sequence()
    ));
    let mut changed_source = source.source().clone();
    changed_source.sequence = (changed_source.sequence.0 + 1).into();
    assert!(!boot_pair_source_matches(
        &changed_source,
        &source.common().run_id,
        filing.sequence()
    ));

    let matches = |candidate: &PairFile| {
        boot_pair_content_matches(
            candidate,
            &source.common().session_id,
            &source.common().run_id,
            &file.scheme_id,
            Some(expected),
        )
    };
    assert!(matches(file));
    let mut changed = file.as_ref().clone();
    changed.source.turn = Some("another-turn".into());
    assert!(changed.validate_structure().is_ok());
    assert!(!matches(&changed));

    let mut changed = file.as_ref().clone();
    let PairRecord::Ended(ended) = &mut changed.pair else {
        unreachable!()
    };
    ended.call_id.run = "another-prior-run".into();
    changed.raw_block = serde_json::value::to_raw_value(ended).unwrap();
    assert!(changed.validate_structure().is_ok());
    assert!(!matches(&changed));
}

pub(in crate::flow::context) fn retained_terminals(
    context: &RecordingContext,
) -> &[FilingCompletion] {
    &context
        .boot_pairs
        .as_ref()
        .expect("retained repair")
        .terminals
}
