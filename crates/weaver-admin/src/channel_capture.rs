//! The sole admin socket reader and sender for the paired capture projection.
//! The actor retains exchange/phase custody; transport uses its original windows.
use super::{ChannelFault, Coordination, MAX_ENVELOPE_BYTES, poll_cap_ms};
use crate::serving_capture_window::CaptureWindows;
use nix::sys::socket::MsgFlags;
use std::os::fd::AsRawFd;
use weaver_types::state_flow::capture::{
    CaptureDirective, CaptureEnvelope, CapturePayload, CaptureSender,
};

#[derive(Debug)]
pub(crate) struct ReceivedCapture {
    pub(crate) envelope: CaptureEnvelope,
    pub(crate) wire: Vec<u8>,
}

/// A whole packet remains evidence even if decoding crossed its deadline.
/// The actor must judge timing before granting any successful continuation.
pub(crate) struct CaptureRead {
    pub(crate) received: ReceivedCapture,
    pub(crate) timing: Result<(), ChannelFault>,
}

/// Only a socket send can establish peer closure. A failed clock witness uses
/// Closed too, but must never grant permission to continue the exchange.
#[derive(Debug)]
pub(crate) enum CaptureSendFailure {
    Fault(ChannelFault),
    PeerClosed,
}

impl CaptureSendFailure {
    pub(crate) fn channel_fault(self) -> ChannelFault {
        match self {
            Self::Fault(fault) => fault,
            Self::PeerClosed => ChannelFault::Closed,
        }
    }
}

impl From<ChannelFault> for CaptureSendFailure {
    fn from(fault: ChannelFault) -> Self {
        Self::Fault(fault)
    }
}

impl Coordination {
    pub(crate) fn send_capture(
        &self,
        envelope: &CaptureEnvelope,
        windows: &CaptureWindows,
    ) -> Result<(), ChannelFault> {
        self.send_capture_tracked(envelope, windows, &mut false)
    }

    pub(crate) fn send_capture_tracked(
        &self,
        envelope: &CaptureEnvelope,
        windows: &CaptureWindows,
        sent: &mut bool,
    ) -> Result<(), ChannelFault> {
        self.send_capture_attempt(envelope, windows, sent)
            .map_err(CaptureSendFailure::channel_fault)
    }

    pub(crate) fn send_capture_attempt(
        &self,
        envelope: &CaptureEnvelope,
        windows: &CaptureWindows,
        sent: &mut bool,
    ) -> Result<(), CaptureSendFailure> {
        *sent = false;
        if let CapturePayload::Directive(CaptureDirective::SavePoint { window, .. }) =
            &envelope.payload
            && (windows.demand_window() != Some(*window) || windows.capture().is_some())
        {
            return Err(ChannelFault::Undecodable.into());
        }
        windows.check()?;
        let bytes = envelope
            .encode(CaptureSender::Admin)
            .map_err(|_| ChannelFault::Undecodable)?;
        self.send_capture_bytes_tracked(&bytes, windows, sent)
    }

