use super::*;
use std::io::{Error, ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

fn event() -> Event {
    Event {
        envelope: envelope(Kind::MessageUser, Some("t-1"), 17, 19),
        payload: Some(Payload::Spliced(
            RawValue::from_string(serde_json::json!({"text":"payload".repeat(80_000)}).to_string())
                .unwrap(),
        )),
    }
}

#[test]
fn managed_submission_preserves_one_render_and_sequence_through_partial_writes() {
    let (mut expected_recorder, mut expected_file) = stand("s", "r");
    expected_recorder.submit(event()).unwrap();
    let expected = contents(&mut expected_file).into_bytes();
    let (mut sink, mut reader) = UnixStream::pair().unwrap();
    sink.set_nonblocking(true).unwrap();
    reader.set_nonblocking(true).unwrap();
    let descriptor = sink.as_raw_fd();
    let mut filler = 0;
    loop {
        match sink.write(&[b'x'; 4096]) {
            Ok(n) => {
                assert!(n > 0);
                filler += n;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            other => panic!("fill the real stream: {other:?}"),
        }
    }
    let mut recorder =
        Recorder::receive(sink.into(), RunRef("r".into()), SessionRef("s".into())).unwrap();
    let total = filler + expected.len();
    // The reader progresses independently of the wait hook. Removing that
    // hook cannot disable the fixture's drain or leave this test hung.
    let drain = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        let until = Instant::now() + Duration::from_secs(5);
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        while bytes.len() < total {
            assert!(
                Instant::now() < until,
                "the admitted rendering never completed"
            );
            match reader.read(&mut chunk) {
                Ok(0) => panic!("the sink closed before its admitted line"),
                Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => (),
                other => panic!("read the actual stream: {other:?}"),
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        bytes
    });
    let mut checkpoints = 0;
    let sequence = recorder
        .submit_managed(event(), &mut |fd| {
            assert_eq!(fd.as_raw_fd(), descriptor);
            checkpoints += 1;
            std::thread::sleep(Duration::from_millis(1));
            Ok(())
        })
        .unwrap();
    assert_eq!(sequence, Sequence(0));
    assert!(
        checkpoints > 2,
        "write pressure must revisit the caller's management wait"
    );
    let bytes = drain.join().unwrap();
    assert_eq!(&bytes[..filler], vec![b'x'; filler]);
    assert_eq!(&bytes[filler..], expected);
}

#[test]
fn managed_admission_refusal_never_waits_or_consumes_order() {
    let (mut recorder, mut file) = stand("s", "r");
    let refused = recorder.submit_managed(
        Event {
            envelope: envelope(Kind::ModelOutput, Some("t"), 1, 1),
            payload: None,
        },
        &mut |_| panic!("an unadmitted event cannot touch the sink"),
    );
    assert!(matches!(
        refused,
        Err(weaver_diagnostic::Failure::SubmitRefused { .. })
    ));
    assert!(contents(&mut file).is_empty());
    assert_eq!(
        recorder.submit_managed(event(), &mut |_| Ok(())).unwrap(),
        Sequence(0)
    );
}

#[test]
fn managed_wait_failure_latches_before_any_later_event_or_wait() {
    let (mut recorder, mut file) = stand("s", "r");
    let failure = recorder
        .submit_managed(event(), &mut |_| {
            Err(Error::new(
                ErrorKind::TimedOut,
                "original cancellation interval expired",
            ))
        })
        .unwrap_err();
    assert!(matches!(
        failure,
        weaver_diagnostic::Failure::WriteFailed { .. }
    ));
    assert!(contents(&mut file).is_empty());
    let later = recorder.submit_managed(event(), &mut |_| panic!("first failure must latch"));
    assert_eq!(later.unwrap_err(), failure);
    assert!(contents(&mut file).is_empty());
}
