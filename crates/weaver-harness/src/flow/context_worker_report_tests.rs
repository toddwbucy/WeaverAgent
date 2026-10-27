use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::input_tests::InputFixture;
use crate::flow::context::output_tests::{OutputFixture, original, text};
use crate::flow::pump::PumpDeadline;
use crate::flow::pump_transport::{OrganPort, ReceiveProgress};
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::AsRawFd;

fn received() -> (OrganPort, crate::channel::ChildEnd, ReceivedOrgan) {
    let (channel, peer) = crate::channel::OrganChannel::pair().unwrap();
    let mut port = OrganPort::from_channel(channel).unwrap();
    let report = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Gate,
            ordinal: 102,
        },
        position: Position::Close,
        payload: Payload::Fault(weaver_types::FaultReport {
            case: weaver_types::FaultCase::ListenerLost,
            account: serde_json::value::RawValue::from_string("{ \"original\": [1,  2] }".into())
                .unwrap(),
        }),
    };
    let bytes = serde_json::to_vec(&report).unwrap();
    assert_eq!(
        send(peer.as_fd().as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    let received = port.take_received().unwrap().unwrap();
    (port, peer, received)
}

fn ports<'a, I: PumpIo>(gate: &'a mut OrganPort, io: &'a mut I) -> PumpPorts<'a, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: Some(gate),
        decode: None,
        label: None,
        io,
    }
}

#[test]
fn received_report_yields_to_the_original_output_author() {
    let mut fx = OutputFixture::new();
    let content = vec![text("Read original.")];
    fx.begin(original(content.clone()));
    let (mut gate, _peer, received) = received();
    let bytes = received.raw_bytes().to_vec();
    let mut report = Report::new(&fx.input.run.context, received, Subsystem::Gate);
    let run = &mut fx.input.run;
    let original = format!("{:?}", run.context.waiting_content.as_ref().unwrap().event);
    assert!(
        !report
            .advance(&mut run.context, &mut ports(&mut gate, &mut run.clock))
            .unwrap()
    );
    assert_eq!(
        format!("{:?}", run.context.waiting_content.as_ref().unwrap().event),
        original
    );
    assert!(!report.started);
    assert!(report.author.ticket().is_none());
    assert_eq!(report.received.raw_bytes(), bytes);
    fx.complete(None, false);
    let run = &mut fx.input.run;
    assert!(
        report
            .advance(&mut run.context, &mut ports(&mut gate, &mut run.clock))
            .unwrap()
    );
    let sequence = report.sequence.unwrap();
    let checked = run.context.take_output(&mut run.clock).unwrap().unwrap();
    assert_eq!(checked.original().message.content, content);
    assert_ne!(checked.source().block_id.sequence.0, sequence.0);
    assert!(
        report
            .advance(&mut run.context, &mut ports(&mut gate, &mut run.clock))
            .unwrap()
    );
    assert_eq!(fx.input.records(serving::Kind::MessageAssistant).len(), 1);
    assert_eq!(fx.input.records(serving::Kind::Fault).len(), 1);
    assert_eq!(report.received.raw_bytes(), bytes);
}

#[test]
fn received_report_retains_its_exact_claim_through_trace_pressure() {
    let mut fx = InputFixture::new();
    let (mut gate, _peer, received) = received();
    let bytes = received.raw_bytes().to_vec();
    let mut report = Report::new(&fx.run.context, received, Subsystem::Gate);
    fx.fill_trace();
    let run = &mut fx.run;
    run.context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(104, 1).unwrap(),
            &mut run.clock,
        )
        .unwrap();
    assert!(matches!(
        report.advance(&mut run.context, &mut ports(&mut gate, &mut run.clock)),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    assert!(report.started);
    let ticket = report.author.ticket().unwrap();
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    assert!(report.sequence.is_none());
    assert_eq!(report.received.raw_bytes(), bytes);
    run.context.complete_wait(PumpDeadline::Label).unwrap();
    fx.drain_trace();
    let run = &mut fx.run;
    assert!(
        report
            .advance(&mut run.context, &mut ports(&mut gate, &mut run.clock))
            .unwrap()
    );
    assert_eq!(report.author.ticket(), Some(ticket));
    assert_eq!(report.sequence, report.author.sequence());
    assert!(run.context.pending_author().is_none());
    assert!(
        report
            .advance(&mut run.context, &mut ports(&mut gate, &mut run.clock))
            .unwrap()
    );
    let rows = fx.records(serving::Kind::Fault);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["subsystem"], "gate");
    assert_eq!(rows[0]["sequence"], report.sequence.unwrap().0.to_string());
    assert_eq!(report.received.raw_bytes(), bytes);
}

#[test]
fn received_report_cannot_use_another_ports_custody() {
    let mut fx = InputFixture::new();
    let (mut gate, _peer, received) = received();
    let mut report = Report::new(&fx.run.context, received, Subsystem::Gate);
    let (other, _other_peer) = crate::channel::OrganChannel::pair().unwrap();
    let mut other = OrganPort::from_channel(other).unwrap();
    let run = &mut fx.run;
    assert_eq!(
        report.advance(&mut run.context, &mut ports(&mut other, &mut run.clock)),
        Err(RunFailure::Closed)
    );
    assert!(!report.started);
    assert!(report.event.is_some());
    assert!(report.author.ticket().is_none());
    assert!(report.author.sequence().is_none());
    assert!(
        report
            .advance(&mut run.context, &mut ports(&mut gate, &mut run.clock))
            .unwrap()
    );
    assert_eq!(fx.records(serving::Kind::Fault).len(), 1);
}
