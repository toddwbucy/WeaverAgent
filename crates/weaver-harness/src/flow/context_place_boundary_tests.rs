//! Origin, recorder custody and lifecycle first-byte boundaries.
use super::*;

struct PlacementPressureClock<'a> {
    clock: &'a mut FakePumpIo,
    fd: RawFd,
}
impl PumpIo for PlacementPressureClock<'_> {
    fn now(&mut self) -> io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        // FakePumpIo leaves a wake at the deadline queued. Do not invent that
        // stale wake: the deadline takes priority over readiness at the same time.
        if deadline.is_some_and(|end| self.clock.now().unwrap() + 1 >= end) {
            return self.clock.wait(interests, deadline);
        }
        ReadyClock {
            clock: self.clock,
            fd: self.fd,
        }
        .wait(interests, deadline)
    }
}

impl PlaceFixture {
    pub(super) fn advance_placement_pressure(&mut self) -> Result<PlaceProgress, RunFailure> {
        // The nested author waits until a record or an existing recoverable
        // deadline. Give this fixture that return boundary while the sink is held.
        let run = self.run();
        let pulse = AbsoluteBound::after(run.clock.now().unwrap(), 2).unwrap();
        run.context
            .arm(PumpDeadline::Label, pulse, &mut run.clock)
            .unwrap();
        let run = &mut self.summary.boot.member.run;
        let mut clock = PlacementPressureClock {
            clock: &mut run.clock,
            fd: self.decode.fd().as_raw_fd(),
        };
        let result = run.context.advance_place(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut self.decode),
            label: None,
            io: &mut clock,
        });
        assert_eq!(
            self.run().context.complete_wait(PumpDeadline::Label),
            Some(pulse)
        );
        match result {
            Err(RunFailure::Wait {
                key: PumpDeadline::Label,
                bound,
            }) => {
                assert_eq!(bound, pulse);
                Ok(PlaceProgress::Pending)
            }
            other => other,
        }
    }

    fn pending_placement(&mut self) -> u64 {
        let units = self.text(8, "record");
        self.turn("pending-placement");
        let request = self.request(units);
        self.begin(request);
        let TokenDirective::MeasureDelta { keep, .. } = self.next_measure() else {
            unreachable!()
        };
        self.answer(&self.measured(keep, 3));
        self.summary.boot.member.fill_trace();
        for _ in 0..128 {
            self.advance_placement_pressure().unwrap();
            if let Some(author) = self.run().context.pending_author() {
                assert_eq!(author.event.kind, Kind::StatePlacement);
                return author.ticket;
            }
        }
        panic!("trace pressure did not hold the actual append placement author");
    }
}

#[test]
fn place_install_requires_the_actual_boot_owner_even_when_run_and_read_ids_match() {
    let mut first = SummaryFixture::new(vec![]);
    let foreign = first.boot(None);
    let mut second = SummaryFixture::new(vec![]);
    let own = second.boot(None);
    assert_eq!(
        foreign.read().recall_sequence(),
        own.read().recall_sequence()
    );
    assert_eq!(foreign.read().raw_bytes(), own.read().raw_bytes());
    let run = second.run();
    let error = run
        .context
        .install_place(foreign, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::WrongProof);
    assert!(error.failure.is_none());
    assert!(run.context.loaded_place_schemas().is_none());
    run.context.install_place(own, &mut run.clock).unwrap();
    let run = first.run();
    run.context
        .install_place(error.prerequisite, &mut run.clock)
        .unwrap();
    assert!(
        run.context.loaded_place_schemas().is_some(),
        "returned proof remains usable only by its actual context"
    );
}

#[test]
fn place_foreign_material_and_projection_return_owned_inputs_without_side_effects() {
    for foreign_prefix in [false, true] {
        let mut first = PlaceFixture::new();
        let foreign = first.text(8, "record");
        first.turn("same-turn");
        let first_request = first.request(foreign);
        let mut second = PlaceFixture::new();
        let local = second.text(8, "record");
        second.turn("same-turn");
        let local_request = second.request(local);
        assert_eq!(
            first_request.selection[0].messages(),
            local_request.selection[0].messages()
        );
        let request = if foreign_prefix {
            PlaceRequest {
                prefix: first_request.prefix,
                selection: local_request.selection,
                tool_schemas: local_request.tool_schemas,
                context: local_request.context,
            }
        } else {
            PlaceRequest {
                prefix: local_request.prefix,
                selection: first_request.selection,
                tool_schemas: local_request.tool_schemas,
                context: local_request.context,
            }
        };
        let sequence = request.selection[0].reads()[0].recall_sequence();
        let run = &mut second.summary.boot.member.run;
        let error = run
            .context
            .begin_place(request, &mut second.decode, &mut run.clock)
            .unwrap_err();
        assert_eq!(
            error.refusal,
            if foreign_prefix {
                PlaceError::InvalidPrefix
            } else {
                PlaceError::InvalidUnits
            }
        );
        assert_eq!(
            error.input.selection[0].reads()[0].recall_sequence(),
            sequence
        );
        assert!(error.failure.is_none());
        assert!(run.context.failure().is_none());
        assert!(receive(second.peer.as_fd()).is_none());
        assert_eq!(second.records(Kind::StatePlacement).len(), 1);
        assert_eq!(second.journal().6, 0);
    }
}

