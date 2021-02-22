use common::{
    comp::{BuffChange, ControlEvent, Controller},
    event::{EventBus, ServerEvent},
    metrics::SysMetrics,
    span,
    uid::UidAllocator,
};
use specs::{
    saveload::{Marker, MarkerAllocator},
    shred::ResourceId,
    Entities, Join, Read, ReadExpect, System, SystemData, World, WriteStorage,
};
use vek::*;

#[derive(SystemData)]
pub struct ImmutableData<'a> {
    entities: Entities<'a>,
    uid_allocator: Read<'a, UidAllocator>,
    server_bus: Read<'a, EventBus<ServerEvent>>,
    metrics: ReadExpect<'a, SysMetrics>,
}

pub struct Sys;

impl<'a> System<'a> for Sys {
    type SystemData = (ImmutableData<'a>, WriteStorage<'a, Controller>);

    fn run(&mut self, (immutable_data, mut controllers): Self::SystemData) {
        let start_time = std::time::Instant::now();
        span!(_guard, "run", "controller::Sys::run");
        let mut server_emitter = immutable_data.server_bus.emitter();

        for (entity, controller) in (&immutable_data.entities, &mut controllers).join() {
            let mut inputs = &mut controller.inputs;

            // Note(imbris): I avoided incrementing the duration with inputs.tick() because
            // this is being done manually in voxygen right now so it would double up on
            // speed of time.
            // Perhaphs the duration aspects of inputs could be
            // calculated exclusively on the server (since the client can't be
            // trusted anyway). It needs to be considered if these calculations
            // being on the client are critical for responsiveness/client-side prediction.
            inputs.tick_freshness();

            // Update `inputs.move_dir`.
            inputs.move_dir = if inputs.move_dir.magnitude_squared() > 1.0 {
                // Cap move_dir to 1
                inputs.move_dir.normalized()
            } else {
                inputs.move_dir
            };
            inputs.move_z = inputs.move_z.clamped(-1.0, 1.0);

            // Process other controller events
            for event in controller.events.drain(..) {
                match event {
                    ControlEvent::Mount(mountee_uid) => {
                        if let Some(mountee_entity) = immutable_data
                            .uid_allocator
                            .retrieve_entity_internal(mountee_uid.id())
                        {
                            server_emitter.emit(ServerEvent::Mount(entity, mountee_entity));
                        }
                    },
                    ControlEvent::RemoveBuff(buff_id) => {
                        server_emitter.emit(ServerEvent::Buff {
                            entity,
                            buff_change: BuffChange::RemoveFromController(buff_id),
                        });
                    },
                    ControlEvent::Unmount => server_emitter.emit(ServerEvent::Unmount(entity)),
                    ControlEvent::EnableLantern => {
                        server_emitter.emit(ServerEvent::EnableLantern(entity))
                    },
                    ControlEvent::DisableLantern => {
                        server_emitter.emit(ServerEvent::DisableLantern(entity))
                    },
                    ControlEvent::Interact(npc_uid) => {
                        if let Some(npc_entity) = immutable_data
                            .uid_allocator
                            .retrieve_entity_internal(npc_uid.id())
                        {
                            server_emitter.emit(ServerEvent::NpcInteract(entity, npc_entity));
                        }
                    },
                    ControlEvent::InitiateInvite(inviter_uid, kind) => {
                        server_emitter.emit(ServerEvent::InitiateInvite(entity, inviter_uid, kind));
                    },
                    ControlEvent::InviteResponse(response) => {
                        server_emitter.emit(ServerEvent::InviteResponse(entity, response));
                    },
                    ControlEvent::PerformTradeAction(trade_id, action) => {
                        server_emitter
                            .emit(ServerEvent::ProcessTradeAction(entity, trade_id, action));
                    },
                    ControlEvent::InventoryManip(manip) => {
                        server_emitter.emit(ServerEvent::InventoryManip(entity, manip.into()));
                    },
                    ControlEvent::GroupManip(manip) => {
                        server_emitter.emit(ServerEvent::GroupManip(entity, manip))
                    },
                    ControlEvent::Respawn => server_emitter.emit(ServerEvent::Respawn(entity)),
                }
            }
        }
        immutable_data.metrics.controller_ns.store(
            start_time.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}
