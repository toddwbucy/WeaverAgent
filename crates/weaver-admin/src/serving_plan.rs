//! Root plan-update producer, held for the coupled serving command cutover.
//! Local judgment precedes dialing. A missing answer never authorizes a replay.
use crate::channel::{ChannelFault, Coordination};
use crate::{ServiceConfig, start, verb_table};
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, Instant};
use weaver_types::state_flow::enter::{LoadedStateFacts, ObservationAnswer};
use weaver_types::state_flow::{Digest, PlanUpdateCase, PlanUpdateDocument, decode_strict};
use weaver_types::{
    AgentName, AgentState, Cause, ExchangeId, FieldName, LifecycleAnswer, LifecycleDirective,
    LifecycleRefusal, MAX_ENVELOPE_BYTES, Opener, OrganEnvelope, Payload, Position,
};

/// The update is the sole exchange on a fresh connection. The worker closes
/// Observe's connection after answering it. Fix this actual envelope before
/// either dial so the whole escaped size is decidable before side effects.
const UPDATE_ORDINAL: u64 = 1;

struct Prepared {
    request: OrganEnvelope,
    wire: Vec<u8>,
}

pub(crate) fn update(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    update_with(config, agent, || crate::dial_worker(config))
}

// The process instrument substitutes only account/path lookup. Its dial still
// authenticates the actual worker UID, pidfd and held run lock.
fn update_with(
    config: &ServiceConfig,
    agent: &AgentName,
    dial: impl FnMut() -> Result<Coordination, ChannelFault>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let cause = authorize(
        nix::unistd::geteuid().is_root(),
        crate::cause_from(std::env::var_os("SUDO_UID").as_deref()),
    )?;
    crate::admissible(config, agent)?;
    let directory = crate::prepare_run_directory(config, nix::unistd::geteuid().as_raw())?;
    let _invocation = start::take_invocation_lock(&directory, verb_table::HolderVerb::PlanUpdate)?;
    if !start::run_lock_held(&directory)? {
        admit_view(verb_table::AdminView::Down)?;
    }
    crate::judge_root(&config.root, 0)?;
    let root = crate::judge_ancestors(&config.root, &[0])?;
    let root = crate::save_points::open_directory(&root).map_err(|_| unsafe_file())?;
    let prepared = prepare(root.as_fd(), cause)?;
    exchange(prepared, dial)
}

fn authorize(
    root: bool,
    cause: Result<Cause, LifecycleRefusal>,
) -> Result<Cause, LifecycleRefusal> {
    if !root {
        return Err(LifecycleRefusal::Unauthorized);
    }
    let cause = cause?;
    if cause.uid != 0 {
        return Err(LifecycleRefusal::Unauthorized);
    }
    Ok(cause)
}

fn admit_view(view: verb_table::AdminView) -> Result<(), LifecycleRefusal> {
    use verb_table::VerbAction as A;
    match verb_table::act(verb_table::Verb::PlanUpdate, view) {
        A::DirectPlanUpdate => Ok(()),
        A::RefuseInvocationInFlight => Err(LifecycleRefusal::InvocationInFlight),
        A::RefuseUnanswered => Err(LifecycleRefusal::Unanswered),
        A::RefuseOutOfOrder => Err(LifecycleRefusal::OutOfOrder),
        _ => unreachable!("plan verb table grants only root update admission"),
    }
}

fn unsafe_file() -> LifecycleRefusal {
    LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName("plan-update".into())),
    }
}

fn malformed(detail: impl Into<String>) -> LifecycleRefusal {
    LifecycleRefusal::PlanUpdateRefused {
        case: PlanUpdateCase::MalformedPlan,
        detail: detail.into(),
        rejected_blocks: Vec::new(),
        read_code: None,
    }
}

fn prepare(root: BorrowedFd<'_>, cause: Cause) -> Result<Prepared, LifecycleRefusal> {
    use nix::fcntl::{OFlag, openat};
    let descriptor = openat(
        root,
        "plan-update.json",
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|_| unsafe_file())?;
    let file = File::from(descriptor);
    let stat = file.metadata().map_err(|_| unsafe_file())?;
    if !stat.is_file() || stat.uid() != 0 || stat.mode() & 0o022 != 0 {
        return Err(unsafe_file());
    }
    // Reading one sentinel byte distinguishes an oversized file from a prefix.
    let mut bytes = Vec::new();
    file.take(MAX_ENVELOPE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unsafe_file())?;
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(malformed("plan update exceeds the coordination envelope"));
    }
    prepare_bytes(bytes, cause)
}

