//! A GGUF file's metadata and tensor directory, read without loading any
//! tensor data.
//!
//! Two readers share it: the LoRA fixture's directory checks (LORA-2), which
//! read the adapter and the base, and the device pair amendment's room
//! judgment (PAIR-4), which reads every pinned shard of the base at the header
//! step. It moved here from `lora/` when the second reader arrived.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) enum Value {
    Text(String),
    Number(u64, u32),
    Float(f32),
    /// An integer array of at most [`ARRAY_CAPTURE`] elements, which is how a
    /// per-layer hyperparameter travels (`attention.head_count_kv` as one
    /// value per layer). Longer integer arrays, such as a vocabulary's token
    /// types, are skipped as [`Value::Other`].
    Integers(Vec<i64>),
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct Tensor {
    pub dims: [u64; 4],
    pub kind: u32,
    pub offset: u64,
}

#[derive(Debug)]
pub(crate) struct Directory {
    pub values: BTreeMap<String, Value>,
    pub tensors: BTreeMap<String, Tensor>,
    pub data_offset: u64,
    pub file_bytes: u64,
}

struct Reader {
    input: BufReader<File>,
    remaining: u64,
}

impl Reader {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut bytes = [0; N];
        self.remaining = self
            .remaining
            .checked_sub(N as u64)
            .ok_or("truncated GGUF")?;
        self.input
            .read_exact(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, String> {
        self.bytes().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, String> {
        self.bytes().map(u64::from_le_bytes)
    }

    fn skip(&mut self, count: u64) -> Result<(), String> {
        self.remaining = self.remaining.checked_sub(count).ok_or("truncated GGUF")?;
        let count = i64::try_from(count).map_err(|_| "GGUF offset overflow")?;
        // `seek_relative` keeps the buffer where the skip lands inside it, so
        // a vocabulary's string array costs no system call per entry.
        self.input.seek_relative(count).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn text(&mut self) -> Result<String, String> {
        let len = self.u64()?;
        self.remaining = self
            .remaining
            .checked_sub(len)
            .ok_or("truncated GGUF string")?;
        let len = usize::try_from(len).map_err(|_| "GGUF string length overflow")?;
        let mut data = Vec::new();
        data.try_reserve_exact(len).map_err(|e| e.to_string())?;
        data.resize(len, 0);
        self.input
            .read_exact(&mut data)
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8(data).map_err(|e| e.to_string())?;
        if text.contains('\0') {
            return Err("GGUF string contains NUL".into());
        }
        Ok(text)
    }

    /// One element of an integer array, sign-extended for the signed types.
    /// An unsigned 64-bit element past `i64::MAX` is refused rather than
    /// wrapped.
    fn integer(&mut self, element: u32) -> Result<i64, String> {
        Ok(match element {
            0 => self.bytes::<1>()?[0] as i64,
            1 => self.bytes::<1>()?[0] as i8 as i64,
            2 => u16::from_le_bytes(self.bytes()?) as i64,
            3 => i16::from_le_bytes(self.bytes()?) as i64,
            4 => self.u32()? as i64,
            5 => self.u32()? as i32 as i64,
            10 => i64::try_from(self.u64()?).map_err(|_| "GGUF array element overflow")?,
            11 => self.u64()? as i64,
            _ => return Err("invalid GGUF integer array type".into()),
        })
    }

    fn value(&mut self, kind: u32) -> Result<Value, String> {
        Ok(match kind {
            0 | 1 | 7 => Value::Number(self.bytes::<1>()?[0] as u64, kind),
            2 | 3 => Value::Number(u16::from_le_bytes(self.bytes()?) as u64, kind),
            4 | 5 => Value::Number(self.u32()? as u64, kind),
            6 => Value::Float(f32::from_le_bytes(self.bytes()?)),
            8 => Value::Text(self.text()?),
            9 => {
                let element = self.u32()?;
                let count = self.u64()?;
                if element == 8 {
                    if count > self.remaining / 8 {
                        return Err("truncated GGUF string array".into());
                    }
                    for _ in 0..count {
                        let len = self.u64()?;
                        self.skip(len)?;
                    }
                } else {
                    let size = match element {
                        0 | 1 | 7 => 1,
                        2 | 3 => 2,
                        4..=6 => 4,
                        10..=12 => 8,
                        _ => return Err("invalid GGUF array type".into()),
                    };
                    let integer = matches!(element, 0..=5 | 10 | 11);
                    if integer && count <= ARRAY_CAPTURE {
                        let mut values = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            values.push(self.integer(element)?);
                        }
                        return Ok(Value::Integers(values));
                    }
                    self.skip(
                        count
                            .checked_mul(size)
                            .ok_or("GGUF array length overflow")?,
                    )?;
                }
                Value::Other
            }
            10 | 11 => Value::Number(self.u64()?, kind),
            12 => {
                self.skip(8)?;
                Value::Other
            }
            _ => return Err("invalid GGUF value type".into()),
        })
    }
}

/// The longest integer array captured as [`Value::Integers`]. A per-layer
/// array has one element per layer, far below this.
pub(crate) const ARRAY_CAPTURE: u64 = 4096;

/// The tensor types the pinned engine defines (`ggml.h` at the vendored
/// `e21cdc11`), each with its block's element count and byte size, from
/// `ggml.c`'s type traits. The test pins every row against the engine's own
/// `ggml_blck_size` and `ggml_type_size`.
pub(crate) const TYPE_SIZES: [(u32, u64, u64); 34] = [
    (0, 1, 4),      // F32
    (1, 1, 2),      // F16
    (2, 32, 18),    // Q4_0
    (3, 32, 20),    // Q4_1
    (6, 32, 22),    // Q5_0
    (7, 32, 24),    // Q5_1
    (8, 32, 34),    // Q8_0
    (9, 32, 36),    // Q8_1
    (10, 256, 84),  // Q2_K
    (11, 256, 110), // Q3_K
    (12, 256, 144), // Q4_K
    (13, 256, 176), // Q5_K
    (14, 256, 210), // Q6_K
    (15, 256, 292), // Q8_K
    (16, 256, 66),  // IQ2_XXS
    (17, 256, 74),  // IQ2_XS
    (18, 256, 98),  // IQ3_XXS
    (19, 256, 50),  // IQ1_S
    (20, 32, 18),   // IQ4_NL
    (21, 256, 110), // IQ3_S
    (22, 256, 82),  // IQ2_S
    (23, 256, 136), // IQ4_XS
    (24, 1, 1),     // I8
    (25, 1, 2),     // I16
    (26, 1, 4),     // I32
    (27, 1, 8),     // I64
    (28, 1, 8),     // F64
    (29, 256, 56),  // IQ1_M
    (30, 1, 2),     // BF16
    (34, 256, 54),  // TQ1_0
    (35, 256, 66),  // TQ2_0
    (39, 32, 17),   // MXFP4
    (40, 64, 36),   // NVFP4
    (41, 128, 18),  // Q1_0
];

impl Tensor {
    /// The tensor's bytes as the engine lays it out, from its shape and type:
    /// the first dimension in whole blocks, times the rest. `None` for a type
    /// the table does not carry, a first dimension that is not a whole number
    /// of blocks, or an overflow.
    pub(crate) fn bytes(&self) -> Option<u64> {
        let (_, block, size) = TYPE_SIZES.iter().find(|(kind, _, _)| *kind == self.kind)?;
        if !self.dims[0].is_multiple_of(*block) {
            return None;
        }
        self.dims[1..]
            .iter()
            .try_fold((self.dims[0] / block).checked_mul(*size)?, |n, dim| {
                n.checked_mul(*dim)
            })
    }
}

pub(crate) fn read(path: &Path) -> Result<Directory, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let file_bytes = file.metadata().map_err(|e| e.to_string())?.len();
    let mut r = Reader {
        input: BufReader::new(file),
        remaining: file_bytes,
    };
    if r.bytes::<4>()? != *b"GGUF" || !matches!(r.u32()?, 2 | 3) {
        return Err("expected GGUF version 2 or 3".into());
    }
    let n_tensors = r.u64()?;
    let n_values = r.u64()?;
    if n_values > r.remaining / 12 || n_tensors > r.remaining / 24 {
        return Err("GGUF directory counts exceed the file".into());
    }
    let mut values = BTreeMap::new();
    for _ in 0..n_values {
        let key = r.text()?;
        let kind = r.u32()?;
        let value = r.value(kind)?;
        if values.insert(key, value).is_some() {
            return Err("duplicate GGUF metadata key".into());
        }
    }
    let mut tensors = BTreeMap::new();
    for _ in 0..n_tensors {
        let name = r.text()?;
        let n_dims = r.u32()?;
        if name.is_empty() || name.len() >= 64 || !(1..=4).contains(&n_dims) {
            return Err("invalid GGML tensor name or dimension count".into());
        }
        let mut dims = [1; 4];
        for dim in dims.iter_mut().take(n_dims as usize) {
            *dim = r.u64()?;
            if *dim == 0 || *dim > i64::MAX as u64 {
                return Err("invalid GGML tensor dimension".into());
            }
        }
        let tensor = Tensor {
            dims,
            kind: r.u32()?,
            offset: r.u64()?,
        };
        if tensors.insert(name, tensor).is_some() {
            return Err("duplicate GGUF tensor".into());
        }
    }
    let alignment = match values.get("general.alignment") {
        None => 32,
        Some(Value::Number(n, 4)) if n.is_power_of_two() => *n,
        _ => return Err("invalid GGUF alignment".into()),
    };
    let end = file_bytes - r.remaining;
    let data_offset = end
        .checked_add(alignment - 1)
        .ok_or("GGUF alignment overflow")?
        / alignment
        * alignment;
    if tensors
        .values()
        .any(|t| t.offset % alignment != 0 || !TYPE_SIZES.iter().any(|(kind, ..)| *kind == t.kind))
    {
        return Err("unaligned or unsupported GGML tensor".into());
    }
    // A directory with no tensor has no data section, so its rounded offset
    // may sit past the end without anything being missing.
    if data_offset > file_bytes && !tensors.is_empty() {
        return Err("GGUF data offset exceeds the file".into());
    }
    Ok(Directory {
        values,
        tensors,
        data_offset,
        file_bytes,
    })
}

#[cfg(test)]
mod tests;
