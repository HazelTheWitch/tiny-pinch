use std::path::Path;

use anyhow::anyhow;
use tiny_pinch_common::pdb_utils::{find_offset, load_pdb};

pub struct Offsets {
    pub run_schedule: isize,
}

pub fn find_offsets(pdb_path: impl AsRef<Path>) -> anyhow::Result<Offsets> {
    let pdb = load_pdb(pdb_path.as_ref())?;

    let run_schedule = find_offset(|name| name.starts_with("bevy_ecs::schedule::schedule::Schedule::run"), &pdb)?
        .ok_or(anyhow!("could not find offset: bevy_ecs::schedule::schedule::Schedule::run"))?;

    Ok(Offsets { run_schedule })
}
