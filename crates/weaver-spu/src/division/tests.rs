use super::*;

/// **The engine's own mapping, transcribed.** `llama-model.cpp` at the
/// vendored `e21cdc11`, `load_tensors`: the split values are summed in
/// order into cumulative points, each divided by the total, all in `float`;
/// with `i_gpu_start = max(L + 1 - n_gpu_layers, 0)` and `act_gpu_layers =
/// min(n_gpu_layers, L + 1)`, slot `il` goes to the first device whose point
/// is greater than `float(il - i_gpu_start) / act_gpu_layers`
/// (`std::upper_bound`), and a slot below `i_gpu_start` stays on the host.
/// The output layer is slot L. `None` is the host.
///
/// The arithmetic stays in `f32` because the engine's does: a transcription
/// in `f64` could pass a value the engine's own rounding moves.
fn engine_mapping(split: &[f32], layers: u32, n_gpu_layers: i32) -> Vec<Option<usize>> {
    let mut points = split.to_vec();
    let mut sum = 0.0f32;
    for point in points.iter_mut() {
        sum += *point;
        *point = sum;
    }
    for point in points.iter_mut() {
        *point /= sum;
    }
    let n_layer = layers as i32;
    let i_gpu_start = (n_layer + 1 - n_gpu_layers).max(0);
    let act_gpu_layers = n_gpu_layers.min(n_layer + 1);
    (0..=n_layer)
        .map(|il| {
            if il < i_gpu_start || (il - i_gpu_start) >= act_gpu_layers {
                return None;
            }
            let x = (il - i_gpu_start) as f32 / act_gpu_layers as f32;
            let device = points.partition_point(|&p| p <= x);
            assert!(device < points.len(), "upper_bound ran past the devices");
            Some(device)
        })
        .collect()
}

/// The device each slot should land on, from the counts alone.
fn expected(division: &Division) -> Vec<Option<usize>> {
    (0..=division.layers())
        .map(|slot| division.index_of(slot))
        .collect()
}

/// The loader's offloaded count. `u32::MAX` crosses `with_n_gpu_layers` as
/// `i32::MAX`.
const ALL_LAYERS: i32 = i32::MAX;

/// **Granite 4.2 30B divides 32 and 32 with the output on the second
/// device**, and the values are the amendment's 31.5 and 33.5.
#[test]
fn granite_divides_thirty_two_and_thirty_two() {
    let division = Division::even(64, 2).expect("a division");
    assert_eq!(division.counts(), [32, 32]);
    assert_eq!(division.split_values(), [31.5, 33.5]);
    let mapping = engine_mapping(&division.split_values(), 64, ALL_LAYERS);
    assert_eq!(mapping, expected(&division));
    assert_eq!(mapping.iter().filter(|d| **d == Some(0)).count(), 32);
    assert_eq!(mapping.iter().filter(|d| **d == Some(1)).count(), 33);
    assert_eq!(
        mapping[64],
        Some(1),
        "the output layer is on the second device"
    );
}

/// **A proportional split is not the even division.** The halves give 33
/// and 31 over 64 layers, which is the defect the half-slot values exist to
/// avoid. This pins the transcription as able to tell them apart.
#[test]
fn proportional_halves_do_not_divide_evenly() {
    let division = Division::even(64, 2).expect("a division");
    let mapping = engine_mapping(&[0.5, 0.5], 64, ALL_LAYERS);
    assert_ne!(mapping, expected(&division));
    assert_eq!(mapping.iter().filter(|d| **d == Some(0)).count(), 33);
}

/// **The split is deterministic over every shape the binding can take**:
/// odd L, N = 3, and widths up to the engine's device limit, each checked
/// against the transcription of the engine's mapping. The first L mod N
/// devices take the larger count, and the output is on the last device.
///
/// Perturbation: replace `split_values` with the counts themselves
/// (proportional) and this test fails. A boundary on an edge (C(i) in place
/// of C(i) - 0.5) passes here, because the engine's `float` rounding happens
/// to land each edge the right way for these shapes; the next test is the
/// one that refuses it. Both watched.
#[test]
fn every_shape_reproduces_its_counts_under_the_engine_mapping() {
    for layers in 1..=130u32 {
        for devices in 1..=16usize {
            let division = Division::even(layers, devices).expect("a division");
            let counts = division.counts();
            assert_eq!(counts.iter().sum::<u32>(), layers);
            let (floor, ceil) = (layers / devices as u32, layers.div_ceil(devices as u32));
            let larger = (layers % devices as u32) as usize;
            for (index, count) in counts.iter().enumerate() {
                let want = if index < larger { ceil } else { floor };
                assert_eq!(*count, want, "L {layers} N {devices} device {index}");
            }
            let mapping = engine_mapping(&division.split_values(), layers, ALL_LAYERS);
            assert_eq!(mapping, expected(&division), "L {layers} N {devices}");
            assert_eq!(mapping[layers as usize], Some(devices - 1));
        }
    }
}

/// Odd L over three devices, by hand: 13 layers divide 5, 4 and 4.
#[test]
fn odd_layers_over_three_devices() {
    let division = Division::even(13, 3).expect("a division");
    assert_eq!(division.counts(), [5, 4, 4]);
    assert_eq!(division.split_values(), [4.5, 4.0, 5.5]);
    assert_eq!(
        engine_mapping(&division.split_values(), 13, ALL_LAYERS),
        expected(&division)
    );
}

/// **No boundary sits on an edge.** Each cumulative point, scaled back to
/// slots, is half a slot inside its interval, so the engine's `float`
/// rounding cannot move a layer across it.
///
/// Perturbation: drop the `- 0.5` and this test fails.
#[test]
fn every_boundary_is_half_a_slot_inside_its_interval() {
    for (layers, devices) in [(64u32, 2usize), (13, 3), (40, 3), (81, 4)] {
        let division = Division::even(layers, devices).expect("a division");
        let values = division.split_values();
        let mut boundary = 0.0f64;
        let mut cumulative = 0u32;
        for (index, value) in values.iter().enumerate().take(devices - 1) {
            boundary += f64::from(*value);
            cumulative += division.counts()[index];
            assert_eq!(boundary, f64::from(cumulative) - 0.5);
        }
        let total: f64 = values.iter().map(|v| f64::from(*v)).sum();
        assert_eq!(total, f64::from(layers) + 1.0);
    }
}

/// **The mapping holds only with every layer offloaded.** With fewer
/// offloaded than L + 1, the engine's first offloaded slot moves and the
/// division no longer matches, which is why the loader sets the count high.
#[test]
fn the_division_needs_every_layer_offloaded() {
    let division = Division::even(64, 2).expect("a division");
    let values = division.split_values();
    assert_eq!(engine_mapping(&values, 64, 65), expected(&division));
    assert_ne!(engine_mapping(&values, 64, 64), expected(&division));
}

/// The two shapes with nothing to divide.
#[test]
fn no_device_or_no_layer_has_no_division() {
    assert_eq!(Division::even(64, 0), None);
    assert_eq!(Division::even(0, 2), None);
}

/// `index_of` names the device for every slot and nothing past the output.
#[test]
fn index_of_names_every_slot_and_nothing_past_the_output() {
    let division = Division::even(5, 2).expect("a division");
    let slots: Vec<_> = (0..=6).map(|s| division.index_of(s)).collect();
    assert_eq!(
        slots,
        [Some(0), Some(0), Some(0), Some(1), Some(1), Some(1), None]
    );
}
