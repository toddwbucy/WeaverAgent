//! The even layer split of a multi-device GGUF load, per the device pair
//! amendment PAIR-3.
//!
//! **The division is arithmetic over the header's layer count and the
//! binding's width, and nothing else.** It reads no device and no free-memory
//! figure, so two loads of one artifact on one binding divide the same way.
//! The engine's own default, used when no tensor split is set, divides by each
//! device's free memory at load, which can move from run to run.
//!
//! Each device takes a contiguous block of repeating layers in binding order,
//! floor(L/N) or ceil(L/N) of them, the first L mod N devices taking the
//! larger count. The output layer, slot L in the engine's numbering, falls in
//! the last device's interval. The input layer stays in host memory under
//! every split, because the engine never offloads it.
//!
//! The engine maps a tensor split to layers by cumulative fraction over L + 1
//! slots (`llama-model.cpp`, `load_tensors`), so a proportional split is not
//! the even division: `[0.5, 0.5]` over 64 layers gives 33 and 31. The split
//! values below put every boundary half a slot inside its interval, never on
//! an edge, and the tests check them against a transcription of the engine's
//! mapping.
//!
//! The native (candle) pair does not use this division. Its tensor-parallel
//! design shards every layer's projections across both devices.

/// The repeating layers each device takes, in binding order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Division {
    layers: u32,
    counts: Vec<u32>,
}

impl Division {
    /// The even division of `layers` repeating layers over `devices` devices.
    ///
    /// `None` for no device or no layer, the two shapes with no division to
    /// compute. A device with no repeating layer is admitted by this
    /// arithmetic where the binding is wider than the model is deep: it takes
    /// floor(L/N), which is zero.
    pub fn even(layers: u32, devices: usize) -> Option<Division> {
        let width = u32::try_from(devices).ok().filter(|&n| n > 0)?;
        if layers == 0 {
            return None;
        }
        let (base, larger) = (layers / width, layers % width);
        let counts = (0..width)
            .map(|index| base + u32::from(index < larger))
            .collect();
        Some(Division { layers, counts })
    }

    /// The artifact's repeating layer count, L.
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// The repeating layers each device takes, in binding order.
    pub fn counts(&self) -> &[u32] {
        &self.counts
    }

    /// The binding index that holds engine slot `slot`: a repeating layer for
    /// a slot below L, the output layer for slot L. `None` past the output.
    pub fn index_of(&self, slot: u32) -> Option<usize> {
        if slot == self.layers {
            return Some(self.counts.len() - 1);
        }
        let mut end = 0u32;
        for (index, count) in self.counts.iter().enumerate() {
            end += count;
            if slot < end {
                return Some(index);
            }
        }
        None
    }

    /// The tensor split values handed to the engine, one per device.
    ///
    /// With C(i) the cumulative repeating-layer count through device i, the
    /// cumulative boundary of device i below the last is C(i) - 0.5 and the
    /// last device's is L + 1. Each value is its boundary minus the previous
    /// one. For L = 64 and N = 2 this is 31.5 and 33.5, which divides 32 and
    /// 32 with the output on the second device.
    ///
    /// The mapping holds only with every layer offloaded: the engine's first
    /// offloaded slot is L + 1 minus the offloaded count, so the loader sets
    /// that count to at least L + 1.
    pub fn split_values(&self) -> Vec<f32> {
        let last = self.counts.len() - 1;
        let mut values = Vec::with_capacity(self.counts.len());
        let mut previous = 0.0f64;
        let mut cumulative = 0u64;
        for (index, count) in self.counts.iter().enumerate() {
            cumulative += u64::from(*count);
            let boundary = if index == last {
                f64::from(self.layers) + 1.0
            } else {
                cumulative as f64 - 0.5
            };
            values.push((boundary - previous) as f32);
            previous = boundary;
        }
        values
    }
}

#[cfg(test)]
mod tests;
