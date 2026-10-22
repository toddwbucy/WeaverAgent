use super::*;
use crate::ServingStore;
use weaver_types::state_flow::*;

fn identity() -> MemberObservationIdentity {
    MemberObservationIdentity {
        instrument_version: "state-observation/2".into(),
        pid: 123,
        start_ticks: Some(10.into()),
        ticks_per_second: Some(100.into()),
        unavailable_reason: None,
    }
}

fn observation(id: u64, request: u64, operation: StateOperation) -> StateOperationObservation {
    StateOperationObservation {
        operation_id: id.into(),
        parent_id: None,
        request_id: request.into(),
        operation,
        began_ns: 10.into(),
        ended_ns: 20.into(),
        phases: vec![
            OperationPhase {
                name: PhaseName::Validate,
                began_ns: 10.into(),
                ended_ns: 11.into(),
            },
            OperationPhase {
                name: PhaseName::Encode,
                began_ns: 19.into(),
                ended_ns: 20.into(),
            },
        ],
        resource: OperationResource {
            cpu_ns: None,
            copied_bytes: Some(0.into()),
            held_bytes: Some(0.into()),
            unavailable: vec![ResourceUnavailable {
                field: ResourceField::CpuNs,
                reason: ObservationUnavailable::NotSampled,
            }],
        },
        disk: OperationDisk {
            enabled: None,
            before: None,
            after: None,
            unavailable_reason: Some(ObservationUnavailable::NotSampled),
        },
    }
}

fn store() -> ServingSqlite {
    let request = super::super::image::tests::fixture();
    let image = super::super::image::seed_image(&request).unwrap();
    let start = StartIdentity {
        seed_id: request.seed_id,
        artifact_id: "fixture".into(),
        dump_sha256: super::super::image::sha256(image.as_bytes()),
        config_sha256: super::super::image::sha256(b"fixture declaration"),
        parent_artifact_id: None,
        position: None,
    };
    ServingSqlite::adopt(
        image.as_bytes(),
        &Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 1.into(),
        },
        "state-content/1",
        &request.scheme_id,
        &start,
        ReadLimits {
            max_result_bytes: 1_000_000.into(),
            measurement_ref: "fixture".into(),
        },
    )
    .unwrap()
}

fn stored(store: &ServingSqlite, run: &str) -> Vec<StoredOperationObservation> {
    match store
        .query(&ReadSelector::Observations {
            run: run.into(),
            after: 0.into(),
            through: None,
        })
        .unwrap()
    {
        QueryResult::Observations { records, .. } => records,
        _ => panic!("wrong read variant"),
    }
}

