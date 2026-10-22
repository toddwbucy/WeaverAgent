//! Context admission for read-backed material; conversion has no I/O continuation.

use super::*;
use crate::flow::material::{self, HeldMaterial, MaterialError, MaterialFailure};

impl RecordingContext {
    pub(in crate::flow) fn hold_material(
        &mut self,
        read: RecordedRead,
        io: &mut impl PumpIo,
    ) -> Result<HeldMaterial, MaterialError> {
        if let Err(failure) = self.live(io) {
            return Err(MaterialError {
                kind: MaterialFailure::Unavailable,
                reads: vec![read],
                failure: Some(failure),
            });
        }
        if !read.belongs_to(self) {
            return Err(self.material_error(
                MaterialError {
                    kind: MaterialFailure::ForeignContext,
                    reads: vec![read],
                    failure: None,
                },
                io,
            ));
        }
        material::from_read(read).map_err(|error| self.material_error(error, io))
    }

    pub(in crate::flow) fn hold_pair(
        &mut self,
        read: RecordedRead,
        entry: usize,
        pair: usize,
        statement: RecordedRead,
        io: &mut impl PumpIo,
    ) -> Result<HeldMaterial, MaterialError> {
        if let Err(failure) = self.live(io) {
            return Err(MaterialError {
                kind: MaterialFailure::Unavailable,
                reads: vec![read, statement],
                failure: Some(failure),
            });
        }
        if !read.belongs_to(self) || !statement.belongs_to(self) {
            return Err(self.material_error(
                MaterialError {
                    kind: MaterialFailure::ForeignContext,
                    reads: vec![read, statement],
                    failure: None,
                },
                io,
            ));
        }
        material::from_pair(read, entry, pair, statement)
            .map_err(|error| self.material_error(error, io))
    }

    fn material_error(&mut self, mut error: MaterialError, io: &mut impl PumpIo) -> MaterialError {
        let detail = match error.kind {
            MaterialFailure::ForeignContext => {
                Some("material read belongs to another recording context")
            }
            MaterialFailure::Invalid(detail) => Some(detail),
            _ => None,
        };
        if let Some(detail) = detail {
            error.failure = Some(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::StateReply,
                detail,
            ));
        }
        error
    }
}
