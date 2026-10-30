//! The existing codecs retain their frame while a lifecycle owner services
//! management at each channel wait, including between decode segments.
use super::*;
use nix::poll::PollFlags;

pub(crate) type Wait<'a> = dyn FnMut(BorrowedFd<'_>, PollFlags) -> Result<(), ChannelFault> + 'a;

/// One atomic packet attempt. Pending retains the caller's exact bytes.
pub(crate) fn try_send(end: BorrowedFd<'_>, bytes: &[u8]) -> Result<bool, ChannelFault> {
    match nix::sys::socket::send(
        end.as_raw_fd(),
        bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
    ) {
        Ok(count) if count == bytes.len() => Ok(true),
        Err(nix::errno::Errno::EAGAIN | nix::errno::Errno::EINTR) => Ok(false),
        _ => Err(ChannelFault::Closed),
    }
}

fn send(end: BorrowedFd<'_>, bytes: &[u8], wait: &mut Wait<'_>) -> Result<(), ChannelFault> {
    loop {
        wait(end, PollFlags::POLLOUT)?;
        if try_send(end, bytes)? {
            return Ok(());
        }
    }
}

impl OrganChannel {
    pub(crate) fn send_managed(
        &self,
        envelope: &OrganEnvelope,
        wait: &mut Wait<'_>,
    ) -> Result<(), ChannelFault> {
        send(self.end.as_fd(), &self.encode(envelope)?, wait)
    }

    pub(crate) fn recv_managed(&self, wait: &mut Wait<'_>) -> Result<OrganEnvelope, ChannelFault> {
        wait(self.end.as_fd(), PollFlags::POLLIN)?;
        self.recv()
    }
}

impl DecodeChannel {
    /// The entered replay can abandon an unsent request, but a segment series
    /// whose first packet crossed must finish before a matching Cancel follows.
    pub(crate) fn send_replay_managed(
        &self,
        directive: &weaver_types::TokenDirective,
        wait: &mut dyn FnMut(BorrowedFd<'_>, PollFlags, bool) -> Result<(), ChannelFault>,
    ) -> Result<(), ChannelFault> {
        let mut started = false;
        send_decode(directive, |bytes| {
            loop {
                wait(self.end.as_fd(), PollFlags::POLLOUT, started)?;
                if try_send(self.end.as_fd(), bytes)? {
                    started = true;
                    return Ok(());
                }
            }
        })
    }

    pub(crate) fn send_managed(
        &self,
        directive: &weaver_types::TokenDirective,
        wait: &mut Wait<'_>,
    ) -> Result<(), ChannelFault> {
        send_decode(directive, |bytes| send(self.end.as_fd(), bytes, wait))
    }

    pub(crate) fn recv_managed(&self, wait: &mut Wait<'_>) -> Result<DecodeReply, ChannelFault> {
        receive_decode(|| {
            wait(self.end.as_fd(), PollFlags::POLLIN)?;
            recv_octets(self.end.as_fd())
        })
    }
}

impl ClassifyChannel {
    pub(crate) fn recv_managed(&self, wait: &mut Wait<'_>) -> Result<ClassifyReply, ChannelFault> {
        wait(self.end.as_fd(), PollFlags::POLLIN)?;
        self.recv_reply()
    }
}