#[test]
fn open_identity_and_observation_commit_together_and_cannot_be_replaced() {
    let mut store = store();
    store.connection.execute_batch("CREATE TEMP TRIGGER refuse_metadata BEFORE INSERT ON operation_observation BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(matches!(
        store.begin_observations(&identity(), &observation(1, 1, StateOperation::Open)),
        Err(ReadFault::ComponentFailure { .. })
    ));
    assert_eq!(
        store
            .connection
            .query_row("SELECT COUNT(*) FROM observation_run", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    store
        .connection
        .execute_batch("DROP TRIGGER refuse_metadata")
        .unwrap();
    let open = observation(1, 1, StateOperation::Open);
    store.begin_observations(&identity(), &open).unwrap();
    assert_eq!(
        stored(&store, "run"),
        vec![StoredOperationObservation {
            request_source: None,
            observation: open.clone()
        }]
    );
    let mut other_identity = identity();
    other_identity.pid = 456;
    assert!(store.begin_observations(&other_identity, &open).is_err());
    match store
        .query(&ReadSelector::Observations {
            run: "run".into(),
            after: 0.into(),
            through: None,
        })
        .unwrap()
    {
        QueryResult::Observations {
            member_observation,
            records,
            ..
        } => {
            assert_eq!(member_observation, identity());
            assert_eq!(records.len(), 1);
        }
        _ => panic!("wrong read variant"),
    }
}

#[test]
fn open_metadata_refuses_a_noninitial_request_before_any_insert() {
    let mut store = store();
    let invalid = observation(1, 2, StateOperation::Open);
    assert!(matches!(
        store.begin_observations(&identity(), &invalid),
        Err(ReadFault::ComponentFailure { .. })
    ));
    for table in ["observation_run", "operation_observation"] {
        assert_eq!(
            store
                .connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    store
        .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
        .unwrap();
    assert_eq!(stored(&store, "run").len(), 1);
}

#[test]
fn metadata_is_separate_from_content_and_a_failed_insert_does_not_undo_content() {
    let mut store = store();
    store
        .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
        .unwrap();
    let before = store.held().unwrap();
    let mut filing = super::super::tests::filing::statement(2, "message.user");
    filing.scheme_id = store.context.scheme_id.clone();
    store.file(&filing).unwrap();
    let committed = store.position().unwrap();
    let record = StoredOperationObservation {
        request_source: Some(filing.block_id.clone()),
        observation: observation(2, 2, StateOperation::File),
    };
    store.connection.execute_batch("CREATE TEMP TRIGGER refuse_metadata BEFORE INSERT ON operation_observation BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(matches!(
        store.record_observation(&record),
        Err(ReadFault::ComponentFailure { .. })
    ));
    assert_eq!(store.position().unwrap(), committed);
    assert_eq!(store.held().unwrap().0, before.0 + 1);
    assert_eq!(stored(&store, "run").len(), 1);
    // The actual member closes here. A later test-only insert proves that this
    // metadata port changes no source count or content frontier of its own.
    store
        .connection
        .execute_batch("DROP TRIGGER refuse_metadata")
        .unwrap();
    store.record_observation(&record).unwrap();
    assert_eq!(stored(&store, "run")[1], record);
    assert_eq!(store.held().unwrap().0, before.0 + 1);
    assert_eq!(store.position().unwrap(), committed);
    assert!(
        store.file(&filing).is_err(),
        "the first content commit survived"
    );
}

#[test]
fn observation_ids_are_contiguous_while_continuations_keep_the_request_id() {
    let mut store = store();
    store
        .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
        .unwrap();
    let record = |id, operation| StoredOperationObservation {
        request_source: None,
        observation: observation(id, 2, operation),
    };
    assert!(
        store
            .record_observation(&record(3, StateOperation::Read))
            .is_err()
    );
    store
        .record_observation(&record(2, StateOperation::Snapshot))
        .unwrap();
    store
        .record_observation(&record(3, StateOperation::Abort))
        .unwrap();
    assert!(
        store
            .record_observation(&record(3, StateOperation::Abort))
            .is_err()
    );
    assert!(
        store
            .record_observation(&record(4, StateOperation::Open))
            .is_err()
    );
    let mut refused = record(4, StateOperation::File);
    refused.request_source = Some(SourceBlockId {
        run: "wrong-request-run".into(),
        sequence: 2.into(),
        block: 0.into(),
    });
    store.record_observation(&refused).unwrap();
    assert_eq!(stored(&store, "run")[3], refused);
    assert_eq!(
        store.held().unwrap().0,
        1,
        "refusal metadata is not held content"
    );
    let mut invalid = record(5, StateOperation::Read);
    invalid.observation.resource.cpu_ns = Some(1.into());
    assert!(store.record_observation(&invalid).is_err());
    assert_eq!(stored(&store, "run").len(), 4);
}

#[test]
fn captured_metadata_has_a_fixed_frontier_and_reopen_requires_a_new_run() {
    let mut store = store();
    store
        .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
        .unwrap();
    let image = store.image().unwrap();
    store
        .record_observation(&StoredOperationObservation {
            request_source: None,
            observation: observation(2, 2, StateOperation::Snapshot),
        })
        .unwrap();
    let mut start = store.context.start.clone();
    start.dump_sha256 = super::super::image::sha256(&image);
    let mut common = Common {
        session_id: "session-2".into(),
        run_id: "run".into(),
        request_id: 1.into(),
    };
    let load = |common: &Common| {
        ServingSqlite::adopt(
            &image,
            common,
            "state-content/1",
            &store.context.scheme_id,
            &start,
            store.read_limits.clone(),
        )
    };
    assert_eq!(
        load(&common).err().unwrap().code,
        StateRefusalCode::OpenFailed
    );
    common.run_id = "new-run".into();
    let mut restored = load(&common).unwrap();
    assert_eq!(stored(&restored, "run").len(), 1);
    restored
        .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
        .unwrap();
    assert_eq!(stored(&restored, "new-run").len(), 1);
    assert_eq!(restored.held().unwrap().0, 1);
    assert!(restored.position().unwrap().is_none());
}

#[test]
fn adoption_refuses_missing_or_impossible_operation_history() {
    for corrupt in [
        "open_request",
        "empty",
        "missing_open",
        "gap",
        "second_open",
    ] {
        let mut store = store();
        store
            .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
            .unwrap();
        // Refused filings may refer to a foreign run. Metadata does not assert
        // that their content was admitted, and adoption must preserve that fact.
        store
            .record_observation(&StoredOperationObservation {
                request_source: Some(SourceBlockId {
                    run: "refused-foreign-run".into(),
                    sequence: 1.into(),
                    block: 0.into(),
                }),
                observation: observation(2, 2, StateOperation::File),
            })
            .unwrap();
        store
            .record_observation(&StoredOperationObservation {
                request_source: None,
                observation: observation(3, 3, StateOperation::Held),
            })
            .unwrap();
        let load = |store: &ServingSqlite| {
            let image = store.image().unwrap();
            let mut start = store.context.start.clone();
            start.dump_sha256 = super::super::image::sha256(&image);
            ServingSqlite::adopt(
                &image,
                &Common {
                    session_id: "next-session".into(),
                    run_id: "next-run".into(),
                    request_id: 1.into(),
                },
                "state-content/1",
                &store.context.scheme_id,
                &start,
                store.read_limits.clone(),
            )
        };
        assert!(
            load(&store).is_ok(),
            "valid history with a refused foreign source"
        );
        match corrupt {
            "open_request" => {
                let altered = observation(1, 2, StateOperation::Open);
                store
                    .connection
                    .execute(
                        "UPDATE operation_observation SET request_id=?1,
                    observation=?2 WHERE operation_id=?3",
                        params![
                            2_u64.to_be_bytes(),
                            serde_json::to_string(&altered).unwrap(),
                            1_u64.to_be_bytes()
                        ],
                    )
                    .unwrap();
            }
            "empty" => {
                store
                    .connection
                    .execute("DELETE FROM operation_observation", [])
                    .unwrap();
            }
            "missing_open" => {
                store
                    .connection
                    .execute(
                        "DELETE FROM operation_observation WHERE operation_id=?1",
                        [1_u64.to_be_bytes()],
                    )
                    .unwrap();
            }
            "gap" => {
                store
                    .connection
                    .execute(
                        "DELETE FROM operation_observation WHERE operation_id=?1",
                        [2_u64.to_be_bytes()],
                    )
                    .unwrap();
            }
            "second_open" => {
                let altered = observation(3, 3, StateOperation::Open);
                store
                    .connection
                    .execute(
                        "UPDATE operation_observation SET operation='open',
                    observation=?1 WHERE operation_id=?2",
                        params![
                            serde_json::to_string(&altered).unwrap(),
                            3_u64.to_be_bytes()
                        ],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            load(&store).err().expect(corrupt).code,
            StateRefusalCode::OpenFailed,
            "{corrupt}"
        );
    }
}

#[test]
fn current_observation_run_refuses_old_or_unknown_identity_before_inserting() {
    let mut store = store();
    let open = observation(1, 1, StateOperation::Open);
    for version in ["state-observation/1", "state-observation/3"] {
        let mut old = identity();
        old.instrument_version = version.into();
        assert!(store.begin_observations(&old, &open).is_err(), "{version}");
        let count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM observation_run", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    store.begin_observations(&identity(), &open).unwrap();
}

#[test]
fn observation_history_adoption_and_reads_preserve_v1_and_v2_and_refuse_unknown_versions() {
    for version in [
        "state-observation/1",
        "state-observation/2",
        "state-observation/3",
    ] {
        let mut original = store();
        original
            .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
            .unwrap();
        let mut historical = identity();
        historical.instrument_version = version.into();
        // Construct an image from each producer version. This test-only SQL is
        // not a migration: production adoption must preserve these exact bytes.
        let historical_json = serde_json::to_string(&historical).unwrap();
        original
            .connection
            .execute(
                "UPDATE observation_run SET member_observation=?1 WHERE run='run'",
                [&historical_json],
            )
            .unwrap();
        let selector = ReadSelector::Observations {
            run: "run".into(),
            after: 0.into(),
            through: None,
        };
        let queried = original.query(&selector);
        let image = original.image().unwrap();
        let mut start = original.context.start.clone();
        start.dump_sha256 = super::super::image::sha256(&image);
        let adopted = ServingSqlite::adopt(
            &image,
            &Common {
                session_id: "new-session".into(),
                run_id: "new-run".into(),
                request_id: 1.into(),
            },
            "state-content/1",
            &original.context.scheme_id,
            &start,
            original.read_limits.clone(),
        );
        if version == "state-observation/3" {
            assert!(
                queried.is_err(),
                "unknown historical version cannot be read"
            );
            assert_eq!(adopted.err().unwrap().code, StateRefusalCode::OpenFailed);
            continue;
        }
        let queried = queried.unwrap();
        let QueryResult::Observations {
            member_observation, ..
        } = &queried
        else {
            unreachable!()
        };
        assert_eq!(member_observation, &historical);
        let mut restored = adopted.unwrap();
        assert_eq!(
            serde_json::to_vec(&restored.query(&selector).unwrap()).unwrap(),
            serde_json::to_vec(&queried).unwrap()
        );
        let retained: String = restored
            .connection
            .query_row(
                "SELECT member_observation FROM observation_run WHERE run='run'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            retained, historical_json,
            "adoption changed recorded identity bytes"
        );
        restored
            .begin_observations(&identity(), &observation(1, 1, StateOperation::Open))
            .unwrap();
        let QueryResult::Observations {
            member_observation, ..
        } = restored
            .query(&ReadSelector::Observations {
                run: "new-run".into(),
                after: 0.into(),
                through: None,
            })
            .unwrap()
        else {
            unreachable!()
        };
        assert_eq!(member_observation.instrument_version, "state-observation/2");
        assert_eq!(
            serde_json::to_vec(&restored.query(&selector).unwrap()).unwrap(),
            serde_json::to_vec(&queried).unwrap()
        );
    }
}
