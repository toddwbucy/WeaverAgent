//! conforms: gate-instruction-two-fields-consumed
//! conforms: gate-client-socket-stream
//! conforms: gate-ready-follows-bind
//! conforms: gate-unlinks-nothing
//! conforms: gate-one-bind-site
//! conforms: gate-authenticated-at-accept
//! conforms: gate-agent-uid-denied-by-construction
//! conforms: gate-stopped-follows-close
//! conforms: gate-refused-raise-holds-nothing
//! conforms: gate-bind-shapes-pinned-by-doctest
//!
//! The instruction's resolution, the bind, and the predicate, per
//! `weaver-gate-Spec` section 3.
//!
//! **The instruction is resolved, never interpreted beyond its fields.** This
//! crate consumes exactly two things from it: the socket path to bind and the
//! access rule the predicate judges against.
//!
//! **This module exposes the crate's one bind site**, [`Hook::raise`], which
//! takes the instruction. A listener built anywhere else in this crate is out
//! of bounds, and the two named shapes are pinned here because an absence is
//! what a runtime test structurally cannot demonstrate.
//!
//! **The pins name the real site and fail on its type**, which is what makes
//! them mean anything. An earlier cut called a `Hook::bind` that never existed,
//! so both pins failed to compile on the missing name and would have gone on
//! passing if a path-taking constructor had been added beside it. These fail
//! because a path is not an instruction, and the day `raise` grows a path-
//! taking overload or a generic bound that admits one, they compile and fire:
//!
//! ```compile_fail
//! fn pin(path: &str) -> weaver_gate::hook::Hook {
//!     // A bare string is not an instruction.
//!     weaver_gate::hook::Hook::raise(path).unwrap()
//! }
//! ```
//!
//! ```compile_fail
//! fn pin(path: std::path::PathBuf) -> weaver_gate::hook::Hook {
//!     // Nor is a PathBuf outside the instruction.
//!     weaver_gate::hook::Hook::raise(&path).unwrap()
//! }
//! ```
//!
//! A compile-fail pin passes on any compile error, so this one compiles and
//! names the site and its accessor, failing loudly the day either is renamed
//! away and leaving the two above with nothing to fail against:
//!
//! ```
//! fn reads(hook: &weaver_gate::hook::Hook) -> &std::path::Path {
//!     hook.bound_path()
//! }
//! ```

use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use weaver_types::{AccessRule, GateInstruction, PeerIdentity, authorized};

/// A process umask held for one call and restored on every path out,
/// including a panic.
///
/// **The umask is how a Unix socket's mode is elected.** `bind` takes none,
/// so the file lands at `0777 & ~umask`, and setting the mode afterwards
/// leaves a window in which the socket is already listening at whatever the
/// process inherited.
///
/// **It is process-global, so the guard serializes.** Two threads each saving
/// the umask, setting it, and restoring what they saved will interleave into
/// one restoring the other's value mid-bind, and the socket lands at whatever
/// the loser inherited. A gate organ raises once on its own process and
/// meets no contention, but a caller that raises from several threads would
/// otherwise get a mode nobody elected - which is the defect this type
/// exists to close, reappearing one level up. The lock is held for exactly
/// the span the umask is.
struct Umask {
    previous: nix::sys::stat::Mode,
    _serialized: Serialized,
}

/// The one process umask, and therefore one lock.
static UMASK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Whether this thread already holds `UMASK`.
    ///
    /// **The mutex is not reentrant and the scoped shape does not make it
    /// so.** `with_umask_held` closed the case of a caller holding the lock
    /// across `Hook::raise`, and left the case of a caller calling
    /// `with_umask_held` from inside `with_umask_held` - or raising a hook
    /// from inside it - which self-deadlocks a single thread against itself
    /// with no second party involved. The serialization the lock buys is
    /// between threads, and a thread cannot interleave with itself, so a
    /// nested acquire on the holding thread is a no-op rather than a wait.
    static HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The umask lock, held for a scope, reentrant within one thread.
