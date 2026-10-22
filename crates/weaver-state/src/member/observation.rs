//! Operation intervals and snapshot-only kernel samples under Spec 09.
//! Ordinary operations read the monotonic clock and local byte counters only.

use std::io;

use weaver_types::state_flow::{
    DecimalU64, DiskSample, MemberObservationIdentity, ObservationUnavailable, OperationDisk,
    OperationPhase, OperationResource, PhaseName, ResourceField, ResourceUnavailable,
    StateOperation, StateOperationObservation,
};

type OptionalSample<T> = Result<T, ObservationUnavailable>;

pub(super) use weaver_state::serving_clock::monotonic_ns;

pub(super) fn sample_member_identity() -> MemberObservationIdentity {
    let facts = (|| {
        let stat = read("/proc/self/stat")?;
        let start = stat_counter(&stat, 22)?;
        Ok((start, ticks_per_second()?))
    })();
    let (start_ticks, ticks_per_second, unavailable_reason) = match facts {
        Ok((start, rate)) => (Some(start.into()), Some(rate.into()), None),
        Err(reason) => (None, None, Some(reason)),
    };
    MemberObservationIdentity {
        instrument_version: "state-observation/2".into(),
        pid: std::process::id(),
        start_ticks,
        ticks_per_second,
        unavailable_reason,
    }
}

pub(super) struct SnapshotFacts {
    pub(super) cpu: OptionalSample<u64>,
    pub(super) enabled: OptionalSample<bool>,
    pub(super) disk: OptionalSample<DiskSample>,
}

trait Metrics {
    fn snapshot(&mut self) -> SnapshotFacts;
}

struct Kernel;

impl Metrics for Kernel {
    fn snapshot(&mut self) -> SnapshotFacts {
        #[cfg(test)]
        if let Some(facts) = super::tests::timing::sample_resources() {
            return facts;
        }
        SnapshotFacts {
            cpu: weaver_state::serving_clock::thread_cpu_ns().map_err(unavailable),
            enabled: read("/proc/sys/kernel/task_delayacct").and_then(|text| match text.trim() {
                "0" => Ok(false),
                "1" => Ok(true),
                _ => Err(ObservationUnavailable::ReadFailed),
            }),
            disk: sample_disk(),
        }
    }
}

pub(super) struct OperationSample {
    operation_id: DecimalU64,
    request_id: DecimalU64,
    operation: StateOperation,
    began_ns: DecimalU64,
    phases: Vec<(PhaseName, DecimalU64, Option<DecimalU64>)>,
    resources: ResourceSamples,
}

enum ResourceSamples {
    NotSampled,
    Capturing(SnapshotFacts),
    Captured(OptionalSample<u64>, OperationDisk),
}

impl OperationSample {
    #[cfg(test)]
    pub(super) fn begin(
        operation_id: DecimalU64,
        request_id: DecimalU64,
        operation: StateOperation,
    ) -> io::Result<Self> {
        Self::begin_at(operation_id, request_id, operation, monotonic_ns()?)
    }

    /// Preserve the timestamp taken before any operation hint parsing. Kernel
    /// counters are sampled only if a validated snapshot passes preflight.
    pub(super) fn begin_at(
        operation_id: DecimalU64,
        request_id: DecimalU64,
        operation: StateOperation,
        began_ns: DecimalU64,
    ) -> io::Result<Self> {
        if operation_id.0 == 0 || request_id.0 == 0 {
            return Err(invalid("zero operation or request ID"));
        }
        let mut phases = Vec::new();
        phases.try_reserve_exact(6).map_err(io::Error::other)?;
        Ok(Self {
            operation_id,
            request_id,
            operation,
            began_ns,
            phases,
            resources: ResourceSamples::NotSampled,
        })
    }

    pub(super) fn capture_begin(&mut self) -> io::Result<()> {
        self.capture_begin_with(&mut Kernel)
    }

    fn capture_begin_with(&mut self, metrics: &mut impl Metrics) -> io::Result<()> {
        if self.operation != StateOperation::Snapshot
            || !matches!(self.resources, ResourceSamples::NotSampled)
        {
            return Err(invalid("resource samples require one snapshot capture"));
        }
        self.resources = ResourceSamples::Capturing(metrics.snapshot());
        Ok(())
    }

    pub(super) fn capture_end(&mut self) -> io::Result<()> {
        self.capture_end_with(&mut Kernel)
    }

