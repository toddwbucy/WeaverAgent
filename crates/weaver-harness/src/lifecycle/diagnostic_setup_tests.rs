use super::*;
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;

#[test]
fn diagnostic_setup_unsent_admit_under_pressure_hears_rollback_without_sending() {
    for canonical in [false, true] {
        let root = crate::scratch::dir(format!(
            "diagnostic-unsent-{}-{canonical}",
            std::process::id()
        ));
        let listener = crate::channel::bind_coordination(&root.join("admin.sock")).unwrap();
        let (original, far) = OrganChannel::pair().unwrap();
        let original = if canonical {
            OrganChannel::diagnostic_from_owned(original.into_fd())
        } else {
            original
        };
        let far = far.into_channel();
        let exchange = ExchangeId {
            opener: Opener::Admin,
            ordinal: 4,
        };
        let mut setup = Setup::new(&listener, &original, exchange.clone()).unwrap();
        let (spu, receiver) = OrganChannel::pair().unwrap();
        let receiver = receiver.into_channel();
        let pressure = vec![b'x'; 16_384];
        let mut count = 0;
        loop {
            match send(
                spu.as_fd().as_raw_fd(),
                &pressure,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(n) => {
                    assert_eq!(n, pressure.len());
                    count += 1;
                }
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill actual Admit transport: {other:?}"),
            }
        }
        assert!(count > 0);
        far.send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 5,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 0 },
                forced: true,
                rollback: true,
            }),
        })
        .unwrap();
        let source = weaver_types::state_flow::enter::EnterEnvelope::decode(include_bytes!(
            "../../../weaver-types/tests/fixtures/state-enter.json"
        ))
        .unwrap();
        assert_eq!(
            setup.admit(&spu, 1, source.enter().spu_instruction.clone()),
            Err(LifecycleRefusal::NoResidency)
        );
        assert!(matches!(setup.admission, Admission::Unsent));
        assert!(setup.leaving.is_some());
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        for _ in 0..count {
            let n = recv(
                receiver.as_fd().as_raw_fd(),
                &mut bytes,
                MsgFlags::MSG_DONTWAIT,
            )
            .unwrap();
            assert_eq!(&bytes[..n], pressure.as_slice());
        }
        assert_eq!(
            recv(
                receiver.as_fd().as_raw_fd(),
                &mut bytes,
                MsgFlags::MSG_DONTWAIT
            ),
            Err(nix::errno::Errno::EAGAIN)
        );
        setup.poll(None, None, Some(Duration::ZERO)).unwrap();
        let n = recv(far.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
        let refusal: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(refusal.exchange, exchange);
        assert_eq!(
            refusal.payload,
            weaver_types::Payload::Refusal(LifecycleRefusal::NoResidency)
        );
        assert!(!setup.ready);
    }
}
