//! Inspection copies from actual committed terminal receipts pin defensive
//! associations. No copied value replaces a receipt-bearing owner.

use super::*;

pub(in crate::flow::context) fn receipt_associations(
    filing: &FilingCompletion,
    session: &str,
    run: &str,
    scheme: &str,
) {
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
    // These expected identities come from the real context and its declaration,
    // independently of the committed receipt whose inspection copy is varied.
    assert!(boot_pair_source_matches(
        source.source(),
        run,
        filing.sequence()
    ));
    for field in ["run", "sequence", "block"] {
        let mut changed_source = source.source().clone();
        match field {
            "run" => changed_source.run = "different-current-run".into(),
            "sequence" => changed_source.sequence = (changed_source.sequence.0 + 1).into(),
            "block" => changed_source.block = 1.into(),
            _ => unreachable!(),
        }
        assert!(
            !boot_pair_source_matches(&changed_source, run, filing.sequence()),
            "changed source {field} was accepted"
        );
    }

    let matches = |candidate: &PairFile| {
        boot_pair_content_matches(candidate, session, run, scheme, Some(expected))
    };
    assert!(matches(file));
    for field in [
        "scheme",
        "session",
        "run",
        "kind",
        "channel",
        "origin",
        "subsystem",
        "causal_parent",
    ] {
        let mut changed = file.as_ref().clone();
        match field {
            "scheme" => changed.scheme_id = "different-scheme".into(),
            "session" => changed.source.session = "different-session".into(),
            "run" => {
                changed.source.run = "different-current-run".into();
                changed.block_id.run = changed.source.run.clone();
            }
            "kind" => changed.source.kind = "tool.call.started".into(),
            "channel" => changed.source.channel = "control".into(),
            "origin" => changed.source.origin = "runtime".into(),
            "subsystem" => changed.source.subsystem = "state".into(),
            "causal_parent" => changed.source.causal_parent = Some(1.into()),
            _ => unreachable!(),
        }
        assert!(
            changed.validate_structure().is_ok(),
            "changed {field} structure"
        );
        assert!(!matches(&changed), "changed {field} was accepted");
    }
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

    let mut changed = file.as_ref().clone();
    changed.raw_block = serde_json::value::RawValue::from_string("{}".into()).unwrap();
    assert!(changed.validate_structure().is_err());
    assert!(!matches(&changed), "malformed copied pair was accepted");
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