#[test]
fn place_stale_prepared_prefix_cannot_replace_a_completed_generation() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    let later = fx.text(9, "later");
    fx.turn("stale-prefix");
    let stale = fx.request(later);
    fx.generation(units, 3, 1, "Read result.", Finish::Completed);
    let before = fx.journal();
    fx.decision();
    let run = &mut fx.summary.boot.member.run;
    let error = run
        .context
        .begin_place(stale, &mut fx.decode, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::InvalidPrefix);
    assert_eq!(error.input.selection.len(), 1);
    assert!(error.failure.is_none());
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, before.6);
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn place_real_recorder_pressure_keeps_exact_ticket_bound_candidate_and_no_append() {
    let mut fx = PlaceFixture::new();
    let ticket = fx.pending_placement();
    let bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let before = fx.journal();
    for _ in 0..8 {
        assert_ne!(
            fx.advance_placement_pressure().unwrap(),
            PlaceProgress::Complete
        );
        let run = fx.run();
        assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
        assert!(
            run.context
                .take_placement(&mut run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(run.context.complete_wait(PumpDeadline::Decision).is_none());
        assert!(receive(fx.peer.as_fd()).is_none());
        assert_eq!(fx.journal().6, before.6);
    }
    fx.summary.drain();
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("resumed placement refused");
    };
    assert_eq!(fx.records(Kind::StatePlacement).len(), 2);
    assert!(fx.run().context.pending_author().is_none());
    assert!(fx.run().context.completed_author.is_none());
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        before.1,
        3,
        0,
        "",
        Finish::Completed,
        before.2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    let units = fx.text(9, "next");
    fx.generation(units, 2, 0, "", Finish::Completed);
}

#[test]
fn place_valid_replacement_ticket_never_binds_a_foreign_sequence() {
    let mut fx = PlaceFixture::new();
    let ticket = fx.pending_placement();
    let run = fx.run();
    let event = run.context.pending_author().unwrap().event.clone();
    assert!(run.context.barrier.abandon_boot_author(ticket));
    assert!(matches!(
        run.context.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Observation,
            &run.context.author,
            1,
            &mut run.context.inbox,
            &mut run.context.budget,
            &mut || run.clock.now()
        ),
        RecordProgress::Pending
    ));
    let foreign = run.context.pending_author().unwrap().ticket;
    assert_ne!(foreign, ticket);
    let before = run.clock.now().unwrap();
    let bound = run.context.deadlines.get(PumpDeadline::Decision).unwrap();
    assert!(before < bound.deadline_ns());
    assert!(run.context.failure().is_none());
    let failure = fx.advance().unwrap_err();
    assert_eq!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::Barrier,
            detail: "placement author ticket was replaced".into(),
        }
    );
    assert_eq!(fx.run().clock.now().unwrap(), before);
    assert!(fx.run().clock.now().unwrap() < bound.deadline_ns());
    assert_eq!(fx.run().context.failure(), Some(&failure));
    assert_eq!(fx.run().context.pending_author().unwrap().ticket, foreign);
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert_eq!(fx.journal().6, 0);
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.summary.drain();
}

#[test]
fn place_recorded_then_recoverable_wait_retains_sequence_and_releases_next_owner() {
    let mut witnessed = false;
    for boundary in 1..=32 {
        let mut fx = PlaceFixture::new();
        fx.pending_placement();
        fx.summary.drain();
        let run = &mut fx.summary.boot.member.run;
        let before = run.clock.now().unwrap();
        let bound = AbsoluteBound::after(before, 10).unwrap();
        run.context
            .arm(PumpDeadline::Label, bound, &mut run.clock)
            .unwrap();
        let mut clock = SummaryBoundaryClock {
            before,
            after: bound.deadline_ns(),
            boundary,
            samples: 0,
            broken: false,
        };
        let result = run.context.advance_place(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut clock,
        });
        if result.is_ok() || fx.records(Kind::StatePlacement).len() != 2 {
            continue;
        }
        assert_eq!(
            result.unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Label,
                bound
            }
        );
        witnessed = true;
        let sequence = fx.records(Kind::StatePlacement).pop().unwrap()["sequence"].clone();
        let run = fx.run();
        assert!(
            run.context.take_placement(&mut clock).is_err(),
            "expired wait cannot hand out a grant"
        );
        assert_eq!(run.context.complete_wait(PumpDeadline::Label), Some(bound));
        run.clock.advance_to(bound.deadline_ns()).unwrap();
        let PlacementCompletion::Granted(grant) = fx.take() else {
            panic!("Recorded placement was lost");
        };
        assert_eq!(sequence, grant.placement_sequence().0.to_string());
        assert_eq!(fx.records(Kind::StatePlacement).len(), 2);
        assert!(fx.run().context.completed_author.is_none());
        let journal = fx.journal();
        fx.start_generation(grant);
        fx.sent_generation();
        fx.answer(&TokenAnswer::Generated(generated(
            journal.1,
            3,
            0,
            "",
            Finish::Completed,
            journal.2,
        )));
        assert!(matches!(
            fx.finish_generation(),
            GenerationCompletion::Generated(_)
        ));
        let units = fx.text(9, "next");
        fx.generation(units, 2, 0, "", Finish::Completed);
        break;
    }
    assert!(
        witnessed,
        "must reach a real Recorded append before the recoverable wait"
    );
}

