//! conforms: harness-fault-below-the-exchange-layer
//! conforms: harness-outcome-one-case
//! conforms: harness-failed-set-refuses-construction
//!
//! The failure vocabulary, per `weaver-harness-Spec` section 7. A refusal is a
//! typed answer on an exchange and is always the floor's `LifecycleRefusal`,
//! drawn and never twinned. A channel fault is a failure below the exchange
//! layer, which is why it is a different type rather than a wider refusal set.

/// A failure below the exchange layer: no exchange can carry it, so no refusal
/// answers it and the service ends.
///
/// The three cases that carry no doc of their own: `Truncated` is a read the
/// kernel shortened, `Undecodable` is octets that do not decode to an envelope
/// and so cannot be attributed to any exchange for a refusal to answer, and
/// `Closed` is the far process gone, observed as death and never synthesized
/// into an answer. `SocketPathUnusable` and `WrongPeer` are documented at their
/// variants rather than restated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelFault {
    Truncated {
        bound: usize,
    },
    Undecodable,
    Closed,
    /// The coordination socket's name could not be bound: it is not a usable
    /// Unix address, or the bind or listen refused it. **An occupied name
    /// reaches here rather than being unlinked**, because the only ways a name
    /// is occupied are that a live worker holds it, where unlinking would
    /// strand the running agent's supervisor, or that the manager did not
    /// honor the runtime directory, where this crate's assumption is wrong and
    /// it should say so rather than repair.
    SocketPathUnusable {
        errno: i32,
    },
    /// The dialing peer is not root, refused at the accept before any byte.
    /// The socket is reachable from inside the sandbox by construction, so
    /// this is the check that refuses an elected tool rather than a name it
    /// could not resolve.
    WrongPeer {
        uid: u32,
    },
}

/// How service ends ordinarily, so the composition root branches on a value
/// rather than a guess. Exhaustive: a second case reaches every caller loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Leave was answered, which is the one ordinary ending. **A connection
    /// closing is not an ending**: admin is per-invocation, so each verb's
    /// connection closes when the verb answers and the worker accepts again,
    /// per `weaver-admin-harness-contract` section 4.
    ///
    /// Everything else that ends service is a fault and travels as the error
    /// half of `serve`'s result, so this enum carries the ordinary endings and
    /// no others. A second variant would be a case nothing constructs, which
    /// is the reserved slot the apex forbids in data as firmly as in an
    /// interface.
    Left,
}

/// Why adoption refused. Adoption fails only when a set fails: a hygiene call
/// that errors leaves the worker attachable, so `adopt`
/// returns the fault naming the set rather than proceeding unset, and a
/// `Harness` in hand means the hygiene held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionFault {
    /// The dumpable clear failed, leaving the worker attachable.
    DumpableNotCleared { errno: i32 },
}

impl std::fmt::Display for ChannelFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelFault::Truncated { bound } => {
                write!(f, "read truncated against the {bound} byte envelope bound")
            }
            ChannelFault::Undecodable => write!(f, "octets do not decode to an envelope"),
            ChannelFault::Closed => write!(f, "channel closed"),
            ChannelFault::SocketPathUnusable { errno } => {
                write!(f, "the coordination socket's name is unusable ({errno})")
            }
            ChannelFault::WrongPeer { uid } => {
                write!(f, "dialing peer is uid {uid} and not root")
            }
        }
    }
}

impl std::error::Error for ChannelFault {}

impl std::fmt::Display for AdoptionFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdoptionFault::DumpableNotCleared { errno } => {
                write!(f, "the dumpable flag could not be cleared ({errno})")
            }
        }
    }
}

impl std::error::Error for AdoptionFault {}

/// Why an authored message was refused before submit: the licensing rule of
/// `weaver-traits-Spec` section 3, enforced here because the recorder cannot
/// hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlicensedMessage {
    pub role: &'static str,
    pub block: &'static str,
}

impl std::fmt::Display for UnlicensedMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "a {} message may not carry a {} block",
            self.role, self.block
        )
    }
}

impl std::error::Error for UnlicensedMessage {}
