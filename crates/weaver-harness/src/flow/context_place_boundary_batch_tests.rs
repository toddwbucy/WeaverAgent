//! Real placement evidence combined with independently invalid prerequisites.
use super::*;
use crate::flow::context::held_input_tests::{HOLDS, hold};

#[test]
fn boundary_batch_place_refused_unheld_release_revokes_generation() {
    for held in [
        None,
        Some(HeldInput::Caller),
        Some(HeldInput::Template),
        Some(HeldInput::Both),
    ] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("release-bound");
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        fx.sent_generation();
        let run = fx.run();
        let existing = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT).unwrap();
        run.context
            .arm(PumpDeadline::Leave, existing, &mut run.clock)
            .unwrap();
        let changed = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT + 1).unwrap();
        let originals = held.map(|held| hold(&mut run.context, held));
        assert_eq!(
            run.context.begin_place_release(changed, &mut run.clock),
            Err(RunFailure::PlaceOwned)
        );
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Leave),
            Some(existing)
        );
        assert_eq!(run.context.phase(), RecordingPhase::Entered);
        if let Some(originals) = originals {
            originals.assert_held(&run.context);
            originals.take(&mut run.context);
        }
        let mut packet = None;
        for _ in 0..8 {
            fx.advance_generation().unwrap();
            if let Some(raw) = receive(fx.peer.as_fd()) {
                packet = Some(serde_json::from_slice::<TokenDirective>(&raw).unwrap());
                break;
            }
        }
        assert_eq!(
            packet,
            held.is_none().then(|| TokenDirective::Cancel {
                turn: TurnKey("release-bound".into()),
            })
        );
    }
}

#[test]
fn boundary_batch_prefix_rejects_invalid_cut_before_custody_without_burning_ids() {
    for held in HOLDS {
        let mut fx = PlaceFixture::new();
        let boot = fx.records(Kind::StateMarkPosition).remove(0);
        let cut = s::CutReference {
            span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
            occurrence_id: {
                let mut id: s::MarkOccurrenceId =
                    serde_json::from_value(boot["payload"]["id"].clone()).unwrap();
                id.ordinal = 1000.into();
                id
            },
        };
        let before = fx.journal();
        let run = fx.run();
        let assembly = run.context.boot_journal.as_ref().unwrap().next_assembly();
        let originals = hold(&mut run.context, held);
        assert_eq!(
            run.context
                .prepare_prefix(Some(cut), s::CutMethod::Flush, &mut run.clock)
                .unwrap_err(),
            PlaceError::InvalidPrefix
        );
        assert_eq!(
            run.context.boot_journal.as_ref().unwrap().next_assembly(),
            assembly
        );
        originals.assert_held(&run.context);
        assert_eq!(fx.journal(), before);
    }
}

#[test]
fn boundary_batch_measurement_validates_prefix_schema_and_units_before_custody() {
    for held in HOLDS {
        for defect in ["prefix", "schema", "units"] {
            let mut foreign = PlaceFixture::new();
            let mut fx = PlaceFixture::new();
            let mut units = fx.text(8, "record");
            fx.turn("measure-prerequisite");
            let prefix = if defect == "prefix" {
                foreign.prefix(None, s::CutMethod::Flush)
            } else {
                fx.prefix(None, s::CutMethod::Flush)
            };
            let mut schemas = fx.schemas();
            if defect == "schema" {
                schemas.push("foreign schema".into());
            }
            if defect == "units" {
                units = foreign.text(8, "record");
            }
            let before = fx.journal();
            let run = &mut fx.summary.boot.member.run;
            let originals = hold(&mut run.context, held);
            assert_eq!(
                run.context.begin_place_measurement(
                    &prefix,
                    &units,
                    &schemas,
                    PumpDeadline::Decision,
                    &mut fx.decode,
                    &mut run.clock
                ),
                Err(if defect == "prefix" {
                    PlaceError::Stale
                } else {
                    PlaceError::InvalidUnits
                })
            );
            assert!(run.context.measure.is_none());
            originals.assert_held(&run.context);
            assert!(!fx.decode.send_pending());
            assert_eq!(fx.journal(), before);
        }
    }
}

#[test]
fn boundary_batch_place_request_keeps_genuine_refusals_and_exact_selection() {
    for held in HOLDS {
        for defect in ["proof", "wait", "context", "prefix", "units"] {
            let mut foreign = PlaceFixture::new();
            let mut fx = PlaceFixture::new();
            let units = fx.text(8, "record");
            fx.turn("place-prerequisite");
            let mut request = fx.request(units);
            let expected = match defect {
                "proof" => {
                    fx.run().context.place_engine.take();
                    PlaceError::WrongProof
                }
                "wait" => {
                    fx.run().context.complete_wait(PumpDeadline::Decision);
                    PlaceError::MissingWait
                }
                "context" => {
                    request.context.label.clear();
                    PlaceError::InvalidContext
                }
                "prefix" => {
                    request.prefix = foreign.prefix(None, s::CutMethod::Flush);
                    PlaceError::InvalidPrefix
                }
                "units" => {
                    request.selection = foreign.text(8, "record");
                    PlaceError::InvalidUnits
                }
                _ => unreachable!(),
            };
            let selected = request.selection.as_ptr();
            let assembly = request.prefix.assembly_id();
            let before = fx.journal();
            let run = &mut fx.summary.boot.member.run;
            let next = run.context.boot_journal.as_ref().unwrap().next_assembly();
            let originals = hold(&mut run.context, held);
            let error = run
                .context
                .begin_place(request, &mut fx.decode, &mut run.clock)
                .unwrap_err();
            assert_eq!(error.refusal, expected, "{defect}");
            assert!(error.failure.is_none());
            assert_eq!(error.input.selection.as_ptr(), selected);
            assert_eq!(error.input.prefix.assembly_id(), assembly);
            assert_eq!(
                run.context.boot_journal.as_ref().unwrap().next_assembly(),
                next
            );
            assert!(run.context.place.is_none());
            assert!(run.context.measure.is_none());
            originals.assert_held(&run.context);
            assert!(!fx.decode.send_pending());
            assert_eq!(fx.journal(), before);
        }
    }
}
