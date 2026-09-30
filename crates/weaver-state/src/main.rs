//! conforms: state-serve-restricts-to-the-session
//! conforms: state-preload-door-stands-only-diagnostic
//! conforms: state-replay-answers-at-the-seal
//! conforms: state-preload-door-refuses-the-agent
//!
//! The member's process: take the first door's end with the process, read
//! the election, stand the store, and land distillates until the channel
//! closes. Per `weaver-harness-state-contract` as ruled 2026-08-26, the
//! seam is a connected socketpair with no name: admin creates it at this
//! process's spawn, this member inherits its end at the fixed number
//! below, and possession authenticates, so no bind, no credential, and no
//! wait exist on this door. The arguments are the territory and, where the
//! party that stands this member names one, the preload socket.
//!
//! **The second door stands where that second argument does**, per
//! `weaver-state-Spec` section 4, and it carries this member's one
//! credential judgment: admit the operator principal, refuse every other
//! peer. Both doors are served from one loop by `poll`, so the store keeps
//! one owner and a distillate lands the same way whichever door carried
//! it, which is the mechanism of the contract's indistinguishability
//! claim.

use std::io::Read;

use weaver_state::{
    Ask, Election, Store, parse_ask, parse_distillate, render_grants_answer,
    render_identity_answer, render_recall_answer, render_replay_answer, render_shape_answer,
};

/// The first door's end arrives at this descriptor number, the fixed
/// convention between this member and admin that `weaver-state-Spec`
/// section 2 leaves to this act: the number after the three standard
/// streams, armed by admin's spawn path and inherited with the process.
const FIRST_DOOR_FD: std::os::fd::RawFd = 3;

/// The bound on one answer frame, matched by the harness's own cap on
/// what it reads: an answer past this size is a fault answered with
/// silence, per the contract's clause that custody never invents an
/// answer shape for a fault.
const ANSWER_BOUND: usize = 1024 * 1024;

/// The bound on the answer's write: a peer that takes nothing for this
/// long has stopped reading, and a custodian wedged on its behalf would
/// cost the session its custody, so the seam retires instead, holdings
/// standing.
const RESPOND_WAIT_MS: u16 = 2_000;

fn main() -> std::process::ExitCode {
    member_entry(std::env::args().skip(1), FIRST_DOOR_FD)
}

fn member_entry(
    arguments: impl Iterator<Item = String>,
    first_door: std::os::fd::RawFd,
) -> std::process::ExitCode {
    // **The preload name is a second argument and its absence is a serving
    // load**, per `weaver-state-Spec` section 4: the name reaches the
    // member on the vector because no exchange this member holds carries a
    // path. The first door rides no argument at all, its end inherited at
    // the fixed number above.
    // **The store election rides the vector too**, per `weaver-state-Spec`
    // section 4 as of 2026-09-04: `--engine` names the port, and under the
    // service engine `--store-socket`, `--database`, and `--role` name where
    // and as whom the member connects. Absent flags mean the embedded
    // engine, which is what an absent election means in the declaration.
    let Some(vector) = StoreVector::parse(arguments) else {
        eprintln!(
            "{}",
            serde_json::json!({
                "state_fault":
                    "usage: weaver-state [--engine sqlite|postgres] [--store-socket DIR] \
                     [--database NAME] [--role NAME] <territory> [preload-socket]"
            })
        );
        return std::process::ExitCode::FAILURE;
    };
    let StoreVector {
        engine,
        socket,
        database,
        role,
        territory,
        preload_socket,
    } = vector;

    // The inherited end is this process's by construction: admin armed it
    // onto the fixed number in the spawn path itself. **The number is probed
    // before it is adopted**, because a hand-run process holds whatever its
    // shell left at that number, or nothing: adopting a stranger's
    // descriptor would read it as seam traffic and close it on exit, and
    // adopting a closed number would alias whatever the store opens next.
    // The probe borrows and owns nothing, so a refusal closes nothing that
    // is not this process's to close.
    {
        // SAFETY: the borrow reads one socket option and adopts nothing.
        let probe = unsafe { std::os::fd::BorrowedFd::borrow_raw(first_door) };
        match nix::sys::socket::getsockopt(&probe, nix::sys::socket::sockopt::SockType) {
            Ok(nix::sys::socket::SockType::Stream) => {}
            _ => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "state_fault":
                            "the first door's number holds no stream socket: not started by admin"
                    })
                );
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    // SAFETY: the fixed number is the spawn convention's, armed by the one
    // party that starts this process, probed above, and adopted exactly
    // once.
    let mut channel = unsafe {
        use std::os::fd::FromRawFd;
        std::os::unix::net::UnixStream::from_raw_fd(first_door)
    };

    let mut store: Box<dyn Store> = match open_store(&engine, &territory, socket, database, role) {
        Ok(store) => store,
        Err(fault) => {
            eprintln!(
                "{}",
                serde_json::json!({"state_fault": format!("{fault:?}")})
            );
            return std::process::ExitCode::FAILURE;
        }
    };

    // The election opens the flow, per the contract: the first line is the
    // opener, and the indexes stand before the first distillate.
    let mut lines = LineReader::new(&mut channel);
    let Some(opener) = lines.next_line() else {
        // A channel closed before its opener is a load that did not finish
        // standing, and an empty stand is the honest outcome.
        return std::process::ExitCode::SUCCESS;
    };
    // **The session the opener declared bounds every answer.** Held for the
    // channel's life beside the election, per the contract's amended term.
    // An opener that names none leaves it empty, which matches no row, so a
    // custodian that could not learn its session answers nothing rather than
    // answering across every session the file holds - the defect this
    // repairs, where unbounded reads looked perfectly well formed.
    let session = parse_session(&opener).unwrap_or_default();
    let Some(election) = parse_election(&opener) else {
        eprintln!(
            "{}",
            serde_json::json!({"state_fault": "malformed election in first-door opener"})
        );
        return std::process::ExitCode::FAILURE;
    };
    if let Err(fault) = store.index_election(&election) {
        eprintln!(
            "{}",
            serde_json::json!({"state_fault": format!("{fault:?}")})
        );
        return std::process::ExitCode::FAILURE;
    }

    // Both doors are poll-driven from here, so the first door stops blocking.
    if lines.stream.set_nonblocking(true).is_err() {
        eprintln!(
            "{}",
            serde_json::json!({"state_fault": "channel would not go non-blocking"})
        );
        return std::process::ExitCode::FAILURE;
    }

    // The preload name stands only where the party that stands this member
    // named one, and that party names it only under a diagnostic binding,
    // per `weaver-state-Spec` section 4. Absence is a serving load, not a
    // fault: the door does not exist and a driver finds nothing to dial.
    let preload = match preload_socket.as_deref().map(stand_preload_name) {
        None => None,
        Some(Some(listener)) => Some(listener),
        Some(None) => return std::process::ExitCode::FAILURE,
    };

    serve(lines, preload, preload_socket, store.as_mut(), &session)
}