fn prepare_bytes(bytes: Vec<u8>, cause: Cause) -> Result<Prepared, LifecycleRefusal> {
    let document = String::from_utf8(bytes).map_err(|_| malformed("plan update is not UTF-8"))?;
    let parsed: PlanUpdateDocument = decode_strict(&document)
        .map_err(|_| malformed("plan update is not a strict state-plan-update/1 document"))?;
    parsed
        .validate_shape()
        .map_err(|_| malformed("plan update has an invalid operation shape"))?;
    let document_sha256 = Digest::new(format!("{:x}", Sha256::digest(document.as_bytes())))
        .expect("SHA-256 renders a lowercase digest");
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: UPDATE_ORDINAL,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::PlanUpdate {
            cause,
            document_sha256,
            document,
        }),
    };
    let wire = serde_json::to_vec(&request).map_err(|_| malformed("plan update cannot encode"))?;
    if wire.len() > MAX_ENVELOPE_BYTES {
        return Err(malformed("plan update exceeds the coordination envelope"));
    }
    Ok(Prepared { request, wire })
}

fn exchange(
    prepared: Prepared,
    mut dial: impl FnMut() -> Result<Coordination, ChannelFault>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let mut observed_peer = dial().map_err(channel_refusal)?;
    let observation =
        crate::serving_observe::ask(&mut observed_peer).map_err(|failure| match failure {
            crate::serving_observe::Failure::Refused(refusal) => refusal,
            crate::serving_observe::Failure::Channel(fault) => channel_refusal(fault),
        })?;
    let ObservationAnswer::State {
        state,
        load: Some(facts),
        ..
    } = observation
    else {
        return Err(LifecycleRefusal::OutOfOrder);
    };
    let LoadedStateFacts::Serving {
        state_management,
        start,
        ..
    } = &facts.state
    else {
        return Err(LifecycleRefusal::OutOfOrder);
    };
    let seen = match state {
        AgentState::Idle => verb_table::WorkerSeen::Idle,
        AgentState::Active => verb_table::WorkerSeen::Active,
        AgentState::Absent | AgentState::Unloaded => verb_table::WorkerSeen::Unloaded,
    };
    admit_view(verb_table::class_of(verb_table::Seen::RunLockHeld(seen)))?;
    if state == AgentState::Active {
        return Err(LifecycleRefusal::ActivityNotAtRest);
    }
    let wait = &state_management.ste100.decision_wait;
    wait.validate().map_err(|_| LifecycleRefusal::Malformed)?;
    let mut channel = dial().map_err(channel_refusal)?;
    channel
        .require_same_worker(&observed_peer)
        .map_err(channel_refusal)?;
    drop(observed_peer);
    if channel.next_ordinal() != prepared.request.exchange.ordinal {
        return Err(LifecycleRefusal::Malformed);
    }
    let end = Instant::now()
        .checked_add(Duration::from_nanos(wait.elapsed_ns.0))
        .ok_or(LifecycleRefusal::Malformed)?;
    channel
        .send_plan_until(&prepared.wire, end)
        .map_err(channel_refusal)?;
    let response = channel.recv_plan_until(end).map_err(channel_refusal)?;
    if response.exchange != prepared.request.exchange || response.position != Position::Close {
        return Err(LifecycleRefusal::Malformed);
    }
    match response.payload {
        Payload::Answer(answer @ LifecycleAnswer::PlanUpdated { .. }) => {
            let LifecycleAnswer::PlanUpdated { plan, source } = &answer else {
                unreachable!()
            };
            if plan.plan_id.seed_id != start.seed_id
                || source.block_id.run != facts.run.0
                || source.turn.is_some()
            {
                return Err(LifecycleRefusal::Malformed);
            }
            Ok(answer)
        }
        Payload::Refusal(refusal) => Err(refusal),
        _ => Err(LifecycleRefusal::Malformed),
    }
}

fn channel_refusal(fault: ChannelFault) -> LifecycleRefusal {
    match fault {
        ChannelFault::Closed | ChannelFault::Unanswered => LifecycleRefusal::Unanswered,
        ChannelFault::NoListener | ChannelFault::NotDialable => LifecycleRefusal::OutOfOrder,
        ChannelFault::WrongPeer { .. } => LifecycleRefusal::BoundaryUnverified,
        ChannelFault::Truncated { .. } | ChannelFault::Undecodable => LifecycleRefusal::Malformed,
    }
}

#[cfg(test)]
#[path = "serving_plan_tests.rs"]
mod tests;