    fn capture_end_with(&mut self, metrics: &mut impl Metrics) -> io::Result<()> {
        let ResourceSamples::Capturing(before) =
            std::mem::replace(&mut self.resources, ResourceSamples::NotSampled)
        else {
            return Err(invalid("snapshot capture has no before sample"));
        };
        let (cpu, disk) = snapshot_delta(before, metrics.snapshot());
        self.resources = ResourceSamples::Captured(cpu, disk);
        Ok(())
    }

    pub(super) fn phase_begin(&mut self, name: PhaseName) -> io::Result<()> {
        if self.phases.iter().any(|(prior, _, _)| *prior == name) {
            return Err(invalid("operation phase repeated"));
        }
        self.phases.push((name, monotonic_ns()?, None));
        Ok(())
    }

    /// Import an interval measured inside the primitive that owns that work.
    pub(super) fn phase_record(&mut self, phase: OperationPhase) -> io::Result<()> {
        if self.phases.iter().any(|(name, _, _)| *name == phase.name)
            || phase.began_ns < self.began_ns
            || phase.began_ns > phase.ended_ns
        {
            return Err(invalid("invalid or repeated imported operation phase"));
        }
        self.phases
            .push((phase.name, phase.began_ns, Some(phase.ended_ns)));
        Ok(())
    }

    pub(super) fn phase_end(&mut self, name: PhaseName) -> io::Result<()> {
        let phase = self
            .phases
            .iter_mut()
            .find(|(prior, _, _)| *prior == name)
            .ok_or_else(|| invalid("operation phase was not begun"))?;
        if phase.2.is_some() {
            return Err(invalid("operation phase ended twice"));
        }
        phase.2 = Some(monotonic_ns()?);
        Ok(())
    }