/// The member's vector, parsed: the flags in any order before the two
/// positionals the vector has always carried.
struct StoreVector {
    engine: String,
    socket: Option<String>,
    database: Option<String>,
    role: Option<String>,
    territory: String,
    preload_socket: Option<String>,
}

impl StoreVector {
    fn parse(arguments: impl Iterator<Item = String>) -> Option<StoreVector> {
        let mut engine = String::from("sqlite");
        let mut socket = None;
        let mut database = None;
        let mut role = None;
        let mut positional = Vec::new();
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--engine" => engine = arguments.next()?,
                "--store-socket" => socket = Some(arguments.next()?),
                "--database" => database = Some(arguments.next()?),
                "--role" => role = Some(arguments.next()?),
                flag if flag.starts_with("--") => return None,
                _ => positional.push(argument),
            }
        }
        if positional.is_empty() || positional.len() > 2 {
            return None;
        }
        let mut positional = positional.into_iter();
        Some(StoreVector {
            engine,
            socket,
            database,
            role,
            territory: positional.next()?,
            preload_socket: positional.next(),
        })
    }
}

/// The engine the vector elects, opened, per `weaver-state-Spec` section 3:
/// the embedded engine at the territory's `state.sql`, the service engine
/// over its socket under the declared database and role. An engine this
/// binary was not built with, or one the vector spells that the port does
/// not name, is `StoreUnavailable`: the member cannot stand on a store it
/// does not carry, and admin's inventory should have refused the load
/// before the vector was built.
fn open_store(
    engine: &str,
    territory: &str,
    socket: Option<String>,
    database: Option<String>,
    role: Option<String>,
) -> Result<Box<dyn Store>, weaver_state::CustodyFault> {
    use weaver_state::CustodyFault;
    match engine {
        #[cfg(feature = "sqlite")]
        "sqlite" => {
            let path = std::path::Path::new(territory).join("state.sql");
            Ok(Box::new(weaver_state::engine::sqlite::Sqlite::open(&path)?))
        }
        #[cfg(feature = "postgres")]
        "postgres" => {
            let socket = socket.unwrap_or_else(|| String::from("/run/postgresql"));
            let (Some(database), Some(role)) = (database, role) else {
                return Err(CustodyFault::StoreUnavailable(String::from(
                    "the service engine needs --database and --role",
                )));
            };
            Ok(Box::new(weaver_state::engine::postgres::Postgres::open(
                &socket, &database, &role,
            )?))
        }
        other => {
            let _ = (territory, socket, database, role);
            Err(CustodyFault::StoreUnavailable(format!(
                "no engine named {other:?} in this binary"
            )))
        }
    }
}

