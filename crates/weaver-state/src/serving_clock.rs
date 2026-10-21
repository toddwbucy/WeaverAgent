//! The raw Linux clocks used by the member's operation evidence. Only monotonic
//! wall time is sampled on ordinary state operations.

use std::io;
use weaver_types::state_flow::DecimalU64;

pub fn monotonic_ns() -> io::Result<DecimalU64> {
    clock_ns(nix::libc::CLOCK_MONOTONIC).map(DecimalU64)
}

pub fn thread_cpu_ns() -> io::Result<u64> {
    clock_ns(nix::libc::CLOCK_THREAD_CPUTIME_ID)
}

fn clock_ns(clock: nix::libc::clockid_t) -> io::Result<u64> {
    let mut value = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: value is a live writable timespec for this synchronous call.
    if unsafe { nix::libc::clock_gettime(clock, &mut value) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "clock outside u64 nanoseconds");
    let seconds = u64::try_from(value.tv_sec).map_err(|_| invalid())?;
    let nanos = u64::try_from(value.tv_nsec).map_err(|_| invalid())?;
    if nanos >= 1_000_000_000 {
        return Err(invalid());
    }
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|base| base.checked_add(nanos))
        .ok_or_else(invalid)
}
