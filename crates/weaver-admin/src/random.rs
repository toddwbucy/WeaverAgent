//! Internal identifiers use the operating system's random source.

use std::io::Read;

/// Render exactly `n` random bytes as lowercase hex, or preserve the I/O error.
pub(crate) fn random_hex(n: usize) -> std::io::Result<String> {
    let mut bytes = vec![0_u8; n];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