#[test]
fn place_same_pump_stop_prevents_first_append_byte_and_preserves_lifecycle_envelope() {
    for stop_requested in [false, true] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("stopped-before-send");
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        let before = fx.journal();
        let (admin, peer) = OrganChannel::pair().unwrap();
        let mut admin = LifecyclePeer::from_channel(admin).unwrap();
        let command = directive(if stop_requested {
            weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            }
        } else {
            weaver_types::LifecycleDirective::Observe
        });
        send_bytes(peer.as_fd(), &serde_json::to_vec(&command).unwrap());
        let run = &mut fx.summary.boot.member.run;
        let mut clock = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        let first = run.context.pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
            },
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut clock,
        });
        assert_eq!(admin.take_completion().unwrap().unwrap().envelope, command);
        let mut packet = receive(fx.peer.as_fd());
        if stop_requested {
            assert!(
                packet.is_none(),
                "writable append cannot bypass a newly observed Stop"
            );
            // The common pump's held-command skip no longer protects this retry.
            // Cancellation must stay with the actual owner after the envelope leaves.
            let run = &mut fx.summary.boot.member.run;
            let mut clock = ReadyClock {
                clock: &mut run.clock,
                fd: fx.decode.fd().as_raw_fd(),
            };
            assert_eq!(
                run.context
                    .pump_once(&mut PumpPorts {
                        lifecycle: LifecyclePorts {
                            listener: None,
                            peer: Some(&mut admin),
                            residency: None
                        },
                        gate: None,
                        decode: Some(&mut fx.decode),
                        label: None,
                        io: &mut clock
                    })
                    .unwrap_err(),
                RunFailure::PlaceOwned
            );
            assert!(
                receive(fx.peer.as_fd()).is_none(),
                "taking Stop must not restore first-byte authority"
            );
            assert_eq!(fx.journal().1, before.1);
            assert_eq!(fx.journal().6, 0);
            let run = &mut fx.summary.boot.member.run;
            run.context
                .cancel_generation(&mut fx.decode, &mut run.clock)
                .unwrap();
            assert!(matches!(
                fx.finish_generation(),
                GenerationCompletion::Cancelled(_)
            ));
        } else {
            first.unwrap();
            for _ in 0..16 {
                if packet.is_some() {
                    break;
                }
                let run = &mut fx.summary.boot.member.run;
                let mut clock = ReadyClock {
                    clock: &mut run.clock,
                    fd: fx.decode.fd().as_raw_fd(),
                };
                run.context
                    .pump_once(&mut PumpPorts {
                        lifecycle: LifecyclePorts {
                            listener: None,
                            peer: Some(&mut admin),
                            residency: None,
                        },
                        gate: None,
                        decode: Some(&mut fx.decode),
                        label: None,
                        io: &mut clock,
                    })
                    .unwrap();
                packet = receive(fx.peer.as_fd());
            }
            assert!(matches!(
                serde_json::from_slice::<TokenDirective>(
                    &packet.expect("Observe permits actual granted send with the same readiness")
                )
                .unwrap(),
                TokenDirective::AppendAndGenerate { .. }
            ));
        }
    }
}

#[test]
fn place_real_field_keeps_its_own_position_and_model_raw_boxes_without_invented_token_join() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("field-stream");
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Token {
        token: 31,
        piece: "piece spanning draws".into(),
    });
    for _ in 0..4 {
        fx.advance_generation().unwrap();
    }
    let ranked = vec![
        weaver_types::Candidate {
            token: 9,
            probability: 0.25,
        },
        weaver_types::Candidate {
            token: 31,
            probability: 0.125,
        },
    ];
    fx.answer(&TokenAnswer::Field {
        position: 73,
        ranked: ranked.clone(),
        realized: 1,
    });
    for _ in 0..16 {
        fx.advance_generation().unwrap();
        if fx.records(Kind::ModelField).len() == 1 {
            break;
        }
    }
    let field = fx
        .records(Kind::ModelField)
        .pop()
        .expect("actual Field authors its own position");
    assert_eq!(field["payload"]["position"], 73);
    assert_eq!(field["payload"]["realized"], 1);
    assert_eq!(
        field["payload"]["ranked"],
        serde_json::to_value(ranked).unwrap()
    );
    assert_eq!(fx.journal().6, 0);
    assert_eq!(fx.records(Kind::ModelOutput).len(), 0);
    let terminal = generated(before.1, 3, 1, "Read result.", Finish::Completed, before.2);
    let expected_request = terminal.request.get().to_owned();
    let expected_measurement = terminal.measurement.get().to_owned();
    fx.answer(&TokenAnswer::Generated(terminal));
    let GenerationCompletion::Generated(proof) = fx.finish_generation() else {
        panic!("valid field stream failed");
    };
    assert_eq!(proof.generation().request.get(), expected_request);
    assert_eq!(proof.generation().measurement.get(), expected_measurement);
    assert_eq!(fx.records(Kind::ModelRequest).len(), 1);
    assert_eq!(fx.records(Kind::ModelOutput).len(), 1);
    assert_eq!(fx.records(Kind::ModelMeasurement).len(), 1);
    assert!(fx.run().context.completed_author.is_none());
}

#[test]
fn place_serving_column_ask_absence_refuses_diagnostic_column_without_confirming_generation() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("unelected-column");
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Column {
        position: 0,
        layers: vec![vec![1.0, 2.0]],
    });
    let failure = fx.generation_failure();
    assert_eq!(fx.run().context.failure(), Some(&failure));
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, 0);
    assert_eq!(fx.records(Kind::ModelOutput).len(), 0);
    assert!(receive(fx.peer.as_fd()).is_none());
}

