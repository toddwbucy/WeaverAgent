//! One capture identity and immutable room image, with explicit timeout cleanup.

use super::{
    channel::invalid,
    observation::OperationSample,
    room::{Room, WriteFault},
};
use std::io;
use weaver_state::engine::serving_sqlite::ServingSqlite;
use weaver_types::state_flow::*;

#[derive(Default)]
pub(super) enum Capture {
    #[default]
    None,
    Refused {
        request: DecimalU64,
        id: DecimalU64,
    },
    Held {
        request: DecimalU64,
        id: DecimalU64,
        name: String,
    },
    Finished {
        request: DecimalU64,
        id: DecimalU64,
        name: String,
        _admin_hash: Digest,
    },
}

impl Capture {
    pub(super) fn held(&self) -> bool {
        matches!(self, Self::Held { .. } | Self::Finished { .. })
    }
    pub(super) fn continuation(&self) -> Option<DecimalU64> {
        match self {
            Self::None => None,
            Self::Refused { request, .. }
            | Self::Held { request, .. }
            | Self::Finished { request, .. } => Some(*request),
        }
    }
    pub(super) fn allows_continuation(&self, frame: &StateControl) -> bool {
        match self {
            Self::None => false,
            Self::Refused { .. } => matches!(frame, StateControl::Abort { .. }),
            Self::Held { .. } | Self::Finished { .. } => matches!(
                frame,
                StateControl::Acknowledge { .. } | StateControl::Abort { .. }
            ),
        }
    }
    pub(super) fn fresh(&mut self) {
        if matches!(self, Self::Refused { .. }) {
            *self = Self::None;
        }
    }

    pub(super) fn snapshot(
        &mut self,
        store: &ServingSqlite,
        room: &Room,
        request: DecimalU64,
        id: DecimalU64,
        sample: &mut OperationSample,
    ) -> io::Result<StateControlAnswer> {
        if self.held() {
            return Ok(refused(
                CaptureRefusalCode::InvalidCaptureState,
                "capture is held",
            ));
        }
        if id.0 == 0 {
            return Ok(refused(
                CaptureRefusalCode::UnknownCapture,
                "zero capture ID",
            ));
        }
        sample.phase_begin(PhaseName::Capture)?;
        let image = store.image();
        sample.phase_end(PhaseName::Capture)?;
        let image = match image {
            Ok(image) => image,
            Err(error) => {
                *self = Self::Refused { request, id };
                return Ok(refused(error.code, &error.detail));
            }
        };
        let position = store.position().map_err(|e| invalid(e.to_string()))?;
        sample.phase_begin(PhaseName::Filesystem)?;
        let written = room.write_part(id, &image);
        let name = match written {
            Ok(name) => name,
            Err(WriteFault::Cleanup(error)) => return Err(error),
            Err(WriteFault::Refused(error)) => {
                room.clear_remnants()?;
                sample.phase_end(PhaseName::Filesystem)?;
                *self = Self::Refused { request, id };
                return Ok(refused(
                    CaptureRefusalCode::SnapshotFailed,
                    &error.to_string(),
                ));
            }
        };
        sample.phase_end(PhaseName::Filesystem)?;
        let size_bytes = (image.len() as u64).into();
        *self = Self::Held {
            request,
            id,
            name: name.clone(),
        };
        Ok(StateControlAnswer::Snapshot {
            capture_id: id,
            room_name: name,
            size_bytes,
            position,
        })
    }

    pub(super) fn control(
        &mut self,
        room: &Room,
        request: DecimalU64,
        frame: &StateControl,
        sample: &mut OperationSample,
    ) -> io::Result<StateControlAnswer> {
        match (&*self, frame) {
            (
                Self::Held {
                    request: held, id, ..
                },
                StateControl::Acknowledge {
                    capture_id,
                    dump_sha256,
                },
            ) if *held == request && id == capture_id => {
                let (held, id) = (*held, *id);
                sample.phase_begin(PhaseName::Filesystem)?;
                let name = room.finish(id)?;
                sample.phase_end(PhaseName::Filesystem)?;
                *self = Self::Finished {
                    request: held,
                    id,
                    name: name.clone(),
                    _admin_hash: dump_sha256.clone(),
                };
                Ok(StateControlAnswer::Finished {
                    capture_id: id,
                    room_name: name,
                })
            }
            (
                Self::Held {
                    request: held,
                    id,
                    name,
                }
                | Self::Finished {
                    request: held,
                    id,
                    name,
                    ..
                },
                StateControl::Abort { capture_id },
            ) if *held == request && id == capture_id => {
                let id = *id;
                sample.phase_begin(PhaseName::Filesystem)?;
                room.remove(name)?;
                sample.phase_end(PhaseName::Filesystem)?;
                *self = Self::None;
                Ok(StateControlAnswer::Aborted { capture_id: id })
            }
            (Self::Refused { request: held, id }, StateControl::Abort { capture_id })
                if *held == request && id == capture_id =>
            {
                let id = *id;
                *self = Self::None;
                Ok(StateControlAnswer::Aborted { capture_id: id })
            }
            (
                Self::Finished {
                    request: original,
                    id,
                    name,
                    ..
                },
                StateControl::ReleaseCapture { capture_id },
            ) if original != &request && id == capture_id => {
                let id = *id;
                sample.phase_begin(PhaseName::Filesystem)?;
                room.remove(name)?;
                sample.phase_end(PhaseName::Filesystem)?;
                *self = Self::None;
                Ok(StateControlAnswer::CaptureReleased { capture_id: id })
            }
            (Self::None | Self::Refused { .. }, _) => Ok(refused(
                CaptureRefusalCode::UnknownCapture,
                "no matching capture",
            )),
            _ => Ok(refused(
                CaptureRefusalCode::InvalidCaptureState,
                "control is not allowed in this capture state",
            )),
        }
    }
}

fn refused(code: CaptureRefusalCode, detail: &str) -> StateControlAnswer {
    StateControlAnswer::ControlRefused {
        code: ControlRefusalCode::Capture(code),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests;
