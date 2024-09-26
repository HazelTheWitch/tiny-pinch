use std::{collections::{HashMap, HashSet}, fs::{self, File}, io::{BufWriter, Write}, mem::transmute, path::{Path, PathBuf}, slice::from_raw_parts};

use anyhow::anyhow;
use bevy_ecs::{schedule::{BoxedCondition, NodeId, Schedule}, world::{World, WorldId}};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use retour::static_detour;
use tracing::{error, info};

use crate::{offset::{exe_space, transmute_functon}, Arguments};

const RUN_SCHEDULE_OFFSET: isize = exe_space(0x14082bb30);
 
static INJECT_INSTANT: Mutex<Option<DateTime<Utc>>> = Mutex::new(None);
static ARGUMENTS: Mutex<Option<Arguments>> = Mutex::new(None);
static RAN: Lazy<Mutex<HashMap<WorldId, HashSet<String>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static STARTUP_SYSTEMS: &[&str] = &["PreStartup", "Startup", "PostStartup"];

static_detour! {
    static RunSchedule: extern "cdecl" fn(*mut Schedule, *mut World);
}

pub fn dump_path() -> PathBuf {
    match INJECT_INSTANT.lock().as_ref() {
        Some(instant) => PathBuf::from(instant.naive_utc().format("%Y-%m-%d-%H-%M-%S").to_string()),
        None => PathBuf::from("dump"),
    }
}

pub fn run_schedule(schedule: *mut Schedule, world: *mut World) {
    let schedule = unsafe { &mut *schedule };
    let world = unsafe { &mut *world };

    RunSchedule.call(schedule, world);

    let schedule_id = format!("{:?}", schedule.label());

    if STARTUP_SYSTEMS.contains(&schedule_id.as_str()) {
        if let Err(err) = dump(world, schedule, &schedule_id) {
            error!("Failed to dump world: {err}");
        }
    }

    let ready = if let (Some(instant), Some(args)) = (INJECT_INSTANT.lock().as_ref(), ARGUMENTS.lock().as_ref()) {
        let millis = Utc::now().signed_duration_since(instant).num_milliseconds();

        millis >= (args.delay * 1000.) as i64
    } else {
        false
    };

    if ready {
        let mut ran = RAN.lock();

        let world_ran = ran.entry(world.id()).or_default();

        if !world_ran.contains(&schedule_id) {
            if let Err(err) = dump(world, schedule, &schedule_id) {
                error!("Error dumping world: {err}");
            }

            world_ran.insert(schedule_id);
        }
    }
}

pub fn dump(world: &World, schedule: &mut Schedule, label: &str) -> anyhow::Result<()> {
    let world_id = world.id();

    let world_usize: usize = unsafe {
        transmute(world_id)
    };

    let dump_base_path = dump_path().join(format!("{world_usize}")).join(label);

    fs::create_dir_all(&dump_base_path)?;

    info!("Dumping {label} : {world_id:?}");

    dump_resources(world, &dump_base_path)?;
    dump_archetypes(world, &dump_base_path)?;
    dump_schedule(schedule, &dump_base_path)?;

    Ok(())
}

fn dump_resources(world: &World, path: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut file = BufWriter::new(File::create(path.as_ref().join("resources.txt"))?);

    for (info, ptr) in world.iter_resources() {
        let size = info.layout().size();

        writeln!(file, "{} ({})", info.name(), size)?;
        
        let data = unsafe {
            from_raw_parts(ptr.as_ptr(), size)
        };

        hxdmp::hexdump(data, &mut file)?;

        writeln!(file, "\n")?;
    }

    Ok(())
}

fn dump_archetypes(world: &World, path: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut file = BufWriter::new(File::create(path.as_ref().join("archetypes.txt"))?);

    let check = |x: bool| if x { "+" } else { "-" };

    let components = world.components();

    for archetype in world.archetypes().iter() {
        writeln!(file, "Archetype: {} ({} entities)", archetype.id().index(), archetype.len())?;

        writeln!(file, "  A I R")?;
        writeln!(
            file,
            "H {} {} {}",
            check(archetype.has_add_hook()),
            check(archetype.has_insert_hook()),
            check(archetype.has_remove_hook()),
        )?;
        writeln!(
            file,
            "O {} {} {}",
            check(archetype.has_add_observer()),
            check(archetype.has_insert_observer()),
            check(archetype.has_remove_observer()),
        )?;

        for component in archetype.table_components() {
            let component_info = components.get_info(component).ok_or(anyhow!("Component does not exist in world"))?;
            let size = component_info.layout().size();

            writeln!(file, "T {} ({})", component_info.name(), size)?;
        }

        for component in archetype.sparse_set_components() {
            let component_info = components.get_info(component).ok_or(anyhow!("Component does not exist in world"))?;
            let size = component_info.layout().size();

            writeln!(file, "S {} ({})", component_info.name(), size)?;
        }

        writeln!(file)?;
    }

    Ok(())
}

fn dump_schedule(schedule: &mut Schedule, path: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut file = BufWriter::new(File::create(path.as_ref().join("schedule.txt"))?);

    writeln!(file, "Schedule: {:?}", schedule.label())?;

    let graph = schedule.graph();

    let mut conditions: HashMap<NodeId, &[BoxedCondition]> = Default::default();

    conditions.extend(
        graph
            .systems()
            .map(|(id, _, c)| (id, c))
    );

    conditions.extend(
        graph
            .system_sets()
            .map(|(id, _, c)| (id, c))
    );

    let order = graph.dependency().cached_topsort();

    for node in order {
        if let Some(system) = graph.get_system_at(*node) {
            if let NodeId::System(id) = *node {
                writeln!(file, "{id} {}", system.name())?;
            }
        }

        if let Some(set) = graph.get_set_at(*node) {
            if let NodeId::Set(id) = *node {
                let anonymous = if set.is_anonymous() {
                    "A"
                } else {
                    " "
                };
    
                writeln!(file, "{id} {anonymous} {set:?}")?;
            }
        }

        if let Some(conditions) = conditions.get(node) {
            for condition in conditions.into_iter() {
                writeln!(file, "   {}", condition.name())?;
            }
        }
    }

    Ok(())
}

pub fn operate(args: Arguments) -> anyhow::Result<()> {
    INJECT_INSTANT.lock().replace(Utc::now());
    ARGUMENTS.lock().replace(args);

    info!("Dumping to: {:?}", dump_path());

    unsafe  {
        RunSchedule.initialize(
            transmute_functon(RUN_SCHEDULE_OFFSET),
            run_schedule,
        )?
        .enable()?;
    }

    Ok(())
}
