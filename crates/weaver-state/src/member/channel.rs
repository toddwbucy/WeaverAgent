//! Bounded first-door framing. Sending owns the seam until all bytes are carried.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use weaver_types::state_flow::STATE_REQUEST_LINE_BOUND;

pub(super) struct Channel {
    stream: UnixStream,
    buffered: Vec<u8>,
}

impl Channel {
    pub(super) fn new(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            buffered: Vec::new(),
        })
    }

    pub(super) fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            if let Some(end) = self.buffered.iter().position(|b| *b == b'\n') {
                if end >= STATE_REQUEST_LINE_BOUND {
                    return Err(invalid("request line exceeds FRAME_BOUND"));
                }
                return Ok(Some(self.buffered.drain(..=end).collect()));
            }
            if self.buffered.len() >= STATE_REQUEST_LINE_BOUND {
                return Err(invalid("request line exceeds FRAME_BOUND"));
            }
            let mut chunk = [0u8; 65536];
            let space = chunk
                .len()
                .min(STATE_REQUEST_LINE_BOUND - self.buffered.len());
            match self.stream.read(&mut chunk[..space]) {
                Ok(0) if self.buffered.is_empty() => return Ok(None),
                Ok(0) => return Err(invalid("EOF inside a request")),
                Ok(n) => self.buffered.extend_from_slice(&chunk[..n]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait(&self.stream, nix::poll::PollFlags::POLLIN, None)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// No receiving operation is reachable while these bytes are retained.
    pub(super) fn send(&mut self, bytes: Vec<u8>) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(crate::RESPOND_WAIT_MS));
        let mut answer = PendingAnswer { bytes, offset: 0 };
        loop {
            if answer.advance(&mut self.stream)? {
                return Ok(());
            }
            wait(&self.stream, nix::poll::PollFlags::POLLOUT, Some(deadline))?;
        }
    }
}

fn wait(
    stream: &UnixStream,
    interest: nix::poll::PollFlags,
    deadline: Option<Instant>,
) -> io::Result<()> {
    loop {
        let timeout = match deadline {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "answer carry expired",
                    ));
                }
                nix::poll::PollTimeout::from(left.as_millis().clamp(1, u128::from(u16::MAX)) as u16)
            }
            None => nix::poll::PollTimeout::NONE,
        };
        let mut fds = [nix::poll::PollFd::new(stream.as_fd(), interest)];
        match nix::poll::poll(&mut fds, timeout) {
            Ok(0) => continue,
            Ok(_) => return Ok(()),
            Err(nix::errno::Errno::EINTR) => continue,
            Err(error) => return Err(io::Error::from(error)),
        }
    }
}

struct PendingAnswer {
    bytes: Vec<u8>,
    offset: usize,
}

impl PendingAnswer {
    fn advance(&mut self, writer: &mut impl Write) -> io::Result<bool> {
        while self.offset < self.bytes.len() {
            match writer.write(&self.bytes[self.offset..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "answer carry stopped",
                    ));
                }
                Ok(n) => self.offset += n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }
}

pub(super) fn invalid(detail: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fragmented {
        bytes: Vec<u8>,
        block: bool,
    }
    impl Write for Fragmented {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.block = !self.block;
            if self.block {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let n = 3.min(bytes.len());
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn partial_snapshot_answer_retains_its_finished_observation_without_resampling() {
        use crate::member::{Reply, complete, observation::OperationSample, tests::timing, wrap};
        use weaver_types::state_flow::*;
        timing::enable();
        let mut sample =
            OperationSample::begin(2.into(), 2.into(), StateOperation::Snapshot).unwrap();
        sample.phase_begin(PhaseName::Validate).unwrap();
        sample.phase_end(PhaseName::Validate).unwrap();
        sample.capture_begin().unwrap();
        sample.capture_end().unwrap();
        let (observation, payload) = complete(
            Reply::Control(StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Capture(CaptureRefusalCode::SnapshotFailed),
                detail: "synthetic serialization refusal".into(),
            }),
            sample,
            None,
        )
        .unwrap();
        let expected = wrap(
            &Common {
                session_id: "session".into(),
                run_id: "run".into(),
                request_id: 2.into(),
            },
            "control_answer",
            &payload,
            &observation,
        )
        .unwrap();
        let mut answer = PendingAnswer {
            bytes: expected.clone(),
            offset: 0,
        };
        let mut writer = Fragmented {
            bytes: Vec::new(),
            block: false,
        };
        let mut pending = 0;
        while !answer.advance(&mut writer).unwrap() {
            pending += 1;
            assert_eq!(timing::resource_sample_count(), 2);
        }
        assert!(pending > 1);
        assert_eq!(writer.bytes, expected);
        let decoded = decode_state_line(&writer.bytes, FrameDirection::Response).unwrap();
        assert_eq!(decoded.observation(), Some(&observation));
        assert_eq!(timing::resource_sample_count(), 2);
        timing::disable();
    }

    #[test]
    fn partial_answer_resumes_the_exact_bytes_without_restarting() {
        let expected = b"one complete answer\n".to_vec();
        let mut answer = PendingAnswer {
            bytes: expected.clone(),
            offset: 0,
        };
        let mut writer = Fragmented {
            bytes: Vec::new(),
            block: false,
        };
        while !answer.advance(&mut writer).unwrap() {}
        assert_eq!(writer.bytes, expected);
        assert_eq!(answer.offset, expected.len());
        assert!(answer.advance(&mut writer).unwrap());
        assert_eq!(writer.bytes, expected);
    }
}