    /// Call after the operation payload has been encoded. Metadata insertion
    /// and wrapper encoding happen later, outside this operation interval.
    pub(super) fn finish(
        self,
        copied_bytes: Option<u64>,
        held_bytes: Option<u64>,
    ) -> io::Result<StateOperationObservation> {
        let ended_ns = monotonic_ns()?;
        #[cfg(test)]
        super::tests::timing::observation_end(ended_ns);
        let (cpu, disk) = match self.resources {
            ResourceSamples::Captured(cpu, disk) => (cpu, disk),
            ResourceSamples::Capturing(_) => {
                return Err(invalid("snapshot capture has no after sample"));
            }
            ResourceSamples::NotSampled => (
                Err(ObservationUnavailable::NotSampled),
                OperationDisk {
                    enabled: None,
                    before: None,
                    after: None,
                    unavailable_reason: Some(ObservationUnavailable::NotSampled),
                },
            ),
        };
        let mut missing = Vec::new();
        let cpu_ns = match cpu {
            Ok(value) => Some(value.into()),
            Err(reason) => {
                missing.push(ResourceUnavailable {
                    field: ResourceField::CpuNs,
                    reason,
                });
                None
            }
        };
        if copied_bytes.is_none() {
            missing.push(ResourceUnavailable {
                field: ResourceField::CopiedBytes,
                reason: ObservationUnavailable::Uninstrumented,
            });
        }
        if held_bytes.is_none() {
            missing.push(ResourceUnavailable {
                field: ResourceField::HeldBytes,
                reason: ObservationUnavailable::Uninstrumented,
            });
        }
        let mut phases = self
            .phases
            .into_iter()
            .map(|(name, began_ns, ended_ns)| {
                Ok(OperationPhase {
                    name,
                    began_ns,
                    ended_ns: ended_ns.ok_or_else(|| invalid("operation phase still open"))?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        phases.sort_unstable_by_key(|phase| (phase.began_ns, phase.name.as_str()));
        let result = StateOperationObservation {
            operation_id: self.operation_id,
            parent_id: None,
            request_id: self.request_id,
            operation: self.operation,
            began_ns: self.began_ns,
            ended_ns,
            phases,
            resource: OperationResource {
                cpu_ns,
                copied_bytes: copied_bytes.map(Into::into),
                held_bytes: held_bytes.map(Into::into),
                unavailable: missing,
            },
            disk,
        };
        result.validate_shape().map_err(invalid)?;
        Ok(result)
    }
}

fn snapshot_delta(
    before: SnapshotFacts,
    after: SnapshotFacts,
) -> (OptionalSample<u64>, OperationDisk) {
    let cpu = match (before.cpu, after.cpu) {
        (Ok(start), Ok(end)) => end
            .checked_sub(start)
            .ok_or(ObservationUnavailable::CounterReset),
        (Err(reason), _) | (_, Err(reason)) => Err(reason),
    };
    let enabled = before.enabled.ok();
    let unavailable_enabled = before.enabled.err();
    let pair = match (before.disk, after.disk) {
        (Ok(start), Ok(end))
            if start.ticks_per_second == end.ticks_per_second
                && start.major_faults <= end.major_faults
                && start.read_bytes <= end.read_bytes
                && start.write_bytes <= end.write_bytes
                && start.blkio_delay_ticks <= end.blkio_delay_ticks =>
        {
            Ok((start, end))
        }
        (Ok(_), Ok(_)) => Err(ObservationUnavailable::CounterReset),
        (Err(reason), _) | (_, Err(reason)) => Err(reason),
    };
    let disk = match pair {
        Ok((start, end)) => OperationDisk {
            enabled,
            before: Some(start),
            after: Some(end),
            unavailable_reason: unavailable_enabled,
        },
        Err(reason) => OperationDisk {
            enabled,
            before: None,
            after: None,
            unavailable_reason: Some(reason),
        },
    };
    (cpu, disk)
}

fn sample_disk() -> OptionalSample<DiskSample> {
    let mut usage = std::mem::MaybeUninit::<nix::libc::rusage>::uninit();
    // SAFETY: the destination is valid and read only after getrusage succeeds.
    if unsafe { nix::libc::getrusage(nix::libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return Err(unavailable(io::Error::last_os_error()));
    }
    // SAFETY: success initializes every field of rusage.
    let usage = unsafe { usage.assume_init() };
    let major_faults =
        u64::try_from(usage.ru_majflt).map_err(|_| ObservationUnavailable::Overflow)?;
    let stat = read("/proc/self/stat")?;
    let io = read("/proc/self/io")?;
    Ok(DiskSample {
        major_faults: major_faults.into(),
        read_bytes: io_counter(&io, "read_bytes")?.into(),
        write_bytes: io_counter(&io, "write_bytes")?.into(),
        blkio_delay_ticks: stat_counter(&stat, 42)?.into(),
        ticks_per_second: ticks_per_second()?.into(),
    })
}

fn ticks_per_second() -> OptionalSample<u64> {
    // SAFETY: this sysconf key takes no pointer and changes no process state.
    let value = unsafe { nix::libc::sysconf(nix::libc::_SC_CLK_TCK) };
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ObservationUnavailable::ReadFailed)
}

fn read(path: &str) -> OptionalSample<String> {
    std::fs::read_to_string(path).map_err(unavailable)
}

fn unavailable(error: io::Error) -> ObservationUnavailable {
    match error.kind() {
        io::ErrorKind::PermissionDenied => ObservationUnavailable::Denied,
        io::ErrorKind::NotFound | io::ErrorKind::Unsupported => ObservationUnavailable::Unsupported,
        _ => ObservationUnavailable::ReadFailed,
    }
}

fn stat_counter(text: &str, field: usize) -> OptionalSample<u64> {
    // comm may itself contain whitespace and ')'. The final ')' ends it.
    let tail = text
        .rsplit_once(')')
        .ok_or(ObservationUnavailable::ReadFailed)?
        .1;
    let index = field
        .checked_sub(3)
        .ok_or(ObservationUnavailable::ReadFailed)?;
    decimal(
        tail.split_whitespace()
            .nth(index)
            .ok_or(ObservationUnavailable::ReadFailed)?,
    )
}

fn io_counter(text: &str, name: &str) -> OptionalSample<u64> {
    let mut matches = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| *key == name);
    let value = matches
        .next()
        .ok_or(ObservationUnavailable::ReadFailed)?
        .1
        .trim();
    if matches.next().is_some() {
        return Err(ObservationUnavailable::ReadFailed);
    }
    decimal(value)
}

fn decimal(text: &str) -> OptionalSample<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ObservationUnavailable::ReadFailed);
    }
    text.parse().map_err(|_| ObservationUnavailable::Overflow)
}

fn invalid(detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail.to_string())
}

#[cfg(test)]
mod tests;