struct PlaceReadyAtBound<'a> {
    clock: &'a mut FakePumpIo,
    fd: RawFd,
    at: u64,
    waited: bool,
}
impl PumpIo for PlaceReadyAtBound<'_> {
    fn now(&mut self) -> io::Result<u64> {
        self.clock.now()
    }
    fn wait(&mut self, interests: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        let index = interests
            .iter()
            .position(|interest| interest.fd.as_raw_fd() == self.fd)
            .expect("actual decode interest");
        self.clock.advance_to(self.at)?;
        self.waited = true;
        Ok(WaitOutcome::Ready(vec![ReadyFd {
            index,
            events: interests[index].events & (PollFlags::POLLIN | PollFlags::POLLOUT),
        }]))
    }
}

#[test]
fn place_first_generation_byte_rechecks_inherited_leave_after_actual_readiness_wait() {
    for expired in [false, true] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("wait-crossing");
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        let run = &mut fx.summary.boot.member.run;
        let bound = AbsoluteBound::after(run.clock.now().unwrap(), 100).unwrap();
        run.context
            .arm(PumpDeadline::Leave, bound, &mut run.clock)
            .unwrap();
        let mut io = PlaceReadyAtBound {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
            at: bound.deadline_ns() - u64::from(!expired),
            waited: false,
        };
        let result = run.context.advance_generation(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        });
        assert!(
            io.waited,
            "test must cross the actual transport readiness wait"
        );
        if expired {
            assert_eq!(
                result.unwrap_err(),
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound
                }
            );
            assert!(
                receive(fx.peer.as_fd()).is_none(),
                "readiness at the inherited deadline cannot send Append"
            );
            assert!(fx.run().context.failure().is_none());
            assert_eq!(fx.journal().6, 0);
        } else {
            result.unwrap();
            let raw = receive(fx.peer.as_fd())
                .expect("otherwise valid grant must really send before the bound");
            assert!(matches!(
                serde_json::from_slice::<TokenDirective>(&raw).unwrap(),
                TokenDirective::AppendAndGenerate { .. }
            ));
        }
    }
}

#[test]
fn place_installed_journal_blocks_unreserved_mutations_but_allows_private_grant_and_cancel() {
    for kind in [
        "append", "flush", "elide", "open", "refeed", "cancel", "private",
    ] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("owned-journal");
        fx.generation(units, 3, 1, "Read result.", Finish::Completed);
        let before = fx.journal();
        let turn = TurnKey("owned-journal".into());
        let request = match kind {
            "append" => Some(TokenDirective::AppendAndGenerate {
                turn,
                delta: vec![],
                tool_schemas: fx.schemas(),
            }),
            "flush" => Some(TokenDirective::Flush { keep: before.0 }),
            "elide" => Some(TokenDirective::Elide {
                from: before.0,
                to: before.1,
            }),
            "open" => Some(TokenDirective::Open {
                session: weaver_types::SessionId("same-session".into()),
                messages: vec![],
                column_ask: false,
            }),
            "refeed" => Some(TokenDirective::ReFeed {
                turn,
                rendered: "retained contribution".into(),
                path: vec![31],
            }),
            "cancel" => Some(TokenDirective::Cancel { turn }),
            "private" => None,
            _ => unreachable!(),
        };
        if let Some(request) = &request {
            assert!(matches!(
                fx.decode.stage(request.clone()).unwrap(),
                Stage::Queued
            ));
        } else {
            let units = fx.text(9, "later");
            let grant = fx.grant(units, 2);
            fx.start_generation(grant);
        }
        if matches!(kind, "cancel" | "private") {
            let mut packet = None;
            for _ in 0..16 {
                let run = &mut fx.summary.boot.member.run;
                let mut clock = ReadyClock {
                    clock: &mut run.clock,
                    fd: fx.decode.fd().as_raw_fd(),
                };
                run.context
                    .pump_once(&mut PumpPorts {
                        lifecycle: LifecyclePorts::default(),
                        gate: None,
                        decode: Some(&mut fx.decode),
                        label: None,
                        io: &mut clock,
                    })
                    .unwrap();
                packet = receive(fx.peer.as_fd());
                if packet.is_some() {
                    break;
                }
            }
            let sent: TokenDirective =
                serde_json::from_slice(&packet.expect("permitted transport must actually send"))
                    .unwrap();
            if let Some(request) = request {
                assert_eq!(sent, request);
            } else {
                assert!(matches!(sent, TokenDirective::AppendAndGenerate { .. }));
            }
        } else {
            for _ in 0..2 {
                let run = &mut fx.summary.boot.member.run;
                let mut clock = ReadyClock {
                    clock: &mut run.clock,
                    fd: fx.decode.fd().as_raw_fd(),
                };
                assert_eq!(
                    run.context
                        .pump_once(&mut PumpPorts {
                            lifecycle: LifecyclePorts::default(),
                            gate: None,
                            decode: Some(&mut fx.decode),
                            label: None,
                            io: &mut clock
                        })
                        .unwrap_err(),
                    RunFailure::PlaceOwned,
                    "{kind}"
                );
                assert!(fx.decode.unowned_mutation_not_started());
                assert!(
                    receive(fx.peer.as_fd()).is_none(),
                    "{kind}: no byte may bypass the private owner"
                );
            }
            assert_eq!(fx.journal(), before, "{kind}: journal remains unchanged");
        }
    }
}

