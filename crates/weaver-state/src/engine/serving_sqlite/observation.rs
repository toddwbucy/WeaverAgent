//! Member-owned operation metadata. These transactions never touch content or
//! move the source frontier, and finish before a successful wrapper can be sent.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use weaver_types::state_flow::{
    MemberObservationIdentity, StateOperation, StateOperationObservation,
    StoredOperationObservation,
};

use super::ServingSqlite;
use crate::ReadFault;

impl ServingSqlite {
    /// Establish the member's once-sampled identity and its open observation in
    /// one RAM transaction. A duplicate run cannot acquire a second identity.
    /// The member closes on failure, without sending an adoption success.
    pub fn begin_observations(
        &mut self,
        identity: &MemberObservationIdentity,
        observation: &StateOperationObservation,
    ) -> Result<(), ReadFault> {
        identity.validate().map_err(fault)?;
        observation.validate_shape().map_err(fault)?;
        if observation.operation != StateOperation::Open
            || observation.operation_id.0 != 1
            || observation.request_id.0 != 1
        {
            return Err(fault(
                "the first observation must be open with operation and request IDs one",
            ));
        }
        let identity = serde_json::to_string(identity).map_err(fault)?;
        let record = StoredOperationObservation {
            request_source: None,
            observation: observation.clone(),
        };
        let tx = self.connection.transaction().map_err(fault)?;
        tx.execute(
            "INSERT INTO observation_run(run,member_observation) VALUES(?1,?2)",
            params![self.context.run_id, identity],
        )
        .map_err(fault)?;
        insert(&tx, &self.context.run_id, &record)?;
        tx.commit().map_err(fault)
    }

    /// Record exactly the completed operation that will travel in its response.
    /// This is internal bookkeeping after content commit, not another filing.
    /// In particular a failure cannot turn an uncertain content commit into a
    /// successful receipt or cause its request to be retried.
    pub fn record_observation(
        &mut self,
        record: &StoredOperationObservation,
    ) -> Result<(), ReadFault> {
        record.validate().map_err(fault)?;
        if record.observation.operation == StateOperation::Open {
            return Err(fault("a later operation cannot open the member again"));
        }
        let tx = self.connection.transaction().map_err(fault)?;
        let previous = last_operation(&tx, &self.context.run_id)?
            .ok_or_else(|| fault("no open observation exists for this member run"))?;
        if previous.checked_add(1) != Some(record.observation.operation_id.0) {
            return Err(fault("operation ID is repeated, skipped or exhausted"));
        }
        insert(&tx, &self.context.run_id, record)?;
        tx.commit().map_err(fault)
    }
}

fn last_operation(connection: &Connection, run: &str) -> Result<Option<u64>, ReadFault> {
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT operation_id FROM operation_observation
             WHERE run=?1 ORDER BY operation_id DESC LIMIT 1",
            [run],
            |row| row.get(0),
        )
        .optional()
        .map_err(fault)?;
    bytes
        .map(|bytes| {
            let value: [u8; 8] = bytes
                .try_into()
                .map_err(|_| fault("stored operation ID is not an eight-byte counter"))?;
            Ok(u64::from_be_bytes(value))
        })
        .transpose()
}

fn insert(
    tx: &Transaction<'_>,
    run: &str,
    record: &StoredOperationObservation,
) -> Result<(), ReadFault> {
    let observation = &record.observation;
    let operation = serde_json::to_string(&observation.operation).map_err(fault)?;
    // StateOperation is a closed string enum. Its JSON string is ASCII without
    // escapes, so the inner bytes are the catalog's exact operation spelling.
    let operation = &operation[1..operation.len() - 1];
    let encoded = serde_json::to_string(observation).map_err(fault)?;
    let source = record.request_source.as_ref();
    tx.execute(
        "INSERT INTO operation_observation
         (run,operation_id,request_id,operation,source_run,source_sequence,
          source_block,observation) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            run,
            observation.operation_id.0.to_be_bytes().as_slice(),
            observation.request_id.0.to_be_bytes().as_slice(),
            operation,
            source.map(|source| source.run.as_str()),
            source.map(|source| source.sequence.0.to_be_bytes()),
            source.map(|source| source.block.0.to_be_bytes()),
            encoded,
        ],
    )
    .map_err(fault)?;
    Ok(())
}

fn fault(detail: impl std::fmt::Display) -> ReadFault {
    ReadFault::ComponentFailure {
        detail: format!("operation bookkeeping failed: {detail}"),
    }
}

#[cfg(test)]
mod tests;
