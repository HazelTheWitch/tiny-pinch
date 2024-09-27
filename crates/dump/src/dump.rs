use std::{collections::{HashMap, HashSet}, fs::{self, File}, io::{BufWriter, Write}, mem::transmute, path::{Path, PathBuf}, slice::from_raw_parts};

use anyhow::anyhow;
use bevy_ecs::{archetype::{Archetype, ArchetypeComponentId, ArchetypeId, Archetypes}, component::ComponentId, schedule::{BoxedCondition, NodeId, Schedule}, storage::Storages, world::{World, WorldId}};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use retour::static_detour;
use tracing::{error, info, warn};
use tiny_pinch_common::{exe_space, transmute_functon};

use crate::Arguments;
 
static INJECT_INSTANT: Mutex<Option<DateTime<Utc>>> = Mutex::new(None);
static ARGUMENTS: Mutex<Option<Arguments>> = Mutex::new(None);
static RAN: Lazy<Mutex<HashMap<WorldId, HashSet<String>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static STARTUP_SYSTEMS: &[&str] = &["PreStartup", "Startup", "PostStartup"];

/// bevy_ecs::schedule::schedule::Schedule::run
const RUN_SCHEDULE_OFFSET: isize = exe_space(0x14082bb30);

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
    dump_systems(world, schedule, &dump_base_path)?;

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

fn dump_schedule(schedule: &Schedule, path: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut file = BufWriter::new(File::create(path.as_ref().join("schedule.txt"))?);

    let executor_kind = schedule.get_executor_kind();

    writeln!(file, "Schedule: {:?} ({executor_kind:?})", schedule.label())?;

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

fn dump_systems(world: &World, schedule: &Schedule, path: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut file = BufWriter::new(File::create(path.as_ref().join("systems.txt"))?);

    let components = world.components();
    let archetypes = world.archetypes();

    let flag = |b: bool, f: &'static str| if b { f } else { " " };

    for system in &schedule.executable.systems {
        writeln!(file, "{} {} {} {}", system.name(), flag(system.is_exclusive(), "E"), flag(system.is_send(), "S"), flag(system.has_deferred(), "D"))?;

        let archetype_access = system.archetype_component_access();
        let storages = world.storages();

        for id in archetype_access.reads() {
            let Some(usage) = get_archetype_id_usage(&id, archetypes, storages) else {
                warn!("Could not get archetype usage");
                continue;
            };

            let component_id = usage.component_id();

            let Some(info) = components.get_info(component_id) else {
                warn!("Could not get component info for {component_id:?}");
                continue;
            };

            let flag = usage.flag();

            if let ArchetypeIdUsage::Component(_, archetype_id) = usage {
                writeln!(file, "{flag} R {} {}", info.name(), archetype_id.index())?;
            } else {
                writeln!(file, "{flag} R {}", info.name())?;
            }
        }

        for id in archetype_access.writes() {
            let Some(usage) = get_archetype_id_usage(&id, archetypes, storages) else {
                warn!("Could not get archetype usage");
                continue;
            };

            let component_id = usage.component_id();

            let Some(info) = components.get_info(component_id) else {
                warn!("Could not get component info for {component_id:?}");
                continue;
            };

            let flag = usage.flag();

            if let ArchetypeIdUsage::Component(_, archetype_id) = usage {
                writeln!(file, "{flag} W {} {}", info.name(), archetype_id.index())?;
            } else {   
                writeln!(file, "{flag} W {}", info.name())?;
            }
        }

        writeln!(file)?;
    }

    Ok(())
}

enum ArchetypeIdUsage {
    Component(ComponentId, ArchetypeId),
    Resource(ComponentId),
    NonSendResource(ComponentId),
}

impl ArchetypeIdUsage {
    pub fn component_id(&self) -> ComponentId {
        match self {
            ArchetypeIdUsage::Component(component_id, _) => *component_id,
            ArchetypeIdUsage::Resource(component_id) => *component_id,
            ArchetypeIdUsage::NonSendResource(component_id) => *component_id,
        }
    }

    pub fn flag(&self) -> &'static str {
        match self {
            ArchetypeIdUsage::Component(_, _) => "C",
            ArchetypeIdUsage::Resource(_) => "R",
            ArchetypeIdUsage::NonSendResource(_) => "N",
        }
    }
}

fn get_archetype_id_usage(id: &ArchetypeComponentId, archetypes: &Archetypes, storages: &Storages) -> Option<ArchetypeIdUsage> {
    if let Some((component_id, archetype)) = archetypes
        .iter()
        .find_map(|archetype| lookup_archetype_component_id(archetype, &id).map(|id| (id, archetype)))
    {
        return Some(ArchetypeIdUsage::Component(component_id, archetype.id))
    }

    if let Some(component_id) = storages
        .resources
        .iter()
        .find_map(|(component_id, resource_data)| {
            if resource_data.id() == *id {
                Some(component_id)
            } else {
                None
            }
        }) {
        return Some(ArchetypeIdUsage::Resource(component_id));
    }

    if let Some(component_id) = storages
        .non_send_resources
        .iter()
        .find_map(|(component_id, resource_data)| {
            if resource_data.id() == *id {
                Some(component_id)
            } else {
                None
            }
        }) {
        return Some(ArchetypeIdUsage::NonSendResource(component_id));
    }

    None
}

fn lookup_archetype_component_id(archetype: &Archetype, id: &ArchetypeComponentId) -> Option<ComponentId> {
    for (component_id, archetype_component_info) in archetype.components.iter() {
        if archetype_component_info.archetype_component_id == *id {
            return Some(*component_id);
        }
    }

    None
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