#[test]
fn place_generation_rejects_foreign_actual_decoder_and_returns_exact_grant_for_original_port() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("foreign-decoder");
    let grant = fx.grant(units, 3);
    let before = fx.journal();
    let assembly = grant.assembly_id();
    let sequence = grant.placement_sequence();
    let delta = grant.delta().to_vec();
    let (foreign, peer) = DecodeChannel::pair().unwrap();
    let mut foreign = DecodePort::from_channel(foreign).unwrap();
    let run = fx.run();
    let error = run
        .context
        .begin_generation(grant, &mut foreign, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.failure, Some(RunFailure::PlaceOwned));
    assert_eq!(error.input.assembly_id(), assembly);
    assert_eq!(error.input.placement_sequence(), sequence);
    assert_eq!(error.input.delta(), delta);
    assert!(receive(peer.as_fd()).is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    assert_eq!(fx.journal(), before);
    assert!(fx.run().context.failure().is_none());
    fx.start_generation(error.input);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        before.1,
        3,
        0,
        "",
        Finish::Completed,
        before.2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    assert_eq!(fx.journal().6, 2);
}

#[test]
fn place_extracted_grant_stop_retains_refused_authority_until_original_turn_closes() {
    for stop in [false, true] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        let later = fx.text(9, "later");
        fx.turn("extracted-grant");
        let grant = fx.grant(units, 3);
        let sequence = grant.placement_sequence();
        let (admin, peer) = OrganChannel::pair().unwrap();
        let mut admin = LifecyclePeer::from_channel(admin).unwrap();
        let command = directive(if stop {
            weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            }
        } else {
            weaver_types::LifecycleDirective::Observe
        });
        send_bytes(peer.as_fd(), &serde_json::to_vec(&command).unwrap());
        let run = &mut fx.summary.boot.member.run;
        let mut clock = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        run.context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: None,
                    peer: Some(&mut admin),
                    residency: None,
                },
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut clock,
            })
            .unwrap();
        assert_eq!(admin.take_completion().unwrap().unwrap().envelope, command);
        assert!(receive(fx.peer.as_fd()).is_none());
        if stop {
            let run = &mut fx.summary.boot.member.run;
            let error = run
                .context
                .begin_generation(grant, &mut fx.decode, &mut run.clock)
                .unwrap_err();
            assert_eq!(error.refusal, PlaceError::WrongProof);
            assert!(error.failure.is_none());
            assert_eq!(error.input.placement_sequence(), sequence);
            let account = run.context.cancel_placement_grant(error.input).unwrap();
            assert!(account.raw_bytes().is_none());
            assert!(account.confirmed_generation().is_none());
            assert!(!run.context.native_place_cache_ready());
            // This refusal must survive removal of the outstanding grant. A new
            // raw TurnStarted cannot overwrite the stopped original bracket.
            let mut early = crate::flow::context::tests::event(Kind::TurnStarted, None);
            early.turn = Some(weaver_trace::TurnRef("too-early".into()));
            assert_eq!(
                run.author(early.clone()).unwrap_err(),
                RunFailure::PlaceOwned
            );
            assert_eq!(run.context.refused_author().unwrap().turn, early.turn);
            assert_eq!(run.context.open_turn.as_ref().unwrap().0, "extracted-grant");
            assert_eq!(
                run.context
                    .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                    .unwrap_err(),
                PlaceError::WrongPhase
            );
            // Even an unrelated closed bracket cannot retire the stopped one.
            let wrong_close = AuthoredEvent {
                kind: Kind::TurnClosed,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(weaver_trace::TurnRef("too-early".into())),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::TurnClosed(weaver_trace::TurnClose::Clean)),
            };
            run.author(wrong_close).unwrap();
            assert_eq!(run.context.open_turn.as_ref().unwrap().0, "extracted-grant");
            assert_eq!(
                run.context
                    .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                    .unwrap_err(),
                PlaceError::WrongPhase
            );
            assert!(receive(fx.peer.as_fd()).is_none());
            assert_eq!(fx.journal().6, 0);
        } else {
            fx.start_generation(grant);
            fx.sent_generation();
            fx.answer(&TokenAnswer::Generated(generated(
                fx.journal().1,
                3,
                0,
                "",
                Finish::Completed,
                fx.journal().2,
            )));
            assert!(matches!(
                fx.finish_generation(),
                GenerationCompletion::Generated(_)
            ));
        }
        fx.close_turn();
        fx.turn("after-original-close");
        assert!(
            fx.run().context.native_place_cache_ready(),
            "Stop ends with its actual turn, not the whole loaded run"
        );
        fx.generation(later, 2, 0, "", Finish::Completed);
        assert!(fx.run().context.failure().is_none());
    }
}

fn place_with_pending_effect_summary() -> (PlaceFixture, s::PlanSnapshot) {
    let mut fx = PlaceFixture::new();
    let seed = fx.summary.seed().to_owned();
    let filing = fx.summary.file(effect(&seed, "authorize", None), false);
    let mut snapshot = fx.summary.snapshot();
    snapshot.last_source = committed_position(&filing);
    fx.summary.begin_effect(filing);
    assert!(fx.run().context.plan_summary_blocks_generation());
    assert!(fx.run().context.loaded_place_schemas().is_some());
    (fx, snapshot)
}