///
/// `None` inside means this thread already held it and the outer holder is
/// the one that will release: the umask itself still nests correctly, each
/// guard saving what it found and restoring it on drop in LIFO order.
struct Serialized(Option<std::sync::MutexGuard<'static, ()>>);

impl Serialized {
    fn acquire() -> Self {
        if HELD.with(std::cell::Cell::get) {
            return Serialized(None);
        }
        // A poisoned lock still hands back the guard: the umask is restored
        // on every path out including a panic, so the value behind it is
        // sound whatever happened to the thread that held it last.
        let guard = UMASK.lock().unwrap_or_else(|held| held.into_inner());
        HELD.with(|held| held.set(true));
        Serialized(Some(guard))
    }
}

impl Drop for Serialized {
    fn drop(&mut self) {
        if self.0.is_some() {
            HELD.with(|held| held.set(false));
        }
    }
}

/// Runs `work` with the process umask held, for a caller that must read or
/// set it around this crate's own use.
///
/// **Exposed because the resource is the process's and not this module's.** A
/// caller guarding only itself with RAII still races this crate's guard, and
/// a loosened window is visible to every other file the process creates.
///
/// **Scoped rather than handing back the guard**, so it cannot be held open
/// across a later call by accident - the shape a returned guard invited, and
/// the way a first form of this crate's own test hung the suite by holding it
/// across `Hook::raise`.
///
/// **Reentrant within a thread**, because scoping alone does not make nesting
/// safe: `with_umask_held(|| with_umask_held(..))`, or a raise from inside
/// one, would take the same non-reentrant mutex twice on one thread and
/// deadlock it against itself. The lock serializes threads, and a thread does
/// not interleave with itself, so the inner acquire is a no-op.
pub fn with_umask_held<T>(work: impl FnOnce() -> T) -> T {
    let _serialized = Serialized::acquire();
    work()
}

impl Umask {
    /// Deny every bit to others, so a socket created under it lands at `0770`.
    ///
    /// Connecting to a Unix socket needs write permission, so this is what
    /// excludes every uid outside the owner and the group. The read and
    /// execute bits a laxer mask would leave others buy them nothing on a
    /// socket, which is why the group is the boundary rather than the world.
    fn deny_others() -> Self {
        let serialized = Serialized::acquire();
        Umask {
            previous: nix::sys::stat::umask(nix::sys::stat::Mode::S_IRWXO),
            _serialized: serialized,
        }
    }
}

impl Drop for Umask {
    fn drop(&mut self) {
        nix::sys::stat::umask(self.previous);
    }
}

/// A standing hook: the bound listener and the rule every peer is judged
/// against.
///
/// Holding this is the raised state. Dropping it closes the listener, which is
/// what lets the lower's ordering hold by construction.
#[derive(Debug)]
pub struct Hook {
    listener: UnixListener,
    path: PathBuf,
    /// The instruction's rule with this process's own uid added, at raise,
    /// unconditionally.
    rule: AccessRule,
}

/// Why a raise refused.
#[derive(Debug, PartialEq)]
pub enum RaiseRefusal {
    /// The path could not be bound: occupied, unreachable, or refused by the
    /// filesystem. The reason travels for the operator who must clear it.
    BindFailed { detail: String },
}

impl Hook {
    /// **The crate's one bind site.** It takes what the raise directive
    /// carried and nothing else, so a listener cannot be built from a path
    /// that did not arrive over the seam.
    ///
    /// **The socket and the rule arrive separately because they have separate
    /// authors**, per `weaver-gate-Spec` section 3: the rule is the operator's
    /// election inside the instruction, and the path is the program's,
    /// supplied by the harness inside the unit's runtime directory. This crate
    /// resolves both and elects neither.
    ///
    /// The socket is `SOCK_STREAM`, elected on its contract's framing:
    /// `weaver-gate-world-contract` section 2 fixes newline-delimited JSON, one
    /// request per line, so the newline is the framing and a
    /// boundary-preserving type would carry a second framing under the first.
    ///
    /// **This crate unlinks nothing.** A path already occupied refuses with the
    /// reason carried, and a gate that deleted filesystem entries to make room
    /// for itself would hold an authority its charter never grants. Since the
    /// ruling of 2026-08-15 the case is close to unreachable rather than
    /// merely refused: the path sits in the unit's runtime directory, which
    /// the manager destroys with the unit, so no pathname outlives the worker
    /// that bound it. The refusal stays because unreachable by construction is
    /// a property of the deployment and this crate resolves what it is
    /// handed.
    ///
    /// **A refusal leaves nothing held.** A failed bind holds no listener and
    /// no half-bound socket, so the aggregate's rollback has nothing of this
    /// crate's to unwind: the listener is constructed by the bind itself, and a
    /// bind that fails constructs nothing.
    pub fn raise(
        instruction: &GateInstruction,
        socket: &std::path::Path,
    ) -> Result<Hook, RaiseRefusal> {
        // `UnixListener::bind` creates, binds, and listens, and Rust sets
        // close-on-exec in the creating call. Ready is answered only after this
        // returns, which is what makes ready a fact about the listener.
        //
        // **The mode is elected through the umask around the bind rather than
        // set on the path afterwards**, per `weaver-gate-Spec` section 3.
        // `bind` sets no mode, so the file lands at `0777 & ~umask` and the
        // access control of the agent's front door is decided by whatever
        // umask this process inherited - `0777` on one box and `0775` on
        // another from one build, on 2026-08-28.
        //
        // A `set_permissions` after the bind would answer that and open three
        // things this does not. `bind` also listens, so between the two calls
        // the socket is live at the inherited mode and connections queued
        // there are accepted afterwards. The chmod would go by path, and the
        // adversary this crate's reference walk names is a tool running as the
        // agent uid, which owns the runtime directory: in that window it can
        // unlink the socket and point the name elsewhere, so the chmod
        // succeeds against a decoy while the real socket keeps the umask's
        // mode and the raise reports success. And a chmod that failed would
        // return after the pathname exists, leaving a file this crate unlinks
        // nowhere - so every later raise on that path would answer `Address
        // already in use` for the life of the runtime directory, and
        // `gate-refused-raise-holds-nothing` would stop holding.
        //
        // The umask makes the mode part of the creating call: no window, no
        // path to race, and no post-bind failure to leave a file behind.
        //
        // conforms: gate-socket-mode-is-the-boundarys-election
        let listener = {
            let _mask = Umask::deny_others();
            UnixListener::bind(socket).map_err(|error| RaiseRefusal::BindFailed {
                detail: format!("{}: {error}", socket.display()),
            })?
        };

        // **The agent uid is denied by construction, not by configuration.**
        // This process runs as the agent uid, so it knows the one uid the
        // boundary exists to exclude: its own. Adding it to the deny set at
        // raise, unconditionally, means no operator mistake in the rule can
        // readmit the agent, denial winning over permission.
        let mut rule = instruction.access_rule.clone();
        rule.denied_uids.insert(nix::unistd::getuid().as_raw());

        Ok(Hook {
            listener,
            path: socket.to_path_buf(),
            rule,
        })
    }

    /// The listener, for a caller that must wait on it alongside the channel.
    /// Borrowed rather than handed over: the hook owns it, and its close is the
    /// lower.
    pub fn listener(&self) -> std::os::fd::BorrowedFd<'_> {
        self.listener.as_fd()
    }

    /// The path this hook stands on. The instruction's, unchanged.
    pub fn bound_path(&self) -> &Path {
        &self.path
    }

    /// The rule this hook judges against, the instruction's plus this process's
    /// own uid denied.
    pub fn rule(&self) -> &AccessRule {
        &self.rule
    }

    /// Accept one connection and judge it **before any byte is read**.
    ///
    /// The serve loop calls this whenever the listener is readable, so the
    /// boundary answers during the whole raised window rather than only when a
    /// test reaches it. What happens to an *admitted* connection is the relay
    /// of Spec section 4 and is deferred: the refusal half is what
    /// `weaver-gate-world-contract` section 5 specifies end to end, and it is
    /// the half this act implements.
    ///
    /// The peer's credential is read with `SO_PEERCRED` and the identity is
    /// judged by the floor's one predicate. A peer that fails is refused by
    /// closure with nothing written to it: an admitted-looking answer to a
    /// refused peer is a conversation the boundary already declined.
    ///
    /// The connection is returned only when it passes, so a caller cannot hold
    /// a refused peer's stream by construction.
    pub fn accept(&self) -> Result<Admitted, AcceptOutcome> {
        let (stream, _) = loop {
            match self.listener.accept() {
                Ok(accepted) => break accepted,
                // A signal landing mid-call is not a peer's doing, and the
                // channel's reads and writes already retry it. An accept that
                // reported the listener unusable on a signal would refuse a
                // peer that never arrived.
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(AcceptOutcome::Unusable {
                        detail: error.to_string(),
                    });
                }
            }
        };
        // Close-on-exec is already set: Rust's `UnixListener::accept` uses
        // `accept4(SOCK_CLOEXEC)` on Linux, so the flag arrives atomically with
        // the connection. Setting it again would spend a syscall per accept and
        // add a failure branch whose only reachable effect is to report the
        // listener dead when one connection hiccuped.

        let peer = match peer_identity(&stream) {
            Some(peer) => peer,
            None => {
                // A credential that cannot be read is a peer that cannot be
                // judged, which is a peer that is not admitted.
                drop(stream);
                return Err(AcceptOutcome::Denied);
            }
        };

        if !authorized(&peer, &self.rule) {
            // Refused by closure, before any content is read and with nothing
            // written back.
            drop(stream);
            return Err(AcceptOutcome::Denied);
        }
        Ok(Admitted { stream, peer })
    }

    /// Close the listener, returning once it has.
    ///
    /// **The lower closes first and confirms after**: stopped is answered only
    /// after this returns, so nothing new can arrive anywhere in the interior
    /// once the harness proceeds. In this pass no traffic exists, so the close
    /// is the whole of it.
    ///
    /// The path is left on the filesystem, unlinked by nobody here.
    pub fn lower(self) {
        drop(self.listener);
    }
}

/// A connection that passed the predicate, with the identity it passed as.
#[derive(Debug)]
pub struct Admitted {
    pub stream: UnixStream,
    pub peer: PeerIdentity,
}

/// What an accept produced when it produced no admitted connection.
#[derive(Debug, PartialEq)]
pub enum AcceptOutcome {
    /// The peer was judged and refused, or could not be judged at all.
    Denied,
    /// The listener itself failed.
    Unusable { detail: String },
}

/// Read the connecting peer's own credential from the kernel.
///
/// The identity is the kernel's and the peer never asserts it, which is why the
/// floor derives no `Deserialize` for it.
fn peer_identity(stream: &UnixStream) -> Option<PeerIdentity> {
    let credential =
        nix::sys::socket::getsockopt(&stream.as_fd(), nix::sys::socket::sockopt::PeerCredentials)
            .ok()?;
    Some(PeerIdentity {
        uid: credential.uid(),
        gid: credential.gid(),
        pid: credential.pid(),
    })
}
