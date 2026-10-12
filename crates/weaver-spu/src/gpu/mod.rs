//! conforms: spu-device-authority-is-the-driver
//! conforms: spu-room-refusal-carries-capacity
//!
//! The device queries the admission judgment reads, per `weaver-spu-Spec`
//! section 3.
//!
//! **The device judgment reads the driver rather than this crate's own
//! accounting.** Charter section 10 held the question open with the archived
//! tree's answer recorded as a preference without a reason: that tree held its
//! own allocation ledger as the authority and marked the driver query as
//! diagnostics only, which prefers the number that cannot see the thing the
//! check exists for. This crate holds no fleet ledger to prefer, and the case
//! it exists to catch is a device occupied by something this program did not
//! put there, so the authority is what the device reports free at the moment of
//! admission.
//!
//! **The cost is named rather than hidden:** the queries below run on the admit
//! path, which happens twice per residency and never inside a turn.
//!
//! This module compiles only under the `cuda` feature. The no-feature build
//! reaches none of it, which is what keeps the family surface testable on a
//! machine with no device.

use std::sync::Arc;

use cudarc::driver::{CudaContext, sys};
use weaver_types::DeviceOrdinal;

/// Why the driver refused a device set.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceRefusal {
    /// The ordinal names no device, or the driver would not open it.
    Unreachable { ordinal: u32 },
    /// The one inequality, read per device: room for this device's shard plus
    /// the residency's headroom.
    ///
    /// **`total` is carried because `free` alone cannot say why.** Seven
    /// gibibytes free against a thirty-five gibibyte need is a card too small
    /// where the capacity is eight, and a card held by something else where
    /// the capacity is forty-eight. One is a binding to change and the other
    /// is a machine to clear, and the driver answers both figures in the same
    /// call, so keeping one discarded the distinction for nothing.
    ///
    /// **No case is added and none is classified**, per Spec section 5. A
    /// reader with all three figures can tell, and a `Contended` case would be
    /// a judgment this crate has no ground to make: an occupant may be a
    /// second agent the operator wants there.
    NoRoom {
        ordinal: u32,
        free: u64,
        needed: u64,
        total: u64,
    },
    /// A sharded forward exchanges activations across the set, so a set without
    /// peer access is a set that cannot serve. Discovered at admit rather than
    /// at the first turn.
    NoPeerAccess { from: u32, to: u32 },
}

/// Judge room and reach across the assigned set.
///
/// Room first, per device, then reach across each pair. A single-device set is
/// judged on room alone: there is no pair to exchange activations across, so
/// asking the driver about peer access would be asking a question the topology
/// does not pose.
pub fn room_and_reach(
    devices: &[DeviceOrdinal],
    shard_bytes: u64,
    headroom_bytes: u64,
) -> Result<(), DeviceRefusal> {
    let mut contexts = Vec::with_capacity(devices.len());
    for device in devices {
        let context =
            context_for(device.0).ok_or(DeviceRefusal::Unreachable { ordinal: device.0 })?;
        contexts.push((device.0, context));
    }

    let needed = shard_bytes.saturating_add(headroom_bytes);
    for (ordinal, context) in &contexts {
        let (free, total) = context
            .mem_get_info()
            .map_err(|_| DeviceRefusal::Unreachable { ordinal: *ordinal })?;
        let (free, total) = (free as u64, total as u64);
        if free < needed {
            return Err(DeviceRefusal::NoRoom {
                ordinal: *ordinal,
                free,
                needed,
                total,
            });
        }
    }

    // Reach, across each ordered pair. Peer access is not guaranteed symmetric
    // by the driver's interface, so both directions are asked rather than one
    // being inferred from the other.
    for (from_ordinal, from_context) in &contexts {
        for (to_ordinal, to_context) in &contexts {
            if from_ordinal == to_ordinal {
                continue;
            }
            if !can_access_peer(from_context, to_context) {
                return Err(DeviceRefusal::NoPeerAccess {
                    from: *from_ordinal,
                    to: *to_ordinal,
                });
            }
        }
    }

    Ok(())
}

/// One primary-context handle per ordinal, held for the life of the process.
///
/// **The judge must not churn primary contexts, and the ground is a measured
/// corruption.** `CudaContext::new` retains the device's primary context and
/// the drop releases it, so a judge that was the only holder walked the
/// refcount through zero: a full primary-context destroy and recreate under
/// the driver, once per admit. The engine's own contexts arrive later and
/// looked past it, and candle's multi-device forward did not - after one
/// judge cycle the pair's cross-device arithmetic went nondeterministic,
/// weights byte-exact and every operation exact in isolation. Holding the
/// handle once removes the zero-crossing, and repeated admits stop paying a
/// context build besides.
fn context_for(ordinal: u32) -> Option<Arc<CudaContext>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static HELD: OnceLock<Mutex<HashMap<u32, Arc<CudaContext>>>> = OnceLock::new();
    let held = HELD.get_or_init(|| Mutex::new(HashMap::new()));
    let mut held = held.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(context) = held.get(&ordinal) {
        return Some(context.clone());
    }
    let context = CudaContext::new(ordinal as usize).ok()?;
    held.insert(ordinal, context.clone());
    Some(context)
}

/// Ask the driver whether peer access holds from one device to another.
///
/// `cuDeviceCanAccessPeer` has no safe wrapper in this cudarc line, so the call
/// is made directly. A driver error is read as no access rather than
/// propagated: the question asked is whether the set can serve, and a driver
/// that cannot answer it has not said yes.
fn can_access_peer(from: &Arc<CudaContext>, to: &Arc<CudaContext>) -> bool {
    let mut answer: std::ffi::c_int = 0;
    let status =
        unsafe { sys::cuDeviceCanAccessPeer(&mut answer, from.cu_device(), to.cu_device()) };
    status == sys::CUresult::CUDA_SUCCESS && answer != 0
}

#[cfg(test)]
mod tests;
