//! One certified pre-agent seed, held only while admin copies its raw room image.

use super::{
    channel::{Channel, invalid},
    room::{Room, WriteFault},
};
use std::io;
use weaver_state::engine::serving_sqlite::seed_image;
use weaver_types::state_flow::*;

pub(super) fn run(channel: &mut Channel, room: &Room) -> io::Result<()> {
    let Some(line) = channel.receive()? else {
        return Ok(());
    };
    let SeedFrame::Seed { request } =
        decode_seed_line(&line, FrameDirection::Request).map_err(|e| invalid(e.to_string()))?
    else {
        return Err(invalid("seed mode requires its one seed request"));
    };
    let image = match seed_image(&request) {
        Ok(image) => image,
        Err(error) => return refuse(channel, error.code, error.detail),
    };
    let name = match room.write_seed(&request.seed_id, image.as_bytes()) {
        Ok(name) => name,
        Err(WriteFault::Cleanup(error)) => return Err(error),
        Err(WriteFault::Refused(error)) => {
            return refuse(channel, SeedRefusal::SnapshotFailed, error.to_string());
        }
    };
    let created = SeedFrame::Created {
        created: SeedCreated {
            schema_id: request.schema_id.clone(),
            seed_id: request.seed_id.clone(),
            room_name: name.clone(),
            size_bytes: (image.as_bytes().len() as u64).into(),
        },
    };
    drop(image);
    channel.send(
        encode_seed_line(&created, FrameDirection::Response).map_err(|e| invalid(e.to_string()))?,
    )?;
    // A second request is never a second seed. Neither malformed bytes nor EOF
    // can make this private room image a published artifact.
    let terminal = channel.receive();
    room.remove(&name)?;
    terminal.map(|_| ())
}

fn refuse(channel: &mut Channel, code: SeedRefusal, detail: String) -> io::Result<()> {
    let response = SeedFrame::Refused { code, detail };
    channel.send(
        encode_seed_line(&response, FrameDirection::Response)
            .map_err(|e| invalid(e.to_string()))?,
    )
}