/// Custody until closure, across the doors this standing carries.
///
/// **One store, one path, one thread.** A distillate arriving on the preload
/// channel lands exactly as one arriving from the tee, which is the mechanism
/// of the contract's indistinguishability claim, and the serve loop reaches
/// both doors by `poll` rather than by a second thread so the store keeps one
/// owner and the landing order is the arrival order.
fn serve(
    mut harness: LineReader<'_>,
    preload_listener: Option<std::os::unix::net::UnixListener>,
    preload_path: Option<String>,
    store: &mut dyn Store,
    session: &str,
) -> std::process::ExitCode {
    use std::os::fd::AsFd;

    // The seal and the parked slot, extracted so the parking law is a unit
    // the suite watches, per `state-replay-answers-at-the-seal`.
    let mut parking = ReplayParking::new(preload_listener.is_some());
    // The door stands until a peer is admitted, then the channel stands in
    // its place, and the door stands again when that channel closes: the
    // contract's retry clause reads within a standing, a dead driver's
    // sealless prefix retired by the next opener, so one admitted peer per
    // standing would leave the retry nowhere to arrive.
    let mut listener = preload_listener;
    let mut preload: Option<std::os::unix::net::UnixStream> = None;
    let mut entry_drained = false;
    let mut preload_frames: Vec<u8> = Vec::new();
    let mut preload_opened = false;

    loop {
        // **Frames coalesced into the opener's read drain before the first
        // poll**, because the blocking opener read may have buffered whole
        // lines the socket will never signal again: a shape ask arriving
        // right behind the opener, which is the documented cadence at a
        // run's opening, would otherwise stall until the next traffic.
        if !entry_drained {
            entry_drained = true;
            if drain_harness_lines(&mut harness, store, session, &mut parking).is_some() {
                return std::process::ExitCode::SUCCESS;
            }
        }
        let mut fds = Vec::with_capacity(2);
        fds.push(nix::poll::PollFd::new(
            harness.stream.as_fd(),
            nix::poll::PollFlags::POLLIN,
        ));
        if let Some(channel) = &preload {
            fds.push(nix::poll::PollFd::new(
                channel.as_fd(),
                nix::poll::PollFlags::POLLIN,
            ));
        } else if let Some(door) = &listener {
            fds.push(nix::poll::PollFd::new(
                door.as_fd(),
                nix::poll::PollFlags::POLLIN,
            ));
        }
        match nix::poll::poll(&mut fds, nix::poll::PollTimeout::NONE) {
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return std::process::ExitCode::SUCCESS,
        }
        let harness_ready = fds[0].revents().is_some_and(|r| !r.is_empty());
        let second_ready = fds.len() > 1 && fds[1].revents().is_some_and(|r| !r.is_empty());
        drop(fds);

        // The preload door first, so a seal landing in the same wakeup as a
        // parked ask answers in that wakeup rather than the next.
        if second_ready {
            if let Some(channel) = preload.as_mut() {
                let mut live = fill_buffer(channel, &mut preload_frames);
                while let Some(line) = take_frame(&mut preload_frames) {
                    if !preload_opened {
                        // **The opener's retirement is the one act this path
                        // adds**, per the Spec: the session the opener
                        // declares is the one whose rows retire, in the same
                        // transaction that records it, before any distillate
                        // lands, so re-running a preload replaces the
                        // holdings rather than appending to them and a dead
                        // driver's prefix needs no cleanup act. **A frame
                        // declaring no session is not an opener**. Every
                        // refused opener ends this driver's attempt without
                        // retiring holdings or killing the member. The
                        // dead-driver cleanup below clears buffered traffic
                        // and re-stands the door, keeping parked asks parked.
                        let Some(preload_session) = parse_session(&line).filter(|s| !s.is_empty())
                        else {
                            eprintln!(
                                "{}",
                                serde_json::json!({"state_fault": "missing nonempty session in preload opener"})
                            );
                            live = false;
                            break;
                        };
                        let Some(election) = parse_election(&line) else {
                            eprintln!(
                                "{}",
                                serde_json::json!({"state_fault": "malformed election in preload opener"})
                            );
                            live = false;
                            break;
                        };
                        // **A refused election says which path refused it.**
                        // This door can fail on an election the operator
                        // wrote, per the service engine's naming. Preserve
                        // the diagnosis while the member stays alive for a
                        // retry. The first door prints the same fault before
                        // its startup exit.
                        if let Err(fault) = store.retire_and_index(&preload_session, &election) {
                            eprintln!(
                                "{}",
                                serde_json::json!({"state_fault": format!("{fault:?}")})
                            );
                            live = false;
                            break;
                        }
                        preload_opened = true;
                        continue;
                    }
                    if is_seal(&line) {
                        parking.seal();
                        continue;
                    }
                    if let Some(distillate) = parse_distillate(&line) {
                        let _ = store.land(&distillate);
                    }
                }
                if !live {
                    // A close without a seal leaves the fact false, which is
                    // the clause's point: the prefix looks like holdings at
                    // rest and must not answer a parked ask. **The door
                    // stands again**, per the contract's retry clause: the
                    // next opener is the cleanup, whether the last close
                    // sealed or died, so the name rebinds and a retry finds
                    // a door rather than a refused dial.
                    preload = None;
                    preload_frames.clear();
                    preload_opened = false;
                    listener = preload_path.as_deref().and_then(stand_preload_name);
                }
            } else if let Some(door) = &listener
                && let Some(channel) = admit_operator(door)
            {
                preload = Some(channel);
                listener = None;
            }
        }

        if harness_ready {
            let live = harness.fill();
            if drain_harness_lines(&mut harness, store, session, &mut parking).is_some() {
                return std::process::ExitCode::SUCCESS;
            }
            if !live {
                // Closure is retirement, the holdings standing for the next
                // run. A replay still parked is cleared unanswered with the
                // channel, its answer never owed.
                return std::process::ExitCode::SUCCESS;
            }
        }

        // **The seal is the only fact that answers**, and its view is the
        // seal's position: every distillate received through the seal is in
        // the answer, which holds by construction here because the preload
        // frames of this wakeup landed above before the answer is built.
        for ask in parking.take_ready() {
            if let Ok(frame) = answer_frame(&ask, store, session)
                && frame.len() <= ANSWER_BOUND
                && !harness.respond(frame.as_bytes())
            {
                return std::process::ExitCode::SUCCESS;
            }
        }
    }
}

/// Drain every whole line the harness buffer holds: distillates land, asks
/// answer against pre-ask holdings, and a replay parks per the parking law.
/// `Some` says the peer stopped reading an answer and the seam retires.
/// Never touches the socket, so a caller draining buffered remainders
/// cannot block on a peer that sent nothing since.
fn drain_harness_lines(
    harness: &mut LineReader<'_>,
    store: &mut dyn Store,
    session: &str,
    parking: &mut ReplayParking,
) -> Option<std::process::ExitCode> {
    while let Some(line) = harness.take_line() {
        if let Some(distillate) = parse_distillate(&line) {
            let _ = store.land(&distillate);
            continue;
        }
        let Some(ask) = parse_ask(&line) else {
            continue;
        };
        // **The parked ask steps out of the arrival order**, per the
        // contract's stated exception: a shape or grants ask arriving
        // while an ask parks is answered in its own arrival order, against
        // the holdings the stream carried before it, and the replay,
        // identity, and recall asks park where the door stands and no seal
        // has landed, per `weaver-state-Spec` section 4 as of 2026-09-06.
        if parking.parks(&ask) {
            continue;
        }
        // A store that cannot answer, like an answer past the bound, is
        // silence the harness's bound converts, per the contract: custody
        // never invents an answer shape for a fault.
        if let Ok(frame) = answer_frame(&ask, store, session)
            && frame.len() <= ANSWER_BOUND
            && !harness.respond(frame.as_bytes())
        {
            return Some(std::process::ExitCode::SUCCESS);
        }
    }
    None
}

/// One ask's answer frame against the holdings as they stand, the one
/// site the five asks are answered from, so a parked ask answers at the seal
/// through the same path an immediate one does.
fn answer_frame(
    ask: &Ask,
    store: &mut dyn Store,
    session: &str,
) -> Result<String, weaver_state::CustodyFault> {
    match ask {
        Ask::Shape => store
            .shape(session)
            .map(|shape| render_shape_answer(&shape)),
        Ask::Recall { last_turns } => store
            .recall(session, *last_turns)
            .map(|events| render_recall_answer(&events)),
        Ask::Replay => store
            .replay(session)
            .map(|events| render_replay_answer(&events)),
        Ask::Grants => store.grants().map(|surface| render_grants_answer(&surface)),
        Ask::Identity => store
            .identity(session)
            .map(|events| render_identity_answer(&events)),
    }
}

