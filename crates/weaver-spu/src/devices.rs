//! The admission judgments that read the process environment or the driver,
//! per `weaver-spu-Spec` section 3 and the device pair amendment PAIR-1,
//! PAIR-2 and PAIR-4.
//!
//! **Inside the organ, ordinals are positions.** The harness launches the SPU
//! with `CUDA_DEVICE_ORDER=PCI_BUS_ID` and `CUDA_VISIBLE_DEVICES` set to the
//! binding's ordinals in binding order, so the process sees exactly the
//! binding's devices, renumbered from zero. Every driver and engine call here
//! uses position i for the device at binding index i. Every refusal and
//! account names the binding's own ordinal, never the position, so the
//! operator reads the numbers the device tools show.
//!
//! The judgments take the driver through [`DeviceQuery`], so the order they
//! run in and what each refuses on are watched by a recording query on a
//! machine with no device. The driver's own implementation is
//! `crate::gpu::Driver`, compiled under the `cuda` feature.

use weaver_types::DeviceOrdinal;

/// The variable that fixes the driver's device numbering.
pub const ORDER_VARIABLE: &str = "CUDA_DEVICE_ORDER";
/// The value of [`ORDER_VARIABLE`] PAIR-1 requires.
pub const ORDER_VALUE: &str = "PCI_BUS_ID";
/// The variable that masks the devices the process can see.
pub const VISIBLE_VARIABLE: &str = "CUDA_VISIBLE_DEVICES";

/// The two variables PAIR-1 names, as a process holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visibility {
    pub order: Option<String>,
    pub visible: Option<String>,
}

impl Visibility {
    /// This process's values. A value that is not UTF-8 is read lossily: it
    /// cannot equal the decimal mask the binding expects, so it refuses
    /// either way, and the refusal can still print it.
    pub fn of_process() -> Visibility {
        let read = |name| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        Visibility {
            order: read(ORDER_VARIABLE),
            visible: read(VISIBLE_VARIABLE),
        }
    }

    /// The mask a binding means: its ordinals in binding order, in decimal,
    /// comma-separated with no spaces.
    pub fn mask(devices: &[DeviceOrdinal]) -> String {
        devices
            .iter()
            .map(|d| d.0.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// A launch whose environment does not number the devices as the binding
/// means them, with both values for the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskRefusal {
    /// What the process holds.
    pub found: Visibility,
    /// The mask the binding means.
    pub expected: String,
}

/// **PAIR-1: the mask is judged before any device query.** The order must be
/// `PCI_BUS_ID` and the mask exactly the binding's ordinals in order. This
/// catches a launch path that does not go through the harness, a test or a
/// fixture driver, so the SPU never judges or loads under a numbering the
/// binding does not mean.
pub fn judge_mask(devices: &[DeviceOrdinal], found: &Visibility) -> Result<(), MaskRefusal> {
    let expected = Visibility::mask(devices);
    if found.order.as_deref() == Some(ORDER_VALUE) && found.visible.as_deref() == Some(&expected) {
        Ok(())
    } else {
        Err(MaskRefusal {
            found: found.clone(),
            expected,
        })
    }
}

/// The three facts PAIR-2 compares across the members of a binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The driver's device name string.
    pub name: String,
    /// Compute capability, major and minor.
    pub capability: (i32, i32),
    /// The driver's total-memory figure, in bytes.
    pub total: u64,
}

/// What admission asks the driver, by position under the mask.
pub trait DeviceQuery {
    /// The device's identity, `None` where the driver cannot answer.
    fn identity(&self, position: usize) -> Option<Identity>;
    /// Free and total memory, `None` where the driver cannot answer.
    fn memory(&self, position: usize) -> Option<(u64, u64)>;
    /// Whether the first device can reach the second's memory.
    fn can_access_peer(&self, from: usize, to: usize) -> bool;
}

/// Why the driver refused a device set. Each case names binding ordinals.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceRefusal {
    /// The ordinal names no device, or the driver would not open it.
    Unreachable { ordinal: u32 },
    /// The one inequality, read per device: room for this device's share plus
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
    /// **PAIR-2: the members differ** in name, compute capability or total
    /// memory. Carries every member's ordinal and facts, so the operator
    /// sees which one differs.
    Mismatched { members: Vec<(u32, Identity)> },
}

/// **PAIR-2: every member of a multi-device binding is identical** in name,
/// compute capability and total memory. A single-device binding asks
/// nothing. Judged after the set's shape and the mask, before room.
pub fn judge_identical(
    devices: &[DeviceOrdinal],
    query: &dyn DeviceQuery,
) -> Result<(), DeviceRefusal> {
    if devices.len() < 2 {
        return Ok(());
    }
    let mut members = Vec::with_capacity(devices.len());
    for (position, device) in devices.iter().enumerate() {
        let identity = query
            .identity(position)
            .ok_or(DeviceRefusal::Unreachable { ordinal: device.0 })?;
        members.push((device.0, identity));
    }
    if members
        .iter()
        .all(|(_, identity)| *identity == members[0].1)
    {
        Ok(())
    } else {
        Err(DeviceRefusal::Mismatched { members })
    }
}

/// **Room per device, then reach across each pair**, per Spec section 3 and
/// PAIR-4: device i must have room for `needs[i]` plus the headroom.
///
/// A single-device set is judged on room alone: there is no pair to exchange
/// activations across. Reach is asked in both directions, because the
/// driver's interface does not promise peer access is symmetric.
pub fn judge_room_and_reach(
    devices: &[DeviceOrdinal],
    needs: &[u64],
    headroom: u64,
    query: &dyn DeviceQuery,
) -> Result<(), DeviceRefusal> {
    assert_eq!(devices.len(), needs.len(), "one need per device");
    let mut memory = Vec::with_capacity(devices.len());
    for (position, device) in devices.iter().enumerate() {
        memory.push(
            query
                .memory(position)
                .ok_or(DeviceRefusal::Unreachable { ordinal: device.0 })?,
        );
    }
    for ((device, (free, total)), need) in devices.iter().zip(memory).zip(needs) {
        let needed = need.saturating_add(headroom);
        if free < needed {
            return Err(DeviceRefusal::NoRoom {
                ordinal: device.0,
                free,
                needed,
                total,
            });
        }
    }
    for (from, from_device) in devices.iter().enumerate() {
        for (to, to_device) in devices.iter().enumerate() {
            if from != to && !query.can_access_peer(from, to) {
                return Err(DeviceRefusal::NoPeerAccess {
                    from: from_device.0,
                    to: to_device.0,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