    /// Use the original Leave's graceful or forced window before Beginning.
    pub(crate) fn send_capture_leave(
        &self,
        envelope: &weaver_types::OrganEnvelope,
        windows: &CaptureWindows,
    ) -> Result<(), ChannelFault> {
        if envelope.exchange.opener != weaver_types::Opener::Admin
            || envelope.position != weaver_types::Position::Open
            || !matches!(
                envelope.payload,
                weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                    forced,
                    rollback: false,
                    ..
                }) if windows.matches_leave(forced)
            )
            || windows.capture().is_some()
            || windows.is_join()
        {
            return Err(ChannelFault::Undecodable);
        }
        windows.check()?;
        let bytes = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        self.send_capture_bytes(&bytes, windows)
    }

    pub(crate) fn send_capture_join(
        &self,
        envelope: &weaver_types::OrganEnvelope,
        windows: &CaptureWindows,
    ) -> Result<(), ChannelFault> {
        if !windows.is_join()
            || envelope.exchange.opener != weaver_types::Opener::Admin
            || envelope.position != weaver_types::Position::Open
            || !matches!(
                envelope.payload,
                weaver_types::Payload::Directive(
                    weaver_types::LifecycleDirective::JoinLeave { .. }
                )
            )
        {
            return Err(ChannelFault::Undecodable);
        }
        windows.check()?;
        let bytes = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        self.send_capture_bytes(&bytes, windows)
    }

    fn send_capture_bytes(
        &self,
        bytes: &[u8],
        windows: &CaptureWindows,
    ) -> Result<(), ChannelFault> {
        self.send_capture_bytes_tracked(bytes, windows, &mut false)
            .map_err(CaptureSendFailure::channel_fault)
    }

    fn send_capture_bytes_tracked(
        &self,
        bytes: &[u8],
        windows: &CaptureWindows,
        sent: &mut bool,
    ) -> Result<(), CaptureSendFailure> {
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            }
            .into());
        }
        loop {
            windows.check()?;
            match nix::sys::socket::send(
                self.fd.as_raw_fd(),
                bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(count) if count == bytes.len() => {
                    *sent = true;
                    return windows.check().map_err(CaptureSendFailure::Fault);
                }
                Ok(_) => return Err(ChannelFault::Closed.into()),
                Err(nix::errno::Errno::EINTR) => (),
                Err(nix::errno::Errno::EAGAIN) => self.wait_capture(nix::libc::POLLOUT, windows)?,
                Err(
                    nix::errno::Errno::EPIPE
                    | nix::errno::Errno::ECONNRESET
                    | nix::errno::Errno::ENOTCONN,
                ) => return Err(CaptureSendFailure::PeerClosed),
                Err(_) => return Err(ChannelFault::Closed.into()),
            }
        }
    }

    pub(crate) fn recv_capture(
        &self,
        windows: &CaptureWindows,
    ) -> Result<ReceivedCapture, ChannelFault> {
        self.recv_capture_with(windows, |bytes| {
            CaptureEnvelope::decode(bytes, CaptureSender::Harness)
                .map_err(|_| ChannelFault::Undecodable)
        })
    }

    pub(crate) fn recv_capture_retained(
        &self,
        windows: &CaptureWindows,
    ) -> Result<CaptureRead, ChannelFault> {
        self.recv_capture_retained_with(windows, |bytes| {
            CaptureEnvelope::decode(bytes, CaptureSender::Harness)
                .map_err(|_| ChannelFault::Undecodable)
        })
    }

    /// The same sole reader is probed between publication filesystem steps.
    /// A complete packet remains owned by the actor that judges its phase.
    pub(crate) fn try_recv_capture_retained(
        &self,
        windows: &CaptureWindows,
    ) -> Result<Option<CaptureRead>, ChannelFault> {
        windows.check()?;
        let Some(wire) = self.capture_bytes()? else {
            windows.check()?;
            return Ok(None);
        };
        let envelope = CaptureEnvelope::decode(&wire, CaptureSender::Harness)
            .map_err(|_| ChannelFault::Undecodable)?;
        Ok(Some(CaptureRead {
            received: ReceivedCapture { envelope, wire },
            timing: windows.check(),
        }))
    }

    fn recv_capture_with(
        &self,
        windows: &CaptureWindows,
        decode: impl FnOnce(&[u8]) -> Result<CaptureEnvelope, ChannelFault>,
    ) -> Result<ReceivedCapture, ChannelFault> {
        let read = self.recv_capture_retained_with(windows, decode)?;
        read.timing?;
        Ok(read.received)
    }

    fn recv_capture_retained_with(
        &self,
        windows: &CaptureWindows,
        decode: impl FnOnce(&[u8]) -> Result<CaptureEnvelope, ChannelFault>,
    ) -> Result<CaptureRead, ChannelFault> {
        loop {
            self.wait_capture(nix::libc::POLLIN, windows)?;
            let Some(bytes) = self.capture_bytes()? else {
                continue;
            };
            let envelope = decode(&bytes)?;
            return Ok(CaptureRead {
                received: ReceivedCapture {
                    envelope,
                    wire: bytes,
                },
                timing: windows.check(),
            });
        }
    }

    fn capture_bytes(&self) -> Result<Option<Vec<u8>>, ChannelFault> {
        let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
        let mut slices = [std::io::IoSliceMut::new(&mut bytes)];
        let received = match nix::sys::socket::recvmsg::<()>(
            self.fd.as_raw_fd(),
            &mut slices,
            None,
            MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(received) => received,
            Err(nix::errno::Errno::EAGAIN | nix::errno::Errno::EINTR) => return Ok(None),
            Err(_) => return Err(ChannelFault::Closed),
        };
        // There is no ancillary buffer: the kernel disposes unaccepted rights.
        if received
            .flags
            .intersects(MsgFlags::MSG_TRUNC | MsgFlags::MSG_CTRUNC)
        {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        if received.bytes == 0 {
            return Err(ChannelFault::Closed);
        }
        let count = received.bytes;
        bytes.truncate(count);
        Ok(Some(bytes))
    }

    fn wait_capture(&self, events: i16, windows: &CaptureWindows) -> Result<(), ChannelFault> {
        loop {
            let remaining = windows.remaining()?;
            let millis = remaining.map_or(-1, |remaining| {
                remaining.as_nanos().div_ceil(1_000_000).min(poll_cap_ms()) as i32
            });
            let mut descriptor = nix::libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: the sole Coordination owner holds this valid descriptor.
            let result = unsafe { nix::libc::poll(&mut descriptor, 1, millis) };
            if result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return Err(ChannelFault::Closed);
            }
            windows.check()?;
            if result > 0 {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
#[path = "channel_capture_tests.rs"]
mod tests;