/// **The parking law, one unit**, per `weaver-harness-state-contract`
/// section 2 and `weaver-state-Spec` section 4's
/// `state-replay-answers-at-the-seal`: on a member standing with the
/// preload door, a replay ask parks until a seal has landed, whatever the
/// transport is doing, and the seal is a per-standing fact nothing sets
/// back. **The identity and recall asks park on the same fact**, as of
/// 2026-09-06 per issue #432: a session standing from a preloaded record
/// asks for its prefix and its conversation at the enter, before the
/// driver has sealed, and each answers at the seal in arrival order. At
/// most one replay parks, a second replacing the first, the replaced ask
/// cleared unanswered because its asker's bound already converted it, and
/// the replacement rule reaches the replay alone. Where no door stands, an
/// ask answers immediately like its siblings.
struct ReplayParking {
    door_stands: bool,
    sealed: bool,
    parked: Vec<Ask>,
}

impl ReplayParking {
    fn new(door_stands: bool) -> Self {
        ReplayParking {
            door_stands,
            sealed: false,
            parked: Vec::new(),
        }
    }

    /// An ask arrives: `true` says it parks and `false` says it answers
    /// now. Only the replay, identity, and recall asks park, the shape and
    /// grants asks answering against pre-ask holdings whatever stands.
    fn parks(&mut self, ask: &Ask) -> bool {
        let parkable = matches!(ask, Ask::Replay | Ask::Identity | Ask::Recall { .. });
        if !parkable || !self.door_stands || self.sealed {
            return false;
        }
        if matches!(ask, Ask::Replay) {
            self.parked.retain(|held| !matches!(held, Ask::Replay));
        }
        self.parked.push(ask.clone());
        true
    }

    /// The seal lands. Nothing sets it back.
    fn seal(&mut self) {
        self.sealed = true;
    }

    /// The parked asks ready to answer, in arrival order, clearing the slots
    /// when they are: the seal is the only fact that answers.
    fn take_ready(&mut self) -> Vec<Ask> {
        if self.sealed {
            return std::mem::take(&mut self.parked);
        }
        Vec::new()
    }
}

/// Pop one buffered frame, or nothing where no whole frame is held. Never
/// touches a socket, so a caller draining frames cannot block on a peer that
/// stopped mid frame.
fn take_frame(buffer: &mut Vec<u8>) -> Option<String> {
    let position = buffer.iter().position(|&b| b == b'\n')?;
    let line: Vec<u8> = buffer.drain(..=position).collect();
    Some(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned())
}

/// Read whatever the socket has ready into the buffer. `false` says the
/// channel is done, by close or by fault or by a frame past the bound, which
/// the serve loop reads as the end of that door. The stream is non-blocking
/// by the time this runs, so `WouldBlock` is the ordinary answer and says
/// only that this wakeup is spent.
fn fill_buffer(stream: &mut std::os::unix::net::UnixStream, buffer: &mut Vec<u8>) -> bool {
    use std::io::Read;
    loop {
        if buffer.len() > LineReader::FRAME_BOUND {
            return false;
        }
        let mut chunk = [0u8; 65536];
        match stream.read(&mut chunk) {
            Ok(0) => return false,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return true,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
}

/// A process umask held for one bind and restored on every path out.
///
/// The gate carries the same type for the same reason, per
/// `weaver-gate-Spec` section 3: a Unix socket's mode comes from the umask at
/// creation, and setting it afterwards leaves the name live at whatever was
/// inherited. Serialized on the one process umask, so two binds cannot
/// interleave their save and restore.
struct PreloadUmask {
    previous: nix::sys::stat::Mode,
    _serialized: PreloadSerialized,
}

static PRELOAD_UMASK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Whether this thread already holds [`PRELOAD_UMASK`].
    ///
    /// **The mutex is not reentrant, and the guard alone does not make it
    /// so.** A caller that holds it and then does anything taking it again
    /// waits on itself, with no second party involved and no deadlock
    /// detector to say so. The lock serializes threads, and a thread cannot
    /// interleave with itself, so a nested acquire on the holding thread is
    /// a no-op. `weaver-gate-Spec` section 3 argues the same shape for the
    /// gate's guard.
    static PRELOAD_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The umask lock, held for a scope, reentrant within one thread.
///
/// `None` inside means the outer holder on this thread releases it. The
/// umask nests correctly regardless, each guard restoring what it found in
/// LIFO order.
struct PreloadSerialized(Option<std::sync::MutexGuard<'static, ()>>);

impl PreloadSerialized {
    fn acquire() -> Self {
        if PRELOAD_HELD.with(std::cell::Cell::get) {
            return PreloadSerialized(None);
        }
        // A poisoned lock still hands back the guard: the umask is restored
        // on every path out including a panic, so the value behind it is
        // sound whatever happened to the thread that held it last.
        let guard = PRELOAD_UMASK
            .lock()
            .unwrap_or_else(|held| held.into_inner());
        PRELOAD_HELD.with(|held| held.set(true));
        PreloadSerialized(Some(guard))
    }
}

impl Drop for PreloadSerialized {
    fn drop(&mut self) {
        if self.0.is_some() {
            PRELOAD_HELD.with(|held| held.set(false));
        }
    }
}

/// Runs `work` with the process umask held, for a caller that must read or
/// set it around this module's own use.
///
/// **Exposed to this crate's tests because the resource is the process's.** A
/// test reading the ambient umask, or electing a known one so a mode
/// assertion is not vacuous, races the bind's own guard otherwise.
#[cfg(test)]
fn with_umask_held<T>(work: impl FnOnce() -> T) -> T {
    let _serialized = PreloadSerialized::acquire();
    work()
}

impl PreloadUmask {
    /// Deny every bit to group and other, so the name lands at `0700`.
    fn deny_all_but_owner() -> Self {
        let serialized = PreloadSerialized::acquire();
        PreloadUmask {
            previous: nix::sys::stat::umask(
                nix::sys::stat::Mode::S_IRWXG | nix::sys::stat::Mode::S_IRWXO,
            ),
            _serialized: serialized,
        }
    }
}

impl Drop for PreloadUmask {
    fn drop(&mut self) {
        nix::sys::stat::umask(self.previous);
    }
}

/// Bind the preload name and listen, without waiting for a peer. The door
/// stands from the load so a driver dialing early finds it, and the accept
/// happens in the serve loop beside the first door's traffic.
fn stand_preload_name(path: &str) -> Option<std::os::unix::net::UnixListener> {
    let _ = std::fs::remove_file(path);
    // **The mode is elected in the creating call**, the reasoning of
    // `weaver-gate-Spec` section 3 applied to this door. `bind` sets none, so
    // the name would land at `0777 & ~umask` and the boundary's permissions
    // would be whatever umask this process inherited - `0777` on one box and
    // `0775` on another from one build, on 2026-08-28.
    //
    // **`0700` here rather than the gate's `0770`**, because the accept below
    // admits `uid() == 0` and no one else, so there is no group to reach the
    // door and a mode granting one would describe an access this door does
    // not offer. The credential check is the lock that decides; this is the
    // one that stops a stranger arriving at it.
    //
    // Held across the bind rather than set on the path afterwards, for the
    // reason the gate states: a mode set after `bind` leaves the name live at
    // the inherited mode in between, races a path an unprivileged process may
    // be able to swap, and on failure leaves a file behind.
    //
    // conforms: state-preload-door-states-its-mode
    let listener = {
        let _mask = PreloadUmask::deny_all_but_owner();
        std::os::unix::net::UnixListener::bind(path)
    };
    match listener {
        Ok(listener) => {
            if listener.set_nonblocking(true).is_err() {
                let _ = std::fs::remove_file(path);
                return None;
            }
            Some(listener)
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"state_fault": format!("preload name unavailable: {error}")})
            );
            None
        }
    }
}

