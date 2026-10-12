//! Test-only: the `rotary_precision_through_candle` suite of `decoder/mod.rs`,
//! moved out of the production file by #118.

use super::rotary_precision::{HEAD_DIM, POSITIONS, ROPE_THETA, check, stock_entries};
use candle_core::{DType, Device, IndexOp, Tensor};

/// Widen one stored BF16 entry without changing the value being measured.
fn entry(table: &Tensor, position: usize) -> candle_core::Result<f64> {
    table
        .i((position, 0))?
        .to_dtype(DType::F64)?
        .to_scalar::<f64>()
}

/// Pin nonzero frequency lanes independently of the production power expression.
#[test]
fn production_rotary_frequency_ladder_matches_reference() {
    let inv = super::native_pair::rotary_inverse_frequencies(HEAD_DIM, ROPE_THETA);
    assert_eq!(inv.len(), 64);
    // j=16: (10^6)^(-32/128) = 10^-1.5; j=32: 10^-3.
    for (lane, expected) in [(16, 0.031_622_776_601_683_79_f64), (32, 0.001)] {
        assert!(
            (f64::from(inv[lane]) - expected).abs() < expected * 1e-6,
            "inverse frequency lane {lane}: got {}, expected {expected}",
            inv[lane]
        );
    }
}

/// Read the pair loader's production tables and cross-check both stock references.
#[test]
fn the_production_rotary_tables_hold_their_angles() -> candle_core::Result<()> {
    let dev = Device::Cpu;
    let max = POSITIONS.iter().max().expect("at least one position") + 1;
    let inv = super::native_pair::rotary_inverse_frequencies(HEAD_DIM, ROPE_THETA);
    assert_eq!(
        inv[0], 1.0,
        "the truth uses the builder's unit inverse frequency"
    );
    let (cos, sin) = super::native_pair::rotary_tables(HEAD_DIM, ROPE_THETA, max, &dev, &dev)?;
    assert_eq!(cos.dims(), &[max, HEAD_DIM / 2]);
    assert_eq!(sin.dims(), cos.dims());
    assert_eq!(cos.dtype(), DType::BF16);
    assert_eq!(sin.dtype(), DType::BF16);

    // CPU matmul does not support BF16. Round each operand and the
    // product through BF16 to model that ordering for a one-term product.
    let inv = Tensor::from_vec(inv, (1, HEAD_DIM / 2), &dev)?
        .to_dtype(DType::BF16)?
        .to_dtype(DType::F32)?;
    let positions = Tensor::arange(0u32, max as u32, &dev)?
        .to_dtype(DType::BF16)?
        .to_dtype(DType::F32)?
        .reshape((max, 1))?;
    let rounded = positions.matmul(&inv)?.to_dtype(DType::BF16)?;
    let stock_cos = rounded.cos()?;
    let stock_sin = rounded.sin()?;
    for position in POSITIONS {
        let stock = [entry(&stock_cos, position)?, entry(&stock_sin, position)?];
        assert_eq!(
            stock,
            stock_entries(position, 1.0).map(half::bf16::to_f64),
            "Candle and half agree on both BF16-first outputs at {position}"
        );
        check(
            position,
            1.0,
            [entry(&cos, position)?, entry(&sin, position)?],
            stock,
        );
    }
    Ok(())
}
