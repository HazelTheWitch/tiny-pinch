use std::any::type_name;

use bevy_ecs::{system::Resource, world::{Mut, World}};

pub fn get_resource<R: Resource>(world: &World) -> Option<&R> {
    let resource_name = type_name::<R>();

    for (component_id, ptr) in world.iter_resources() {
        if component_id.name() == resource_name {
            return Some(unsafe { ptr.deref() });
        }
    }

    None
}

pub fn get_resource_mut<'w, R: Resource>(world: &'w mut World) -> Option<Mut<'w, R>> {
    let resource_name = type_name::<R>();
    
    for (component_id, ptr) in world.iter_resources_mut() {
        if component_id.name() == resource_name {
            return Some(unsafe { ptr.with_type() });
        }
    }

    None
}