/// **This member's one credential judgment**, per `weaver-state-Spec`
/// section 4 as ruled 2026-08-26: this accept admits the operator principal
/// and refuses every other peer before any byte is read, the agent's among
/// them and no longer knowable by number, the vector having dropped the
/// agent's uid with the first door's judgment. The operator principal is
/// root today, the identity the driver runs as over the operator's own
/// storage, per `weaver-analysis-state-contract`.
fn admit_operator(
    listener: &std::os::unix::net::UnixListener,
) -> Option<std::os::unix::net::UnixStream> {
    let (channel, _address) = listener.accept().ok()?;
    match nix::sys::socket::getsockopt(&channel, nix::sys::socket::sockopt::PeerCredentials) {
        Ok(credentials) if credentials.uid() == 0 => {
            if channel.set_nonblocking(true).is_err() {
                return None;
            }
            Some(channel)
        }
        Ok(_) => {
            eprintln!(
                "{}",
                serde_json::json!({
                    "state_fault": "preload dial refused: only the operator principal preloads"
                })
            );
            None
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"state_fault": format!("credential read failed: {error}")})
            );
            None
        }
    }
}

/// Whether a preload frame is the seal: one empty frame after the last
/// distillate, per `weaver-analysis-state-contract` section 2. Empty means
/// an empty JSON object, which is what distinguishes a seal from a blank
/// line a sender's framing left behind.
fn is_seal(line: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(line) {
        Ok(serde_json::Value::Object(members)) => members.is_empty(),
        _ => false,
    }
}

/// The session the opener names, per the contract's `election` term as
/// amended 2026-08-20. Absent where the frame does not parse or carries no
/// session, which the caller reads as the empty session.
fn parse_session(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    Some(value.get("session")?.as_str()?.to_string())
}

/// The opener carries the whole election, per harness-state contract section 2.
/// Missing or mistyped members refuse the whole parse. Explicit empty keys or
/// paths remain valid; neither absence nor malformed content stands for them.
fn parse_election(line: &str) -> Option<Election> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let election = value.get("election")?;
    let all_kinds = election.get("all_kinds")?.as_bool()?;
    let keys = election
        .get("keys")?
        .as_array()?
        .iter()
        .map(|entry| {
            let kind = entry.get("kind")?.as_str()?.to_string();
            let paths = entry
                .get("paths")?
                .as_array()?
                .iter()
                .map(|path| path.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()?;
            Some((kind, paths))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Election { all_kinds, keys })
}

/// Newline-delimited reading over the stream, per the seam's provisional
/// JSON encoding.
struct LineReader<'a> {
    stream: &'a mut std::os::unix::net::UnixStream,
    buffer: Vec<u8>,
}

impl<'a> LineReader<'a> {
    fn new(stream: &'a mut std::os::unix::net::UnixStream) -> Self {
        LineReader {
            stream,
            buffer: Vec::new(),
        }
    }

    /// One frame larger than the bound is not the seam's traffic, and the
    /// custodian answers it as closure rather than growing without bound:
    /// the peer holds a credential, not a license to exhaust this process.
    const FRAME_BOUND: usize = 8 * 1024 * 1024;