#[test]
fn place_join_defers_raw_append_until_actual_summary_recorded_then_rechecks_ownership() {
    let (mut fx, snapshot) = place_with_pending_effect_summary();
    let expected_source = snapshot.last_source.clone();
    let original_journal = fx.journal();
    let original_fd = fx.decode.fd().as_raw_fd();
    let request = TokenDirective::AppendAndGenerate {
        turn: TurnKey("unreserved-summary-wait".into()),
        delta: vec![],
        tool_schemas: fx.schemas(),
    };
    assert!(matches!(fx.decode.stage(request).unwrap(), Stage::Queued));
    let raw = summary_bytes(Some(snapshot));
    let mut read_seen = false;
    let mut release_seen = false;
    let mut completed = false;
    for _ in 0..256 {
        let run = &mut fx.summary.boot.member.run;
        let mut io = SummaryReadyClock(&mut run.clock);
        let mut supplied = PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        };
        run.context.pump_once(&mut supplied).unwrap();
        let progress = run.context.advance_plan_summary(&mut supplied).unwrap();
        assert_eq!(
            supplied.decode.as_deref().unwrap().fd().as_raw_fd(),
            original_fd
        );
        assert!(fx.decode.generation_not_started());
        assert!(receive(fx.peer.as_fd()).is_none());
        let mut line = [0u8; 65536];
        let received = match fx.run().state.read(&mut line) {
            Ok(count) => {
                Some(s::decode_state_line(&line[..count], s::FrameDirection::Request).unwrap())
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => None,
            other => panic!("actual summary request read: {other:?}"),
        };
        if let Some(request) = received {
            let answer = match &request {
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::PlanCurrent {},
                        },
                    ..
                } => {
                    assert!(!read_seen);
                    read_seen = true;
                    page(913, 0, raw.len(), &raw)
                }
                s::StateFrame::Control {
                    frame: s::StateControl::Release { snapshot_id },
                    ..
                } => {
                    assert!(read_seen && !release_seen);
                    release_seen = true;
                    assert_eq!(snapshot_id.0, 913);
                    s::StateControlAnswer::Released {
                        snapshot_id: *snapshot_id,
                    }
                }
                other => panic!("unexpected required summary request {other:?}"),
            };
            let response = fx.summary.boot.member.answer(&request, answer);
            // The member's actual scripted observation ends at current + 2.
            let received_at = fx.summary.boot.member.now() + 3;
            fx.run().clock.advance_to(received_at).unwrap();
            fx.run()
                .state
                .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
                .unwrap();
        }
        if progress == PlanSummaryProgress::Complete {
            completed = true;
            break;
        }
    }
    assert!(read_seen && release_seen && completed);
    let run = fx.run();
    let PlanSummaryCompletion::Effect(proof) = run
        .context
        .take_plan_summary(&mut run.clock)
        .unwrap()
        .unwrap()
    else {
        panic!("actual committed effect refresh did not complete");
    };
    assert_eq!(proof.read().raw_bytes(), raw);
    assert_eq!(proof.summary().unwrap().last_source, expected_source);
    assert!(!run.context.plan_summary_blocks_generation());
    // Cache completion removes only summary deferral. It never licenses the
    // attempted raw append to mutate the installed placement journal.
    for _ in 0..2 {
        let run = &mut fx.summary.boot.member.run;
        let mut io = SummaryReadyClock(&mut run.clock);
        let mut supplied = PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        };
        assert_eq!(
            run.context.pump_once(&mut supplied).unwrap_err(),
            RunFailure::PlaceOwned
        );
        assert_eq!(
            supplied.decode.as_deref().unwrap().fd().as_raw_fd(),
            original_fd
        );
        assert!(fx.decode.unowned_mutation_not_started());
        assert!(receive(fx.peer.as_fd()).is_none());
    }
    assert_eq!(fx.journal(), original_journal);
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_join_pending_summary_still_checks_other_mutations_and_sends_cancel() {
    for kind in ["flush", "elide", "open", "refeed", "cancel"] {
        let (mut fx, _) = place_with_pending_effect_summary();
        let before = fx.journal();
        let original_fd = fx.decode.fd().as_raw_fd();
        let turn = TurnKey("pending-summary-control".into());
        let request = match kind {
            "flush" => TokenDirective::Flush { keep: before.0 },
            "elide" => TokenDirective::Elide {
                from: before.0,
                to: before.1,
            },
            "open" => TokenDirective::Open {
                session: weaver_types::SessionId("same-session".into()),
                messages: vec![],
                column_ask: false,
            },
            "refeed" => TokenDirective::ReFeed {
                turn,
                rendered: "retained contribution".into(),
                path: vec![31],
            },
            "cancel" => TokenDirective::Cancel { turn },
            _ => unreachable!(),
        };
        assert!(matches!(
            fx.decode.stage(request.clone()).unwrap(),
            Stage::Queued
        ));
        assert!(!fx.decode.generation_not_started());
        let mut sent = None;
        for _ in 0..16 {
            let run = &mut fx.summary.boot.member.run;
            let mut io = SummaryReadyClock(&mut run.clock);
            let mut supplied = PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io,
            };
            let outcome = run.context.pump_once(&mut supplied);
            assert_eq!(
                supplied.decode.as_deref().unwrap().fd().as_raw_fd(),
                original_fd
            );
            if kind == "cancel" {
                outcome.unwrap();
                sent = receive(fx.peer.as_fd());
                if sent.is_some() {
                    break;
                }
            } else {
                assert_eq!(outcome.unwrap_err(), RunFailure::PlaceOwned, "{kind}");
                assert!(fx.decode.unowned_mutation_not_started());
                assert!(receive(fx.peer.as_fd()).is_none());
                break;
            }
        }
        if kind == "cancel" {
            assert_eq!(
                serde_json::from_slice::<TokenDirective>(&sent.expect("Cancel must actually send"))
                    .unwrap(),
                request
            );
        }
        assert!(fx.run().context.plan_summary_blocks_generation());
        assert!(fx.run().context.failure().is_none());
        assert_eq!(fx.journal(), before);
    }
}

