//! The Diagnostic fixtures' child-exit watch. Callers drain captured pipes
//! concurrently and retain their own absolute deadline and assertion context.
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

pub(crate) fn wait_until(
    child: &mut Child,
    deadline: Instant,
) -> std::io::Result<(ExitStatus, bool)> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            child.kill()?;
            return child.wait().map(|status| (status, true));
        }
        std::thread::sleep(remaining.min(Duration::from_millis(1)));
    }
}