    /// Write one answer frame back on the channel, whole inside the write
    /// bound or reporting the seam broken: the serve direction's one write
    /// site, used only when asked, per the contract. The wait rides a poll
    /// deadline because a blocking write against a peer that stopped
    /// reading would wedge custody for the session's life.
    fn respond(&mut self, bytes: &[u8]) -> bool {
        use std::io::Write;
        use std::os::fd::AsFd;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(u64::from(RESPOND_WAIT_MS));
        let mut sent = 0;
        while sent < bytes.len() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let wait = remaining.as_millis().min(u128::from(u16::MAX)) as u16;
            let mut fds = [nix::poll::PollFd::new(
                self.stream.as_fd(),
                nix::poll::PollFlags::POLLOUT,
            )];
            match nix::poll::poll(&mut fds, wait) {
                Ok(0) => return false,
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return false,
            }
            match self.stream.write(&bytes[sent..]) {
                Ok(0) => return false,
                Ok(count) => sent += count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        true
    }

    fn take_line(&mut self) -> Option<String> {
        take_frame(&mut self.buffer)
    }

    fn fill(&mut self) -> bool {
        fill_buffer(self.stream, &mut self.buffer)
    }

    fn next_line(&mut self) -> Option<String> {
        loop {
            if let Some(position) = self.buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=position).collect();
                let text = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
                return Some(text);
            }
            if self.buffer.len() > Self::FRAME_BOUND {
                return None;
            }
            let mut chunk = [0u8; 65536];
            match self.stream.read(&mut chunk) {
                Ok(0) | Err(_) => return None,
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// A path under the temp directory, removed when the test ends, pass or
    /// fail: the guard drops on the unwind a failed assertion takes as on a
    /// clean return (#690 item C2.9).
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            match std::fs::symlink_metadata(&self.0) {
                Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
                Ok(_) => drop(std::fs::remove_file(&self.0)),
                Err(_) => {}
            }
        }
    }

    /// **The engine leads and the positionals keep their places**, per
    /// `weaver-state-Spec` section 4 as of 2026-09-04. Perturbation: parse
    /// the flags after the positionals and the diagnostic case reads the
    /// preload name as the territory.
    #[test]
    fn the_vector_elects_the_engine_before_the_territory() {
        fn words(v: &[&str]) -> std::vec::IntoIter<String> {
            v.iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        }
        let embedded =
            super::StoreVector::parse(words(&["--engine", "sqlite", "/t"])).expect("parses");
        assert_eq!(embedded.engine, "sqlite");
        assert_eq!(embedded.territory, "/t");
        assert!(embedded.preload_socket.is_none());
        let bare = super::StoreVector::parse(words(&["/t", "/t/preload.sock"])).expect("parses");
        assert_eq!(
            bare.engine, "sqlite",
            "absent flags are the embedded engine"
        );
        assert_eq!(bare.preload_socket.as_deref(), Some("/t/preload.sock"));
        let service = super::StoreVector::parse(words(&[
            "--engine",
            "postgres",
            "--store-socket",
            "/run/postgresql",
            "--database",
            "d",
            "--role",
            "r",
            "/t",
            "/t/preload.sock",
        ]))
        .expect("parses");
        assert_eq!(service.engine, "postgres");
        assert_eq!(service.socket.as_deref(), Some("/run/postgresql"));
        assert_eq!(service.database.as_deref(), Some("d"));
        assert_eq!(service.role.as_deref(), Some("r"));
        assert_eq!(service.territory, "/t");
        assert!(
            super::StoreVector::parse(words(&["--engine"])).is_none(),
            "a flag without a value"
        );
        assert!(
            super::StoreVector::parse(words(&["--other", "x", "/t"])).is_none(),
            "an unknown flag"
        );
        assert!(
            super::StoreVector::parse(words(&["/t", "/p", "/x"])).is_none(),
            "three positionals"
        );
        assert!(
            super::StoreVector::parse(words(&[])).is_none(),
            "no territory"
        );
    }

    use super::*;

