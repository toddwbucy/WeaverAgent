use super::*;
use std::os::fd::OwnedFd;

use crate::authorship::{Author, harness_report};

fn author() -> Author {
    Author::new(
        &weaver_types::SessionId("s-1".into()),
        &weaver_types::RunId("r-1".into()),
    )
}

fn full_device() -> OwnedFd {
    OwnedFd::from(
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("the full device"),
    )
}

fn scratch_sink(tag: &str) -> OwnedFd {
    let path = std::env::temp_dir().join(format!(
        "weaver-record-{tag}-{}-{:?}.ndjson",
        std::process::id(),
        std::thread::current().id()
    ));
    let file = std::fs::File::create(&path).expect("sink");
    let _ = std::fs::remove_file(&path);
    OwnedFd::from(file)
}

fn fault(author: &Author, record: &mut Record) -> Result<Sequence, RecordFailure> {
    author.author_fault(
        record,
        Subsystem::Harness,
        None,
        &harness_report(weaver_types::FaultCase::OrganDeathObserved, "{}"),
    )
}

/// **A writer that fails between two submissions is latched at the next**
/// (#116, review focus 1): the first submission is queued and answered
/// `Ok`, the write fails at the device, and the drain and every later
/// submission answer `Err`, each setting the latch. A fresh record answers
/// unlatched. Perturbation: never set the latch, and this fails.
#[test]
fn a_writer_that_fails_is_latched_at_the_next_authoring() {
    let author = author();
    let fresh = Record::from(
        weaver_trace::Recorder::receive(
            scratch_sink("fresh"),
            weaver_trace::RunRef("r-1".into()),
            weaver_trace::SessionRef("s-1".into()),
        )
        .expect("recorder"),
    );
    assert!(!fresh.failed(), "a fresh record is unlatched");
    let mut record = Record::from(
        weaver_trace::Recorder::receive(
            full_device(),
            weaver_trace::RunRef("r-1".into()),
            weaver_trace::SessionRef("s-1".into()),
        )
        .expect("recorder"),
    );
    let _ = fault(&author, &mut record);
    assert!(record.drain().is_err(), "the writer has failed");
    assert!(record.failed(), "the drain's failure is latched");
    let mut again = Record::from(
        weaver_trace::Recorder::receive(
            full_device(),
            weaver_trace::RunRef("r-1".into()),
            weaver_trace::SessionRef("s-1".into()),
        )
        .expect("recorder"),
    );
    let first = fault(&author, &mut again);
    assert!(first.is_ok(), "the first submission is queued: {first:?}");
    let _ = again.serving_mut().expect("serving").drain();
    assert!(
        !again.failed(),
        "a drain past the record's own road is unseen"
    );
    assert!(fault(&author, &mut again).is_err(), "the failure stands");
    assert!(again.failed(), "and the next authoring latches it");
}

/// **A submission refused on its own is latched, the writer sound** (#116,
/// Q5): an event outside the diagnostic vocabulary is refused at the
/// conversion, the recorder takes the next event, and the record is
/// latched all the same, since it is missing the refused one.
/// Perturbation: latch only a writer's failure, and this fails.
#[test]
fn a_refusal_on_submit_is_latched_with_the_writer_sound() {
    let author = author();
    let mut record = Record::from(
        weaver_diagnostic::Recorder::receive(
            scratch_sink("refused"),
            weaver_diagnostic::RunRef("r-1".into()),
            weaver_diagnostic::SessionRef("s-1".into()),
        )
        .expect("recorder"),
    );
    let refused = author.author(
        &mut record,
        Kind::ClassifyRequest,
        Subsystem::Harness,
        None,
        Some(weaver_trace::Payload::ClassifyRequest(
            weaver_trace::ClassifyAsk {
                content: "x".into(),
            },
        )),
    );
    assert!(
        matches!(refused, Err(RecordFailure::OutsideDiagnosticVocabulary)),
        "{refused:?}"
    );
    assert!(fault(&author, &mut record).is_ok(), "the writer is sound");
    assert!(record.failed(), "the refused event is latched");
}

/// **The diagnostic arm's failed write is latched** (#116, Q4): its writer
/// is synchronous and its drain vacuous, so the latch is the one place a
/// failure on a diagnostic run is seen after the submission that met it.
/// Perturbation: never set the latch, and this fails.
#[test]
fn a_diagnostic_write_that_fails_is_latched_though_the_drain_is_vacuous() {
    let author = author();
    let mut record = Record::from(
        weaver_diagnostic::Recorder::receive(
            full_device(),
            weaver_diagnostic::RunRef("r-1".into()),
            weaver_diagnostic::SessionRef("s-1".into()),
        )
        .expect("recorder"),
    );
    assert!(fault(&author, &mut record).is_err(), "the write fails");
    assert!(record.drain().is_ok(), "the drain is vacuous");
    assert!(record.failed(), "the failure is latched");
}
