//! Inspection copies from real returned receipts exercise defensive association
//! checks. None of these helpers constructs or alters a receipt-bearing owner.

use super::*;

/// Read-only snapshot of a real task's retained licensing evidence. Debug text
/// here compares private receipts without exposing a constructor or wire form.
#[derive(Debug, PartialEq)]
pub(in crate::flow::context) struct TaskMaterialEvidence {
    input: String,
    scope: TaskScopeRef,
    catalog: Vec<u8>,
    recall: Sequence,
    mark: Option<String>,
}

pub(in crate::flow::context) fn task_material_evidence(
    task: &BegunTask,
) -> TaskMaterialEvidence {
    TaskMaterialEvidence {
        input: format!("{:?}", task.input),
        scope: task.scope.clone(),
        catalog: task.catalog.raw_bytes().to_vec(),
        recall: task.catalog.recall_sequence(),
        mark: task.mark.as_ref().map(|mark| format!("{mark:?}")),
    }
}

pub(in crate::flow::context) fn input_associations(task: &BegunTask) {
    let input = &task.input;
    let seated = &input.seated;
    let SourceDisposition::Committed(committed) = input.receipt.dispositions().next().unwrap()
    else {
        panic!("real task input was not committed");
    };
    let LedgerFile::Statement(file) = committed.file() else {
        panic!("real task input was not a statement");
    };
    let sequence = Some(Sequence(seated.source.sequence.0));
    let matches = |sequence, source: &SourceBlockId, file: &StatementFile| {
        input_receipt_matches(sequence, seated, &seated.turn, source, file)
    };
    assert!(matches(sequence, committed.source(), file));
    assert!(!matches(None, committed.source(), file));
    assert!(!matches(
        Some(Sequence(seated.source.sequence.0 + 1)),
        committed.source(),
        file
    ));
    let mut changed = file.as_ref().clone();
    changed.text = "Read different.".into();
    assert!(!matches(sequence, committed.source(), &changed));
    let mut changed_source = committed.source().clone();
    changed_source.block = 1.into();
    assert!(!matches(sequence, &changed_source, file));
    let mut changed = file.as_ref().clone();
    changed.source.causal_parent = None;
    assert!(!matches(sequence, committed.source(), &changed));
    let mut changed = file.as_ref().clone();
    changed.source.turn = Some("different-turn".into());
    assert!(!matches(sequence, committed.source(), &changed));
}

pub(in crate::flow::context) fn mark_associations(task: &BegunTask) {
    let filing = task.mark.as_ref().unwrap();
    let SourceDisposition::Committed(committed) = filing.receipts().dispositions().next().unwrap()
    else {
        panic!("real mark was not committed");
    };
    let LedgerFile::Statement(file) = committed.file() else {
        panic!("real mark was not a statement");
    };
    let run = &task.input.seated.source.run;
    assert!(mark_source_matches(
        committed.source(),
        run,
        filing.sequence()
    ));
    for field in 0..3 {
        let mut changed = committed.source().clone();
        match field {
            0 => changed.run = "another-run".into(),
            1 => changed.sequence = (changed.sequence.0 + 1).into(),
            _ => changed.block = 1.into(),
        }
        assert!(
            !mark_source_matches(&changed, run, filing.sequence()),
            "source field {field}"
        );
    }
    let Some(StatementEffect::Mark(effect)) = &file.effect else {
        panic!("real mark lost its effect");
    };
    let matches = |candidate: &StatementFile| {
        mark_content_matches(
            candidate,
            &task.input.seated.turn,
            &file.text,
            effect,
            false,
        )
    };
    assert!(matches(file));
    let mut changed = file.as_ref().clone();
    changed.text = "Read different.".into();
    assert!(!matches(&changed));
    let mut changed = file.as_ref().clone();
    changed.context = None;
    assert!(!matches(&changed));
    let mut changed = file.as_ref().clone();
    changed.effect = None;
    assert!(!matches(&changed));
}

pub(in crate::flow::context) fn cancelled_evidence(account: &TaskAccount) {
    let work = &account.work;
    assert!(work.settled);
    assert!(work.cancelled);
    assert!(work.token.is_none());
    assert!(work.scope.is_some());
    assert!(work.seated.is_some());
    assert!(work.input_receipt.as_ref().unwrap().is_complete());
    assert!(work.filing.as_ref().unwrap().receipts().is_complete());
    assert!(matches!(work.read, Some(ReadCompletion::Recorded(_))));
}