#[test]
fn place_join_pending_and_revoked_summary_keep_decoder_peer_failure_visible() {
    for revoked in [false, true] {
        let (mut fx, _) = place_with_pending_effect_summary();
        if revoked {
            let run = fx.run();
            run.context.cancel_plan_summary(&mut run.clock).unwrap();
            assert!(matches!(
                fx.summary.finish(),
                PlanSummaryCompletion::Cancelled(_)
            ));
        }
        let before = fx.journal();
        assert!(fx.run().context.plan_summary_blocks_generation());
        assert!(matches!(
            fx.decode
                .stage(TokenDirective::AppendAndGenerate {
                    turn: TurnKey("summary-peer-closed".into()),
                    delta: vec![],
                    tool_schemas: fx.schemas(),
                })
                .unwrap(),
            Stage::Queued
        ));
        let original_fd = fx.decode.fd().as_raw_fd();
        nix::sys::socket::shutdown(
            fx.peer.as_fd().as_raw_fd(),
            nix::sys::socket::Shutdown::Both,
        )
        .unwrap();
        assert!(
            fx.decode.failure().is_none(),
            "the common pump must discover the close"
        );
        let run = &mut fx.summary.boot.member.run;
        let mut io = SummaryReadyClock(&mut run.clock);
        let mut supplied = PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        };
        let error = run.context.pump_once(&mut supplied).unwrap_err();
        assert!(matches!(
            error,
            RunFailure::Transport {
                source: PumpSource::Decode,
                ..
            }
        ));
        assert_eq!(
            supplied.decode.as_deref().unwrap().fd().as_raw_fd(),
            original_fd
        );
        assert_eq!(
            fx.decode.failure().unwrap().fault,
            crate::failure::ChannelFault::Closed
        );
        assert!(fx.decode.generation_not_started());
        assert!(fx.decode.send_pending());
        assert!(
            !fx.decode.replace_generation_send_hold(false),
            "scoped hold must restore even after transport failure"
        );
        assert_eq!(fx.journal(), before);
        assert!(fx.run().context.plan_summary_blocks_generation());
    }
}

#[test]
fn place_join_pending_and_revoked_summary_report_the_original_standing_decode_failure() {
    for revoked in [false, true] {
        let (mut fx, _) = place_with_pending_effect_summary();
        if revoked {
            let run = fx.run();
            run.context.cancel_plan_summary(&mut run.clock).unwrap();
            assert!(matches!(
                fx.summary.finish(),
                PlanSummaryCompletion::Cancelled(_)
            ));
        }
        let before = fx.journal();
        assert!(matches!(
            fx.decode
                .stage(TokenDirective::AppendAndGenerate {
                    turn: TurnKey("summary-standing-failure".into()),
                    delta: vec![],
                    tool_schemas: fx.schemas(),
                })
                .unwrap(),
            Stage::Queued
        ));
        nix::sys::socket::shutdown(
            fx.peer.as_fd().as_raw_fd(),
            nix::sys::socket::Shutdown::Both,
        )
        .unwrap();
        let original = fx.decode.try_receive().unwrap_err();
        let run = &mut fx.summary.boot.member.run;
        let mut io = SummaryReadyClock(&mut run.clock);
        let error = run
            .context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io,
            })
            .unwrap_err();
        assert_eq!(
            error,
            RunFailure::Transport {
                source: PumpSource::Decode,
                failure: original.clone()
            }
        );
        assert_eq!(fx.decode.failure(), Some(&original));
        assert!(fx.decode.generation_not_started());
        assert!(fx.decode.send_pending());
        assert!(!fx.decode.replace_generation_send_hold(false));
        assert_eq!(fx.journal(), before);
        assert!(fx.run().context.plan_summary_blocks_generation());
    }
}

#[test]
fn place_same_pump_stop_retains_decoder_health_before_the_first_append_byte() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("stopped-with-closed-decoder");
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    let before = fx.journal();
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    let command = directive(weaver_types::LifecycleDirective::Stop {
        cause: weaver_types::Cause { uid: 0 },
    });
    send_bytes(peer.as_fd(), &serde_json::to_vec(&command).unwrap());
    nix::sys::socket::shutdown(
        fx.peer.as_fd().as_raw_fd(),
        nix::sys::socket::Shutdown::Both,
    )
    .unwrap();
    let run = &mut fx.summary.boot.member.run;
    let mut io = SummaryReadyClock(&mut run.clock);
    let outcome = run.context.pump_once(&mut PumpPorts {
        lifecycle: LifecyclePorts {
            listener: None,
            peer: Some(&mut admin),
            residency: None,
        },
        gate: None,
        decode: Some(&mut fx.decode),
        label: None,
        io: &mut io,
    });
    assert!(
        outcome.is_err(),
        "held Stop cannot conceal a failed decoder"
    );
    assert_eq!(admin.take_completion().unwrap().unwrap().envelope, command);
    assert_eq!(
        fx.decode.failure().unwrap().fault,
        crate::failure::ChannelFault::Closed
    );
    assert!(fx.decode.generation_not_started());
    assert!(fx.decode.send_pending());
    assert_eq!(fx.journal(), before);
}

