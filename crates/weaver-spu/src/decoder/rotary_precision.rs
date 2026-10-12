//! Test-only: the `rotary_precision` suite of `decoder/mod.rs`,
//! moved out of the production file by #118.

use half::bf16;

// Exercise the production builder at a served head dimension (64 lanes).
#[cfg(feature = "cuda")]
pub(super) const HEAD_DIM: usize = 128;
#[cfg(feature = "cuda")]
pub(super) const ROPE_THETA: f64 = 1_000_000.0;
// Include the first inexact BF16 position, the 1024/4096 fixture edges,
// and a larger context position available to artifacts with that capacity.
pub(super) const POSITIONS: [usize; 4] = [257, 1023, 4095, 15_962];

/// Scalar diagnostic in cosine/sine order, casting only after the trig operation.
pub(super) fn entries(position: usize, inv: f32) -> [bf16; 2] {
    let angle = position as f32 * inv;
    [bf16::from_f32(angle.cos()), bf16::from_f32(angle.sin())]
}

/// BF16-first reference, including rounding the product before either trig operation.
pub(super) fn stock_entries(position: usize, inv: f32) -> [bf16; 2] {
    let angle =
        bf16::from_f32(bf16::from_f32(position as f32).to_f32() * bf16::from_f32(inv).to_f32())
            .to_f32();
    [bf16::from_f32(angle.cos()), bf16::from_f32(angle.sin())]
}

/// Require each reference to discriminate before testing against the fp64 angle.
pub(super) fn check(position: usize, inv: f32, watched: [f64; 2], stock: [f64; 2]) {
    let angle = position as f64 * inv as f64;
    for (name, actual, rounded, truth) in [
        ("cosine", watched[0], stock[0], angle.cos()),
        ("sine", watched[1], stock[1], angle.sin()),
    ] {
        // The 0.4 threshold includes sine at 257 and 4095, whose
        // BF16-first gaps are below 0.5. The weakest is 4095 (~0.4041).
        // This checks the reference discriminator, not production accuracy.
        assert!(
            (rounded - truth).abs() > 0.4,
            "{name} does not discriminate at {position}: stock {rounded}, true {truth}"
        );
        assert!(
            (actual - truth).abs() < 0.01,
            "{name} lost the rotary angle at {position}: got {actual}, expected {truth}"
        );
    }
}

/// Check scalar reference arithmetic only; this does not watch production ordering.
#[test]
fn scalar_references_show_bf16_first_rounding_loss() {
    for position in POSITIONS {
        check(
            position,
            1.0,
            entries(position, 1.0).map(bf16::to_f64),
            stock_entries(position, 1.0).map(bf16::to_f64),
        );
    }
}