    /// **The preload door denies every uid but its owner.**
    ///
    /// `bind` sets no mode, so this name would land at `0777 & ~umask` and
    /// the boundary's permissions would be whatever umask the process
    /// inherited. Only `uid() == 0` is admitted at the accept, so there is no
    /// group to reach the door, and `0700` is the access it offers.
    ///
    /// **The ambient umask is read and left alone**, and where it would
    /// produce `0700` by itself the test says so and skips rather than
    /// asserting against a run that cannot distinguish.
    ///
    /// Perturbation: drop the `PreloadUmask::deny_all_but_owner()` guard from
    /// `stand_preload_name` and this reports `0777`. Watched under exactly
    /// that removal, and watched **here** - the form this replaced skipped
    /// wherever the ambient umask already produced `0700`, which `0077` does,
    /// so on a hardened image it ran nowhere while its record said otherwise.
    #[test]
    fn the_preload_door_denies_every_uid_but_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "weaver-state-preload-mode-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )));
        let path = scratch.0.to_str().expect("a utf-8 scratch path");
        // **The ambient umask is elected rather than read, so this runs
        // everywhere.** An earlier form read it and skipped where it already
        // produced `0700`, which `0077` does - a common hardened default, so
        // the watch skipped on a hardened CI image and a later removal of
        // the guard would have shipped green. Reading avoided a false red at
        // the cost of the watch running nowhere, which is the trade this
        // program does not take.
        //
        // `0o000` cannot produce `0700` by itself, so under it the mode on
        // disk is the election or nothing. The lock is held across the whole
        // window, and it is reentrant, so `stand_preload_name` taking it
        // again inside is a no-op rather than a wait.
        let elected = with_umask_held(|| {
            let previous = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
            let stood = stand_preload_name(path);
            let seen = std::fs::metadata(path).map(|meta| meta.permissions().mode() & 0o777);
            nix::sys::stat::umask(previous);
            (stood, seen)
        });
        let (listener, seen) = elected;
        assert!(listener.is_some(), "the preload name stands");
        let mode = seen.expect("the socket is on disk");
        assert_eq!(
            mode, 0o700,
            "the door states its mode rather than inheriting one, got {mode:04o}"
        );
        drop(listener);
    }

    /// **A replay ask parks at an open preload until the seal**, per
    /// `weaver-harness-state-contract` section 2 and
    /// `state-replay-answers-at-the-seal`: where the door stands and no
    /// seal has landed the ask parks, a second replaces the first with one
    /// answer still owed, the seal readies exactly one answer, and a
    /// sealless close readies nothing because the seal alone answers.
    /// Where no door stands the ask answers immediately.
    ///
    /// Perturbation: make `parks` ignore the seal (park whenever the door
    /// stands) and the after-the-seal case fails; make `take_ready` ignore
    /// the seal and the sealless case fails.
    #[test]
    fn a_replay_parks_at_an_open_preload_until_the_seal() {
        // No door: never parks.
        let mut open_door = ReplayParking::new(false);
        assert!(
            !open_door.parks(&Ask::Replay),
            "no door, the ask answers now"
        );
        // Door standing, no seal: parks, and a second ask replaces the
        // first rather than queueing a second answer.
        let mut parking = ReplayParking::new(true);
        assert!(parking.parks(&Ask::Replay), "an open preload parks the ask");
        assert!(
            parking.parks(&Ask::Replay),
            "a second ask replaces the first"
        );
        assert!(parking.take_ready().is_empty(), "the seal alone answers");
        parking.seal();
        assert_eq!(
            parking.take_ready(),
            vec![Ask::Replay],
            "the seal readies one"
        );
        assert!(parking.take_ready().is_empty(), "one answer per parked ask");
        // After the seal, an ask answers immediately: sealed never unsets.
        assert!(!parking.parks(&Ask::Replay), "after the seal nothing parks");
    }

    /// **The identity and recall asks park on the seal beside the replay**,
    /// per `weaver-state-Spec` section 4 as of 2026-09-06 and the contract's
    /// parking clause of 2026-09-04: where the door stands and no seal has
    /// landed each parks, they answer at the seal in arrival order, the
    /// replay's replacement rule reaches the replay alone, and the shape
    /// and grants asks never park.
    ///
    /// Perturbation: park the replay alone, as the law did before this
    /// act, and the identity ask answers before the seal from empty
    /// holdings, the first assertion failing. Watched under exactly that
    /// removal.
    #[test]
    fn the_enters_asks_park_on_the_seal_in_arrival_order() {
        let mut parking = ReplayParking::new(true);
        assert!(parking.parks(&Ask::Identity), "the identity ask parks");
        assert!(
            parking.parks(&Ask::Recall { last_turns: None }),
            "the recall ask parks"
        );
        assert!(parking.parks(&Ask::Replay));
        assert!(parking.parks(&Ask::Replay), "the replay replaces its own");
        assert!(!parking.parks(&Ask::Shape), "a shape ask answers now");
        assert!(!parking.parks(&Ask::Grants), "a grants ask answers now");
        assert!(parking.take_ready().is_empty(), "nothing before the seal");
        parking.seal();
        assert_eq!(
            parking.take_ready(),
            vec![Ask::Identity, Ask::Recall { last_turns: None }, Ask::Replay],
            "arrival order, the replay held once"
        );
        let mut doorless = ReplayParking::new(false);
        assert!(
            !doorless.parks(&Ask::Identity),
            "no door, the identity answers now"
        );
    }

    /// **The preload door admits the operator principal and refuses every
    /// other peer**, per `weaver-state-Spec` section 4 as ruled 2026-08-26:
    /// this member's one credential judgment, the agent's uid among the
    /// refused and no longer knowable by number. The dial in this test
    /// carries the running uid, which is not the operator's, so the door
    /// refuses it. The refusal half is what a suite can exercise, the admit
    /// half wanting the operator principal a test does not run as.
    ///
    /// Perturbation: drop the `credentials.uid() == 0` arm from
    /// `admit_operator` and this fails, a non-operator peer admitted to a
    /// door that exists to keep everything but the operator out.
    #[test]
    fn the_preload_door_refuses_every_peer_but_the_operator() {
        if nix::unistd::getuid().is_root() {
            // The refusal half is meaningful only unrooted: a root dial is
            // the operator principal and is admitted.
            return;
        }
        let scratch = Scratch(
            std::env::temp_dir().join(format!("weaver-state-preload-{}", std::process::id())),
        );
        let path = scratch.0.to_string_lossy().into_owned();
        let listener = stand_preload_name(&path).expect("the name stands");
        let _dialer = std::os::unix::net::UnixStream::connect(&path).expect("dials");
        // **Wait for the door to be readable before judging it.** The
        // listener is non-blocking and `admit_operator` answers `None`
        // both for a refused peer and for an accept that would block, so
        // a bare call could report a refusal the credential never made.
        // Polling first makes the accept certain and the `None` mean the
        // one thing this test is about.
        use std::os::fd::AsFd;
        let mut fds = [nix::poll::PollFd::new(
            listener.as_fd(),
            nix::poll::PollFlags::POLLIN,
        )];
        let ready = nix::poll::poll(&mut fds, 5_000u16).expect("the door answers poll");
        assert_eq!(ready, 1, "the dial reaches the door");
        assert!(
            admit_operator(&listener).is_none(),
            "a non-operator peer is refused"
        );
    }

    /// **The door stands only where the party that stands the member names
    /// it**, per `weaver-state-Spec` section 4, and this crate's half of that
    /// is the second argument: no name, no door. The kind is the caller's
    /// fact and reaches here as the name's presence alone, which is what
    /// keeps this crate from holding an opinion about the binding.
    ///
    /// **The perturbation is at the vector and not here.** No name reaches
    /// this function on a serving load, so the absence is decided in
    /// `StoreVector::parse`, and defaulting the preload positional there
    /// stands a door nothing should dial. Watched under exactly that change,
    /// by `the_vector_elects_the_engine_before_the_territory`, whose
    /// `preload_socket.is_none()` fails on it.
    ///
    /// **This test watches the positive half**, that a named one stands. Its
    /// first line takes a `None` through `Option::map` and is a property of
    /// that combinator rather than of this crate, `stand_preload_name` taking
    /// a `&str` and never an absence. It is disclosed rather than deleted
    /// because whether it goes is a decision about the suite.
    #[test]
    fn no_preload_name_means_no_preload_door() {
        let absent: Option<String> = None;
        assert!(
            absent.as_deref().map(stand_preload_name).is_none(),
            "a serving load names no preload socket and stands no door"
        );
        let scratch = Scratch(
            std::env::temp_dir().join(format!("weaver-state-named-{}", std::process::id())),
        );
        let path = scratch.0.to_string_lossy().into_owned();
        assert!(
            stand_preload_name(&path).is_some(),
            "and a named one stands"
        );
    }

    /// The operator's 2026-09-22 ruling admits trace only as test scaffolding.
    /// Read Cargo's dependency kinds, including aliases and target-qualified
    /// declarations, so no normal or build edge can silently reintroduce it.
    /// Perturbation: move the trace dependency back to production.
    #[test]
    fn trace_is_a_dev_dependency_only() {
        let metadata = std::process::Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--locked",
                "--offline",
            ])
            .output()
            .expect("cargo metadata runs");
        assert!(
            metadata.status.success(),
            "metadata failed: {}",
            String::from_utf8_lossy(&metadata.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"] == "weaver-state")
            .expect("state is in the manifest's workspace");
        let trace: Vec<_> = package["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|dependency| dependency["name"] == "weaver-trace")
            .collect();
        assert!(
            !trace.is_empty(),
            "the test scaffolding must remain declared"
        );
        assert!(
            trace.iter().all(|dependency| dependency["kind"] == "dev"),
            "trace must be test-only across every declaration: {trace:?}"
        );
    }

    /// The contract's election round trip, section 8: the opener as the
    /// tee renders it parses to the same election on this end, so a
    /// restarted member rebuilds the identical index set.
    #[test]
    fn the_opener_round_trips_across_the_seam() {
        let sent = weaver_trace::Election {
            all_kinds: false,
            keys: vec![weaver_trace::ElectedKind {
                kind: "turn.closed".into(),
                paths: vec!["close".into(), "request.sampling".into()],
            }],
        };
        let opener = weaver_trace::opener("s-1", &sent);
        let received = parse_election(opener.trim_end()).expect("the opener parses");
        assert!(!received.all_kinds);
        assert_eq!(
            received.keys,
            vec![(
                "turn.closed".to_string(),
                vec!["close".to_string(), "request.sampling".to_string()]
            )]
        );
    }

    // Shared by the parser and both real-door watches. Valid entries surround
    // invalid entries so silently filtering a partial election cannot pass.
    pub(super) fn malformed_openers() -> Vec<String> {
        use serde_json::{Value, json};
        let wrap = |election| json!({"session":"target", "election":election}).to_string();
        let mut cases = vec![
            "not json".into(),
            "null".into(),
            "[]".into(),
            json!({"session":"target"}).to_string(),
        ];
        for election in [
            Value::Null,
            json!([]),
            json!(false),
            json!({}),
            json!({"keys":[]}),
            json!({"all_kinds":false}),
        ] {
            cases.push(wrap(election));
        }
        for bad in [Value::Null, json!("false"), json!(0), json!([]), json!({})] {
            cases.push(wrap(json!({"all_kinds":bad,"keys":[]})));
        }
        for bad in [Value::Null, json!(false), json!(0), json!(""), json!({})] {
            cases.push(wrap(json!({"all_kinds":false,"keys":bad})));
        }
        let good = json!({"kind":"load", "paths":["new.index"]});
        let mut bad_entries = vec![
            Value::Null,
            json!([]),
            json!(false),
            json!("kind"),
            json!({}),
            json!({"kind":"load"}),
            json!({"paths":[]}),
        ];
        for bad in [Value::Null, json!(false), json!(0), json!([]), json!({})] {
            bad_entries.push(json!({"kind":bad,"paths":[]}));
        }
        for bad in [Value::Null, json!(false), json!(0), json!(""), json!({})] {
            bad_entries.push(json!({"kind":"load","paths":bad}));
        }
        for bad in [Value::Null, json!(false), json!(0), json!([]), json!({})] {
            bad_entries.push(json!({"kind":"load","paths":["before",bad,"after"]}));
        }
        for bad in bad_entries {
            cases.push(wrap(json!({"all_kinds":false,"keys":[good,bad,good]})));
        }
        cases
    }

    #[test]
    fn a_malformed_election_refuses_whole() {
        for opener in malformed_openers() {
            assert!(parse_election(&opener).is_none(), "accepted {opener}");
        }
    }

    #[test]
    fn explicit_empty_keys_and_paths_remain_elections() {
        for all_kinds in [false, true] {
            for keys in [vec![], vec![("load".to_string(), vec![])]] {
                let wire_keys: Vec<_> = keys
                    .iter()
                    .map(|(kind, paths)| serde_json::json!({"kind":kind,"paths":paths}))
                    .collect();
                let opener = serde_json::json!({"election":{
                    "all_kinds":all_kinds,"keys":wire_keys,"future":null}})
                .to_string();
                assert_eq!(parse_election(&opener), Some(Election { all_kinds, keys }));
            }
        }
    }

    /// A distillate as the tee renders it parses whole on this end: the
    /// envelope's five attributable, the elected pair carried verbatim.
    #[test]
    fn a_distillate_crosses_from_tee_to_row() {
        let line = concat!(
            r#"{"session":"alpha-1","run":"r-1","turn":"t-1","kind":"turn.closed","#,
            r#""sequence":"7","subsystem":"harness","wall_ms":1,"monotonic_ns":"2","#,
            r#""payload":{"close":"clean"}}"#
        );
        let election = weaver_trace::Election {
            all_kinds: true,
            keys: vec![weaver_trace::ElectedKind {
                kind: "turn.closed".into(),
                paths: vec!["close".into()],
            }],
        };
        let frame = weaver_trace::distill(line, &election).expect("distills");
        let distillate = parse_distillate(frame.trim_end()).expect("parses");
        assert_eq!(distillate.session, "alpha-1");
        assert_eq!(distillate.run, "r-1");
        assert_eq!(distillate.turn.as_deref(), Some("t-1"));
        assert_eq!(distillate.kind, "turn.closed");
        assert_eq!(distillate.sequence, 7);
        assert_eq!(
            distillate.pairs,
            vec![("close".to_string(), "\"clean\"".to_string())]
        );
    }
}

#[cfg(all(test, feature = "postgres"))]
use weaver_state::engine;

#[cfg(all(test, feature = "postgres"))]
#[allow(dead_code)]
#[path = "engine/postgres_scratch.rs"]
mod postgres_scratch;

#[cfg(all(test, feature = "postgres"))]
mod comparison;
