use super::*;

fn phases(sample: &mut OperationSample) {
    sample.phase_begin(PhaseName::Validate).unwrap();
    sample.phase_end(PhaseName::Validate).unwrap();
    sample.phase_begin(PhaseName::Encode).unwrap();
    sample.phase_end(PhaseName::Encode).unwrap();
}

#[test]
fn ordinary_operations_never_sample_kernel_resources_or_invent_missing_bytes() {
    for operation in [
        StateOperation::Open,
        StateOperation::File,
        StateOperation::FilePair,
        StateOperation::Read,
        StateOperation::Next,
        StateOperation::Release,
        StateOperation::Shape,
        StateOperation::Held,
        StateOperation::Position,
        StateOperation::Grants,
        StateOperation::Acknowledge,
        StateOperation::Abort,
        StateOperation::ReleaseCapture,
    ] {
        super::super::tests::timing::enable();
        let mut sample = OperationSample::begin(1.into(), 2.into(), operation).unwrap();
        phases(&mut sample);
        let observation = sample.finish(Some(25), None).unwrap();
        assert_eq!(super::super::tests::timing::resource_sample_count(), 0);
        super::super::tests::timing::disable();
        assert_eq!(observation.operation, operation);
        assert!(observation.resource.cpu_ns.is_none());
        assert_eq!(observation.resource.copied_bytes, Some(25.into()));
        assert!(observation.resource.held_bytes.is_none());
        assert_eq!(
            observation.resource.unavailable,
            vec![
                ResourceUnavailable {
                    field: ResourceField::CpuNs,
                    reason: ObservationUnavailable::NotSampled
                },
                ResourceUnavailable {
                    field: ResourceField::HeldBytes,
                    reason: ObservationUnavailable::Uninstrumented
                },
            ]
        );
        assert_eq!(
            observation.disk,
            OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(ObservationUnavailable::NotSampled)
            }
        );
    }
}

fn facts(cpu: u64, counter: u64) -> SnapshotFacts {
    SnapshotFacts {
        cpu: Ok(cpu),
        enabled: Ok(false),
        disk: Ok(DiskSample {
            major_faults: counter.into(),
            read_bytes: counter.into(),
            write_bytes: counter.into(),
            blkio_delay_ticks: counter.into(),
            ticks_per_second: 100.into(),
        }),
    }
}

struct KnownKernel {
    calls: u64,
}
impl Metrics for KnownKernel {
    fn snapshot(&mut self) -> SnapshotFacts {
        self.calls += 1;
        facts(10 * self.calls, self.calls)
    }
}

#[test]
fn snapshot_samples_before_and_after_but_disabled_accounting_stays_disabled() {
    let mut kernel = KnownKernel { calls: 0 };
    let mut sample = OperationSample::begin(1.into(), 1.into(), StateOperation::Snapshot).unwrap();
    assert_eq!(kernel.calls, 0);
    sample.capture_begin_with(&mut kernel).unwrap();
    assert_eq!(kernel.calls, 1);
    sample.capture_end_with(&mut kernel).unwrap();
    assert_eq!(kernel.calls, 2);
    phases(&mut sample);
    let result = sample.finish(Some(4), Some(8)).unwrap();
    assert_eq!(kernel.calls, 2);
    assert_eq!(result.resource.cpu_ns, Some(10.into()));
    assert_eq!(result.disk.enabled, Some(false));
    assert_eq!(result.disk.before.unwrap().read_bytes.0, 1);
    assert_eq!(result.disk.after.unwrap().read_bytes.0, 2);
    assert!(result.disk.unavailable_reason.is_none());
}

#[test]
fn reset_or_unreadable_snapshot_counters_are_unavailable_instead_of_zero() {
    let (cpu, disk) = snapshot_delta(facts(20, 20), facts(10, 10));
    assert_eq!(cpu, Err(ObservationUnavailable::CounterReset));
    assert!(disk.before.is_none() && disk.after.is_none());
    assert_eq!(
        disk.unavailable_reason,
        Some(ObservationUnavailable::CounterReset)
    );
    let mut before = facts(10, 1);
    before.enabled = Err(ObservationUnavailable::Denied);
    let (_, disk) = snapshot_delta(before, facts(20, 2));
    assert!(disk.enabled.is_none());
    assert!(disk.before.is_some() && disk.after.is_some());
    assert_eq!(
        disk.unavailable_reason,
        Some(ObservationUnavailable::Denied)
    );
    disk.validate().unwrap();
    let mut after = facts(20, 2);
    after.disk = Err(ObservationUnavailable::ReadFailed);
    let (_, disk) = snapshot_delta(facts(10, 1), after);
    assert!(disk.before.is_none() && disk.after.is_none());
    assert_eq!(
        disk.unavailable_reason,
        Some(ObservationUnavailable::ReadFailed)
    );
}

#[test]
fn phase_lifecycle_refuses_duplicates_unclosed_and_outside_intervals() {
    let mut sample = OperationSample::begin(1.into(), 1.into(), StateOperation::File).unwrap();
    assert!(sample.phase_end(PhaseName::Query).is_err());
    sample.phase_begin(PhaseName::Validate).unwrap();
    assert!(sample.phase_begin(PhaseName::Validate).is_err());
    sample.phase_end(PhaseName::Validate).unwrap();
    assert!(sample.phase_end(PhaseName::Validate).is_err());
    assert!(
        sample
            .phase_record(OperationPhase {
                name: PhaseName::Transaction,
                began_ns: 0.into(),
                ended_ns: 0.into()
            })
            .is_err()
    );
    sample.phase_begin(PhaseName::Encode).unwrap();
    assert!(sample.finish(None, None).is_err());

    let mut sample = OperationSample::begin(1.into(), 1.into(), StateOperation::File).unwrap();
    let start = monotonic_ns().unwrap();
    let end = monotonic_ns().unwrap();
    sample
        .phase_record(OperationPhase {
            name: PhaseName::Transaction,
            began_ns: start,
            ended_ns: end,
        })
        .unwrap();
    phases(&mut sample);
    let result = sample.finish(None, None).unwrap();
    assert_eq!(
        result
            .phases
            .iter()
            .find(|phase| phase.name == PhaseName::Transaction)
            .unwrap()
            .began_ns,
        start
    );
    result.validate_shape().unwrap();
}

#[test]
fn proc_fields_handle_spaces_and_parentheses_and_refuse_incomplete_values() {
    let fields: Vec<_> = (3..=42)
        .map(|field| {
            if field == 3 {
                "S".to_string()
            } else {
                field.to_string()
            }
        })
        .collect();
    let stat = format!("123 (name with ) a closing paren) {}", fields.join(" "));
    assert_eq!(stat_counter(&stat, 22), Ok(22));
    assert_eq!(stat_counter(&stat, 42), Ok(42));
    assert!(stat_counter("123 (short) S", 22).is_err());
    assert!(stat_counter(&stat, 2).is_err());
    assert_eq!(
        io_counter("read_bytes: 10\nwrite_bytes: 20\n", "read_bytes"),
        Ok(10)
    );
    assert!(io_counter("read_bytes: 10\nread_bytes: 20\n", "read_bytes").is_err());
    assert_eq!(
        decimal("18446744073709551616"),
        Err(ObservationUnavailable::Overflow)
    );
    assert!(decimal("-1").is_err());
    sample_member_identity().validate_current_open().unwrap();
}
