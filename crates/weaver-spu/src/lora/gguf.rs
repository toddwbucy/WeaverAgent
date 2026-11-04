//! Metadata and tensor-directory checks for LORA-2. No tensor data is loaded.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone)]
pub(super) enum Value {
    Text(String),
    Number(u64, u32),
    Float(f32),
    Other,
}

#[derive(Debug, Clone)]
pub(super) struct Tensor {
    pub dims: [u64; 4],
    pub kind: u32,
    pub offset: u64,
}

#[derive(Debug)]
pub(super) struct Directory {
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
        self.input
            .seek(SeekFrom::Current(count))
            .map_err(|e| e.to_string())?;
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

pub(super) fn read(path: &Path) -> Result<Directory, String> {
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
        .any(|t| t.offset % alignment != 0 || !matches!(t.kind, 0..=3 | 6..=30 | 34..=35 | 39..=41))
    {
        return Err("unaligned or unsupported GGML tensor".into());
    }
    if data_offset > file_bytes {
        return Err("GGUF data offset exceeds the file".into());
    }
    Ok(Directory {
        values,
        tensors,
        data_offset,
        file_bytes,
    })
}
