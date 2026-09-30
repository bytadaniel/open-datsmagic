//! # Физический движок симулятора DatsMagic (FE-002)
//!
//! Модуль отвечает за векторную кинематику, вязкое трение, пошаговую схему
//! численного интегрирования Эйлера и связывание с игровым циклом симулятора
//! в соответствии со спецификацией [`FE-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features/FE-002-euler-physics-engine.md)
//! и архитектурным решением [`ADR-002`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/adr/ADR-002-repository-structure.md).

pub mod forces;
pub mod integrator;
pub mod vector2d;

pub use forces::{apply_friction, combine_forces, compute_effective_accel};
pub use integrator::{EulerIntegrator, PhysicsIntegrator};
pub use vector2d::{Vec2, Vector2D};

use crate::engine::command_buffer::{PlayerCommand, PlayerId};
use crate::engine::loop_runner::PhysicsStepHandler;
use crate::engine::state::WorldData;
use std::collections::HashMap;

/// Реализация обработчика шага физики симулятора для интеграции с GameEngine
#[derive(Clone, Debug, Default)]
pub struct WorldPhysicsEngine {
    integrator: EulerIntegrator,
    friction: f64,
}

impl WorldPhysicsEngine {
    /// Создает новый физический обработчик с заданным коэффициентом трения
    pub fn new(friction: f64) -> Self {
        Self {
            integrator: EulerIntegrator,
            friction,
        }
    }
}

impl PhysicsStepHandler for WorldPhysicsEngine {
    fn step(
        &mut self,
        world: &mut WorldData,
        commands: &HashMap<PlayerId, PlayerCommand>,
        dt: f64,
    ) {
        for (player_id, player) in world.players.iter_mut() {
            if player.is_destroyed() {
                player.velocity = (0.0, 0.0);
                for carpet in player.carpets.values_mut() {
                    carpet.mark_destroyed();
                }
                continue;
            }

            if player.carpets.is_empty() {
                let cmd_accel = commands
                    .get(player_id)
                    .map(|cmd| Vec2::new(cmd.acceleration.0, cmd.acceleration.1))
                    .unwrap_or(Vec2::ZERO);

                let pos = Vec2::new(player.position.0, player.position.1);
                let vel = Vec2::new(player.velocity.0, player.velocity.1);

                let env_forces =
                    crate::spatial::compute_environmental_forces_from_states(pos, &world.anomalies);

                let (new_pos, new_vel) = self.integrator.step(
                    pos,
                    vel,
                    cmd_accel,
                    env_forces,
                    player.max_acceleration,
                    player.max_velocity,
                    self.friction,
                    dt,
                    player.is_stunned(),
                );

                player.position = (new_pos.x, new_pos.y);
                player.velocity = (new_vel.x, new_vel.y);
            } else {
                for carpet in player.carpets.values_mut() {
                    if carpet.is_destroyed() {
                        carpet.velocity = (0.0, 0.0);
                        continue;
                    }

                    // Поиск команды сначала по carpet_id, затем по player_id (обратная совместимость)
                    let cmd_accel = commands
                        .get(&carpet.id)
                        .or_else(|| commands.get(player_id))
                        .map(|cmd| Vec2::new(cmd.acceleration.0, cmd.acceleration.1))
                        .unwrap_or(Vec2::ZERO);

                    let pos = Vec2::new(carpet.position.0, carpet.position.1);
                    let vel = Vec2::new(carpet.velocity.0, carpet.velocity.1);

                    let env_forces =
                        crate::spatial::compute_environmental_forces_from_states(pos, &world.anomalies);

                    let (new_pos, new_vel) = self.integrator.step(
                        pos,
                        vel,
                        cmd_accel,
                        env_forces,
                        carpet.max_acceleration,
                        carpet.max_velocity,
                        self.friction,
                        dt,
                        carpet.is_stunned(),
                    );

                    carpet.position = (new_pos.x, new_pos.y);
                    carpet.velocity = (new_vel.x, new_vel.y);
                }
                player.sync_from_carpets();
            }
        }
    }
}
