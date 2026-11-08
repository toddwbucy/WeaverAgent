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

pub use crate::devices::DeviceRefusal;
use crate::devices::{DeviceQuery, Identity};

/// The driver, asked by position under the PAIR-1 mask.
///
/// **A position is an index into the binding, not a device ordinal.** Under
/// the mask the harness sets, the process sees the binding's devices
/// renumbered from zero in binding order, so position i is the device at
/// binding index i. The judgments in `crate::devices` turn a position back
/// into the binding's ordinal for every refusal.
#[derive(Debug, Clone, Copy, Default)]
pub struct Driver;

impl DeviceQuery for Driver {
    fn identity(&self, position: usize) -> Option<Identity> {
        let context = context_for(position)?;
        Some(Identity {
            name: context.name().ok()?,
            capability: context.compute_capability().ok()?,
            total: context.total_mem().ok()? as u64,
        })
    }

    fn memory(&self, position: usize) -> Option<(u64, u64)> {
        let (free, total) = context_for(position)?.mem_get_info().ok()?;
        Some((free as u64, total as u64))
    }

    fn can_access_peer(&self, from: usize, to: usize) -> bool {
        match (context_for(from), context_for(to)) {
            (Some(from), Some(to)) => can_access_peer(&from, &to),
            _ => false,
        }
    }
}

/// Judge room and reach across the assigned set with one even shard per
/// device, through [`Driver`].
///
/// Admission judges each device against its own share
/// (`crate::devices::judge_room_and_reach`). This even form stays for the
/// tests that probe the driver with a uniform need.
pub fn room_and_reach(
    devices: &[DeviceOrdinal],
    shard_bytes: u64,
    headroom_bytes: u64,
) -> Result<(), DeviceRefusal> {
    let needs = vec![shard_bytes; devices.len()];
    crate::devices::judge_room_and_reach(devices, &needs, headroom_bytes, &Driver)
}

/// One primary-context handle per position, held for the life of the
/// process.
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
fn context_for(position: usize) -> Option<Arc<CudaContext>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static HELD: OnceLock<Mutex<HashMap<usize, Arc<CudaContext>>>> = OnceLock::new();
    let held = HELD.get_or_init(|| Mutex::new(HashMap::new()));
    let mut held = held.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(context) = held.get(&position) {
        return Some(context.clone());
    }
    let context = CudaContext::new(position).ok()?;
    held.insert(position, context.clone());
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
