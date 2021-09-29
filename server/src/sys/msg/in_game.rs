#[cfg(feature = "persistent_world")]
use crate::TerrainPersistence;
use crate::{client::Client, presence::Presence, Settings};
use common::{
    comp::{CanBuild, RemoteController, SkillSet},
    event::{EventBus, ServerEvent},
    resources::Time,
    terrain::TerrainGrid,
    vol::ReadVol,
};
use common_ecs::{Job, Origin, Phase, System};
use common_net::msg::{ClientGeneral, PresenceKind, ServerGeneral};
use common_state::{BlockChange, BuildAreas};
use specs::{Entities, Join, Read, ReadExpect, ReadStorage, Write, WriteStorage};
use std::time::Duration;
use tracing::{debug, trace};

#[cfg(feature = "persistent_world")]
pub type TerrainPersistenceData<'a> = Option<Write<'a, TerrainPersistence>>;
#[cfg(not(feature = "persistent_world"))]
pub type TerrainPersistenceData<'a> = ();

impl Sys {
    #[allow(clippy::too_many_arguments)]
    fn handle_client_in_game_msg(
        server_emitter: &mut common::event::Emitter<'_, ServerEvent>,
        entity: specs::Entity,
        time: &Time,
        client: &Client,
        maybe_presence: &mut Option<&mut Presence>,
        terrain: &ReadExpect<'_, TerrainGrid>,
        can_build: &ReadStorage<'_, CanBuild>,
        skill_sets: &mut WriteStorage<'_, SkillSet>,
        block_changes: &mut Write<'_, BlockChange>,
        remote_controllers: &mut WriteStorage<'_, RemoteController>,
        settings: &Read<'_, Settings>,
        build_areas: &Read<'_, BuildAreas>,
        _terrain_persistence: &mut TerrainPersistenceData<'_>,
        msg: ClientGeneral,
    ) -> Result<(), crate::error::Error> {
        let presence = match maybe_presence {
            Some(g) => g,
            None => {
                debug!(?entity, "client is not in_game, ignoring msg");
                trace!(?msg, "ignored msg content");
                return Ok(());
            },
        };
        match msg {
            // Go back to registered state (char selection screen)
            ClientGeneral::ExitInGame => {
                server_emitter.emit(ServerEvent::ExitIngame { entity });
                client.send(ServerGeneral::ExitInGameSuccess)?;
                *maybe_presence = None;
            },
            ClientGeneral::SetViewDistance(view_distance) => {
                presence.view_distance = settings
                    .max_view_distance
                    .map(|max| view_distance.min(max))
                    .unwrap_or(view_distance);

                //correct client if its VD is to high
                if settings
                    .max_view_distance
                    .map(|max| view_distance > max)
                    .unwrap_or(false)
                {
                    client.send(ServerGeneral::SetViewDistance(
                        settings.max_view_distance.unwrap_or(0),
                    ))?;
                }
            },
            ClientGeneral::Control(rc) => {
                if matches!(presence.kind, PresenceKind::Character(_)) {
                    if let Ok(remote_controller) = remote_controllers
                        .entry(entity)
                        .map(|e| e.or_insert_with(Default::default))
                    {
                        let ids = remote_controller.append(rc);
                        remote_controller.maintain(Some(Duration::from_secs_f64(time.0)));
                        // confirm controls
                        client.send(ServerGeneral::AckControl(ids, *time))?;
                        //Todo: FIXME!!!
                        /*
                                                // Skip respawn if client entity is alive
                        if let ControlEvent::Respawn = event {
                            if healths.get(entity).map_or(true, |h| !h.is_dead) {
                                //Todo: comment why return!
                                return Ok(());
                            }
                        }
                             */
                    }
                }
            },
            ClientGeneral::BreakBlock(pos) => {
                if let Some(comp_can_build) = can_build.get(entity) {
                    if comp_can_build.enabled {
                        for area in comp_can_build.build_areas.iter() {
                            if let Some(old_block) = build_areas
                                .areas()
                                .get(*area)
                                // TODO: Make this an exclusive check on the upper bound of the AABB
                                // Vek defaults to inclusive which is not optimal
                                .filter(|aabb| aabb.contains_point(pos))
                                .and_then(|_| terrain.get(pos).ok())
                            {
                                let new_block = old_block.into_vacant();
                                let _was_set = block_changes.try_set(pos, new_block).is_some();
                                #[cfg(feature = "persistent_world")]
                                if _was_set {
                                    if let Some(terrain_persistence) = _terrain_persistence.as_mut()
                                    {
                                        terrain_persistence.set_block(pos, new_block);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            ClientGeneral::PlaceBlock(pos, new_block) => {
                if let Some(comp_can_build) = can_build.get(entity) {
                    if comp_can_build.enabled {
                        for area in comp_can_build.build_areas.iter() {
                            if build_areas
                                .areas()
                                .get(*area)
                                // TODO: Make this an exclusive check on the upper bound of the AABB
                                // Vek defaults to inclusive which is not optimal
                                .filter(|aabb| aabb.contains_point(pos))
                                .is_some()
                            {
                                let _was_set = block_changes.try_set(pos, new_block).is_some();
                                #[cfg(feature = "persistent_world")]
                                if _was_set {
                                    if let Some(terrain_persistence) = _terrain_persistence.as_mut()
                                    {
                                        terrain_persistence.set_block(pos, new_block);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            ClientGeneral::UnlockSkill(skill) => {
                skill_sets
                    .get_mut(entity)
                    .map(|mut skill_set| skill_set.unlock_skill(skill));
            },
            ClientGeneral::RefundSkill(skill) => {
                skill_sets
                    .get_mut(entity)
                    .map(|mut skill_set| skill_set.refund_skill(skill));
            },
            ClientGeneral::UnlockSkillGroup(skill_group_kind) => {
                skill_sets
                    .get_mut(entity)
                    .map(|mut skill_set| skill_set.unlock_skill_group(skill_group_kind));
            },
            ClientGeneral::RequestSiteInfo(id) => {
                server_emitter.emit(ServerEvent::RequestSiteInfo { entity, id });
            },
            ClientGeneral::RequestLossyTerrainCompression {
                lossy_terrain_compression,
            } => {
                presence.lossy_terrain_compression = lossy_terrain_compression;
            },
            _ => tracing::error!("not a client_in_game msg"),
        }
        Ok(())
    }
}

/// This system will handle new messages from clients
#[derive(Default)]
pub struct Sys;
impl<'a> System<'a> for Sys {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Entities<'a>,
        Read<'a, Time>,
        Read<'a, EventBus<ServerEvent>>,
        ReadExpect<'a, TerrainGrid>,
        ReadStorage<'a, CanBuild>,
        WriteStorage<'a, SkillSet>,
        Write<'a, BlockChange>,
        WriteStorage<'a, Presence>,
        WriteStorage<'a, Client>,
        WriteStorage<'a, RemoteController>,
        Read<'a, Settings>,
        Read<'a, BuildAreas>,
        TerrainPersistenceData<'a>,
    );

    const NAME: &'static str = "msg::in_game";
    const ORIGIN: Origin = Origin::Server;
    const PHASE: Phase = Phase::Create;

    fn run(
        _job: &mut Job<Self>,
        (
            entities,
            time,
            server_event_bus,
            terrain,
            can_build,
            mut skill_sets,
            mut block_changes,
            mut presences,
            mut clients,
            mut remote_controllers,
            settings,
            build_areas,
            mut terrain_persistence,
        ): Self::SystemData,
    ) {
        let mut server_emitter = server_event_bus.emitter();

        for (entity, client, mut maybe_presence) in
            (&entities, &mut clients, (&mut presences).maybe()).join()
        {
            let _ = super::try_recv_all(client, 2, |client, msg| {
                Self::handle_client_in_game_msg(
                    &mut server_emitter,
                    entity,
                    &time,
                    client,
                    &mut maybe_presence.as_deref_mut(),
                    &terrain,
                    &can_build,
                    &mut skill_sets,
                    &mut block_changes,
                    &mut remote_controllers,
                    &settings,
                    &build_areas,
                    &mut terrain_persistence,
                    msg,
                )
            });
        }
    }
}
