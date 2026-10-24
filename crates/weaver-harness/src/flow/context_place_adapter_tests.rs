//! Read-only sort measurement and retained refusal-response adapters.
use super::*;

#[test]
fn place_readonly_measurement_reuses_real_prefix_and_units_without_reserving_occurrences() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("sort-preview");
    let original = fx.decision();
    let before = fx.journal();
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    let schemas = fx.schemas();
    let assembly = prefix.assembly_id();
    let revision = prefix.revision();
    let run = &mut fx.summary.boot.member.run;
    run.context
        .begin_place_measurement(
            &prefix,
            &units,
            &schemas,
            PumpDeadline::Decision,
            &mut fx.decode,
            &mut run.clock,
        )
        .unwrap();
    let mut packet = None;
    for _ in 0..128 {
        if let Some(raw) = receive(fx.peer.as_fd()) {
            packet = Some(serde_json::from_slice::<TokenDirective>(&raw).unwrap());
            break;
        }
        let run = &mut fx.summary.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        run.context
            .advance_measurement(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io,
            })
            .unwrap();
    }
    let Some(TokenDirective::MeasureDelta {
        keep,
        delta,
        tool_schemas,
    }) = packet
    else {
        panic!("sort actual measurement missing");
    };
    assert_eq!(delta, units[0].messages());
    assert_eq!(tool_schemas, schemas);
    let raw = fx.answer(&fx.measured(keep, 3));
    let mut measurement = None;
    for _ in 0..128 {
        let run = &mut fx.summary.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        run.context
            .advance_measurement(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io,
            })
            .unwrap();
        measurement = run
            .context
            .take_place_measurement(&mut fx.decode, &mut run.clock)
            .unwrap();
        if measurement.is_some() {
            break;
        }
    }
    let Some(MeasureCompletion::Measured(measured)) = measurement else {
        panic!("read-only measure refused");
    };
    assert_eq!(measured.request().assembly_id, assembly);
    assert_eq!(measured.request().revision, revision);
    assert_eq!(measured.bound(), original);
    assert_eq!(measured.raw_bytes(), raw);
    assert_eq!(fx.journal(), before);
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert!(receive(fx.peer.as_fd()).is_none());
    let same = fx.prefix(None, s::CutMethod::Flush);
    assert_eq!(same.assembly_id(), assembly);
    assert_eq!(same.revision(), revision);
    // The same actual units remain consumable by the one real owner afterward.
    let grant = fx.grant(units, 3);
    assert_eq!(grant.assembly_id(), assembly);
}

impl PlaceFixture {
    pub(super) fn finish_notice(&mut self) -> PlaceRefused {
        for _ in 0..128 {
            let run = self.run();
            run.context
                .advance_place_refusal(&mut ports(&mut run.clock))
                .unwrap();
            if let Some(refusal) = run.context.take_place_refusal(&mut run.clock).unwrap() {
                return refusal;
            }
        }
        panic!("placement address refusal did not record its bounded notice");
    }
}

#[test]
fn place_invalid_cut_records_exact_refusal_then_holds_matching_response_before_new_work() {
    for active_turn in [false, true] {
        let mut fx = PlaceFixture::new();
        if active_turn {
            fx.turn("invalid-cut");
        }
        let boot = fx
            .records(Kind::StateMarkPosition)
            .into_iter()
            .next()
            .unwrap();
        let mut cut = s::CutReference {
            span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
            occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
        };
        cut.occurrence_id.ordinal = 1000.into();
        let before = fx.journal();
        let run = fx.run();
        assert_eq!(
            run.context
                .prepare_prefix(Some(cut), s::CutMethod::Flush, &mut run.clock)
                .unwrap_err(),
            PlaceError::InvalidPrefix
        );
        assert!(run.context.failure().is_none());
        let refused = fx.finish_notice();
        assert_eq!(refused.code, s::PlacementCode::InvalidCut);
        assert_eq!(fx.records(Kind::Refusal).len(), 1);
        assert_eq!(fx.records(Kind::TurnClosed).len(), usize::from(active_turn));
        let run = fx.run();
        let frame = run.context.place_close().expect("owned response");
        let value: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
        assert_eq!(
            value["kind"],
            if active_turn { "stopped" } else { "refused" }
        );
        if active_turn {
            assert_eq!(value["turn"], "invalid-cut");
        } else {
            assert!(value.get("turn").is_none());
        }
        assert_eq!(
            run.context
                .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                .unwrap_err(),
            PlaceError::Busy(None)
        );
        assert!(receive(fx.peer.as_fd()).is_none());
        assert_eq!(fx.journal(), before);
        let run = fx.run();
        run.context.place_response_sent(&mut run.clock).unwrap();
        assert!(
            run.context
                .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                .is_ok()
        );
    }
}
