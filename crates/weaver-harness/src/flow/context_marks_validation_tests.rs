//! Inspection copies from real returned receipts exercise defensive association
//! checks. None of these helpers constructs or alters a receipt-bearing owner.

use super::*;

pub(in crate::flow::context) fn fault_author_is_next(context: &RecordingContext) -> bool {
    context
        .task_work()
        .is_some_and(|work| work.stage == Stage::Fault)
}

pub(in crate::flow::context) fn returned_fault_author(
    context: &RecordingContext,
) -> Option<(&AuthorFailure, bool)> {
    let work = context.task_work()?;
    work.returned_author
        .failure
        .as_ref()
        .map(|failure| (failure, work.fault_started))
}

/// Read-only inspection values from a real task's retained licensing evidence.
/// Typed identities and canonical protocol bytes do not depend on Debug output
/// and cannot reconstruct any private receipt-bearing owner.
#[derive(Debug, PartialEq)]
pub(in crate::flow::context) struct TaskMaterialEvidence {
    input: FiledInputEvidence,
    scope: TaskScopeRef,
    catalog: Vec<u8>,
    selector: Vec<u8>,
    snapshot: weaver_types::state_flow::DecimalU64,
    recall: Sequence,
    mark: Option<MarkEvidence>,
}

#[derive(Debug, PartialEq)]
struct FiledInputEvidence {
    message: Message,
    source: SourceBlockId,
    input_id: weaver_types::state_flow::DecimalU64,
    offered_sequence: weaver_types::state_flow::DecimalU64,
    turn: TurnRef,
    receipts: ReceiptEvidence,
}

#[derive(Debug, PartialEq)]
struct MarkEvidence {
    sequence: Sequence,
    recorded_ns: u64,
    receipts: ReceiptEvidence,
}

#[derive(Debug, PartialEq)]
struct ReceiptEvidence {
    sources: Vec<SourceBlockId>,
    complete: bool,
    cancelled: bool,
    retained: (u64, u64),
    committed: Vec<CommittedEvidence>,
}

#[derive(Debug, PartialEq)]
struct CommittedEvidence {
    source: SourceBlockId,
    filing: Vec<u8>,
    receipt: Vec<u8>,
    terminal: crate::flow::ledger::TerminalCandidate,
}

fn receipt_evidence(receipts: &ReceiptSet) -> ReceiptEvidence {
    let committed = receipts
        .dispositions()
        .map(|disposition| {
            let SourceDisposition::Committed(committed) = disposition else {
                panic!("a successful task retains only committed sources");
            };
            let LedgerFile::Statement(file) = committed.file() else {
                panic!("task input and mark are statement filings");
            };
            CommittedEvidence {
                source: committed.source().clone(),
                filing: serde_json::to_vec(file.as_ref()).unwrap(),
                receipt: serde_json::to_vec(committed.receipt()).unwrap(),
                terminal: committed.terminal().clone(),
            }
        })
        .collect();
    ReceiptEvidence {
        sources: receipts.sources().cloned().collect(),
        complete: receipts.is_complete(),
        cancelled: receipts.is_cancelled(),
        retained: receipts.retained(),
        committed,
    }
}

pub(in crate::flow::context) fn task_material_evidence(task: &BegunTask) -> TaskMaterialEvidence {
    let seated = &task.input.seated;
    TaskMaterialEvidence {
        input: FiledInputEvidence {
            message: seated.message.clone(),
            source: seated.source.clone(),
            input_id: seated.input_id,
            offered_sequence: seated.offered_sequence,
            turn: seated.turn.clone(),
            receipts: receipt_evidence(&task.input.receipt),
        },
        scope: task.scope.clone(),
        catalog: task.catalog.raw_bytes().to_vec(),
        selector: serde_json::to_vec(task.catalog.selector()).unwrap(),
        snapshot: task.catalog.snapshot_id(),
        recall: task.catalog.recall_sequence(),
        mark: task.mark.as_ref().map(|mark| MarkEvidence {
            sequence: mark.sequence(),
            recorded_ns: mark.recorded_ns(),
            receipts: receipt_evidence(mark.receipts()),
        }),
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
