//! Context admission for source-backed material; conversion has no I/O continuation.

use super::*;
use crate::flow::material::{
    self, HeldMaterial, MaterialError, MaterialFailure, TaskMaterialError,
};

impl RecordingContext {
    /// Only a completed begin_task can supply this fresh source. A raw seated
    /// input or a cancelled/refused task account has no conversion into it.
    pub(in crate::flow) fn hold_task_material(
        &mut self,
        task: BegunTask,
        io: &mut impl PumpIo,
    ) -> Result<HeldMaterial, TaskMaterialError> {
        if let Err(failure) = self.live(io) {
            return Err(TaskMaterialError {
                kind: MaterialFailure::Unavailable,
                task: Box::new(task),
                failure: Some(failure),
            });
        }
        if !task.belongs_to(self) {
            return Err(self.task_material_error(
                TaskMaterialError {
                    kind: MaterialFailure::ForeignContext,
                    task: Box::new(task),
                    failure: None,
                },
                io,
            ));
        }
        material::from_task(task).map_err(|error| self.task_material_error(error, io))
    }

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
        error.failure = self.material_fault(
            &error.kind,
            "material read belongs to another recording context",
            HaltPoint::StateReply,
            io,
        );
        error
    }

    pub(super) fn task_material_error(
        &mut self,
        mut error: TaskMaterialError,
        io: &mut impl PumpIo,
    ) -> TaskMaterialError {
        // The task producer and context association are harness-owned. Their
        // invariant failures do not attribute a malformed reply to the member.
        error.failure = self.material_fault(
            &error.kind,
            "material task belongs to another recording context",
            HaltPoint::Barrier,
            io,
        );
        error
    }

    fn material_fault(
        &mut self,
        kind: &MaterialFailure,
        foreign: &'static str,
        point: HaltPoint,
        io: &mut impl PumpIo,
    ) -> Option<RunFailure> {
        let detail = match kind {
            MaterialFailure::ForeignContext => Some(foreign),
            MaterialFailure::Invalid(detail) => Some(*detail),
            _ => None,
        };
        detail.map(|detail| self.fail(io, FlowHaltReason::ProtocolFailed, point, detail))
    }
}
