//! The queue-only result names the allocation it actually performed.
use super::*;

#[test]
fn author_queue_review_returns_actual_ticket_before_recorded_and_on_prepare_failure() {
    for failure in [None, Some(false), Some(true)] {
        let (recorder, _reader) = recorder();
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget(1);
        let original = authored("Read exact queued original.");
        let queued = barrier.queue_authored_with_clock(
            original.clone(),
            RecordPurpose::Content,
            &author(),
            1,
            &mut inbox,
            &mut budget,
            &mut || match failure {
                None => Ok(1_001),
                Some(false) => Ok(999),
                Some(true) => Err(std::io::Error::other(
                    "actual queue preparation clock failure",
                )),
            },
        );
        let ticket = queued
            .ticket
            .expect("the original was actually allocated before preparation");
        assert_eq!(barrier.pending_author().unwrap().ticket, ticket);
        same_original(&barrier.pending_author().unwrap().event, &original);
        assert!(barrier.admitted_before_failure().is_none());
        assert_eq!(barrier.recorder().structure().len(), 1);
        if failure.is_some() {
            assert!(matches!(queued.progress, RecordProgress::Failed));
            assert!(barrier.pending().is_none());
            assert!(budget.is_frozen());
        } else {
            assert!(matches!(queued.progress, RecordProgress::Pending));
            assert!(barrier.pending().is_some());
            let progress = barrier.resume_with_clock(&mut inbox, &mut budget, &mut || Ok(1_002));
            let RecordProgress::Recorded { sequence, .. } = progress else {
                panic!("queued original did not record")
            };
            assert_eq!(barrier.recorded_author_ticket(sequence), Some(ticket));
            assert!(barrier.pending_author().is_none());
            assert!(barrier.failure().is_none());
        }
    }
}

#[test]
fn author_queue_review_busy_returns_original_without_claiming_an_older_ticket() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let first = authored("Read first original.");
    let queued = barrier.queue_authored_with_clock(
        first.clone(),
        RecordPurpose::Content,
        &author(),
        1,
        &mut inbox,
        &mut budget,
        &mut || Ok(1_001),
    );
    assert!(matches!(queued.progress, RecordProgress::Pending));
    let first_ticket = queued.ticket.unwrap();
    let second = authored("Read refused second original.");
    let queued = barrier.queue_authored_with_clock(
        second.clone(),
        RecordPurpose::Content,
        &author(),
        1,
        &mut inbox,
        &mut budget,
        &mut || panic!("Busy cannot allocate or prepare"),
    );
    assert_eq!(
        queued.ticket, None,
        "an unadmitted input cannot claim the older allocation"
    );
    let RecordProgress::BusyAuthored { event, .. } = queued.progress else {
        panic!("second original must return")
    };
    same_original(&event, &second);
    assert_eq!(barrier.pending_author().unwrap().ticket, first_ticket);
    same_original(&barrier.pending_author().unwrap().event, &first);
    assert_eq!(barrier.recorder().structure().len(), 1);
}