#[test]
fn place_extracted_boot_proof_cannot_install_after_held_enter_cancellation() {
    for command_case in 0..5 {
        let mut summary = SummaryFixture::new(vec![]);
        let proof = summary.boot(None);
        let raw = proof.read().raw_bytes().to_vec();
        let raw_pointer = proof.read().raw_bytes().as_ptr();
        let recall = proof.read().recall_sequence();
        let head = proof.repaired().checked_open().unwrap().head();
        let enter = proof.repaired().checked_open().unwrap().enter();
        assert!(proof.summary().is_none());
        assert!(summary.run().context.open_turn.is_none());
        let journal = summary
            .run()
            .context
            .boot_journal
            .as_ref()
            .unwrap()
            .boot_snapshot();
        let before = summary.records(Kind::StatePlacement);
        assert_eq!(before.len(), 1);
        assert!(!summary.run().context.plan_summary_blocks_generation());
        assert!(summary.run().context.committed_plan_summary().is_ok());
        let (mut decode, decoder_peer) = summary._decode.take().unwrap();
        let stopping = command_case >= 2;
        if command_case != 0 {
            let command = directive(match command_case {
                1 => weaver_types::LifecycleDirective::Observe,
                2 => weaver_types::LifecycleDirective::Stop {
                    cause: weaver_types::Cause { uid: 0 },
                },
                3 => weaver_types::LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 0 },
                    forced: true,
                    rollback: true,
                },
                4 => weaver_types::LifecycleDirective::JoinLeave {
                    cause: weaver_types::Cause { uid: 0 },
                },
                _ => unreachable!(),
            });
            let (admin, lifecycle_peer) = OrganChannel::pair().unwrap();
            let mut admin = LifecyclePeer::from_channel(admin).unwrap();
            send_bytes(
                lifecycle_peer.as_fd(),
                &serde_json::to_vec(&command).unwrap(),
            );
            for _ in 0..8 {
                let run = summary.run();
                let mut io = SummaryReadyClock(&mut run.clock);
                run.context
                    .pump_once(&mut PumpPorts {
                        lifecycle: LifecyclePorts {
                            listener: None,
                            peer: Some(&mut admin),
                            residency: None,
                        },
                        gate: None,
                        decode: Some(&mut decode),
                        label: None,
                        io: &mut io,
                    })
                    .unwrap();
                if admin.held().is_some() {
                    break;
                }
            }
            assert!(
                admin.held().is_some(),
                "actual lifecycle envelope must be observed"
            );
            assert_eq!(summary.run().context.boot_enter_cancelled, stopping);
            // The transaction was extracted before this command. Its completed
            // cache remains evidence; revocation is held by the boot lifetime.
            assert!(summary.run().context.committed_plan_summary().is_ok());
            assert_eq!(admin.take_completion().unwrap().unwrap().envelope, command);
            assert!(receive(lifecycle_peer.as_fd()).is_none());
        }
        let run = summary.run();
        let result = run.context.install_place(proof, &mut run.clock);
        if stopping {
            let error = result.unwrap_err();
            assert_eq!(error.refusal, PlaceError::WrongProof);
            assert!(error.failure.is_none());
            assert!(run.context.failure().is_none());
            assert!(run.context.loaded_place_schemas().is_none());
            assert_eq!(error.prerequisite.read().raw_bytes(), raw);
            assert_eq!(error.prerequisite.read().raw_bytes().as_ptr(), raw_pointer);
            assert_eq!(error.prerequisite.read().recall_sequence(), recall);
            assert!(error.prerequisite.summary().is_none());
            assert!(error.prerequisite.repaired().belongs_to(&run.context));
            assert_eq!(
                error.prerequisite.repaired().checked_open().unwrap().head(),
                head
            );
            assert_eq!(
                error
                    .prerequisite
                    .repaired()
                    .checked_open()
                    .unwrap()
                    .enter(),
                enter
            );
            assert!(run.context.boot_enter_cancelled);
            // Consuming the held envelope cannot restore boot admission.
            let again = run
                .context
                .install_place(error.prerequisite, &mut run.clock)
                .unwrap_err();
            assert_eq!(again.prerequisite.read().raw_bytes().as_ptr(), raw_pointer);
            assert_eq!(again.refusal, PlaceError::WrongProof);
            assert!(again.failure.is_none());
            assert!(run.context.loaded_place_schemas().is_none());
        } else {
            result.unwrap();
            assert!(!run.context.boot_enter_cancelled);
            assert!(run.context.loaded_place_schemas().is_some());
        }
        assert_eq!(
            run.context.boot_journal.as_ref().unwrap().boot_snapshot(),
            journal
        );
        assert_eq!(summary.records(Kind::StatePlacement), before);
        assert!(receive(decoder_peer.as_fd()).is_none());
    }
}
