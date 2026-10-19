//! The countdown before a run: the player may walk and look around, and nothing else
//! happens (no time, spawns, shots or scoring). At GO, [`Prestart::start`] writes where the
//! player stands and how fast they move into the definition's `[player] start` fields, so
//! the scored [`Session`](super::Session) and every replay of it begin from that state.

use super::def::{PlayerDef, ScenarioDef};
use super::session::FrameInput;
use crate::games::valorant::HumanoidDims;
use crate::math::Vec3;
use crate::movement::{self, LocalVelocity, MoveInput, MovementProfile};

/// Player movement during the start countdown.
#[derive(Debug, Clone, PartialEq)]
pub struct Prestart {
    player: PlayerDef,
    profile: MovementProfile,
    eye_height: f32,
    position: Vec3,
    velocity: LocalVelocity,
}

impl Prestart {
    /// The countdown of `def`, from the player start its session would use.
    pub fn new(def: &ScenarioDef) -> Self {
        Self {
            player: def.player.clone(),
            profile: def.weapon.kind.movement(),
            eye_height: HumanoidDims::default().eye_height,
            position: start_position(&def.player),
            velocity: start_velocity(&def.player),
        }
    }

    /// Advance one countdown frame: only movement is applied, and only when the scenario
    /// enables it. Firing is ignored.
    pub fn update(&mut self, input: FrameInput) {
        if !self.player.movement {
            return;
        }
        let dt = input.dt.clamp(0.0, 0.25);
        (self.position, self.velocity) = walk(
            &self.player,
            &self.profile,
            (self.position, self.velocity),
            input.movement,
            input.view.yaw_deg,
            dt,
        );
    }

    /// Where the player's feet are.
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Where the player's eyes are.
    pub fn eye_position(&self) -> Vec3 {
        self.position + Vec3::new(0.0, self.eye_height, 0.0)
    }

    /// Velocity in the player's local frame, m/s.
    pub fn velocity(&self) -> LocalVelocity {
        self.velocity
    }

    /// `def` starting from the reached position and velocity. A scenario without movement
    /// is returned unchanged.
    pub fn start(&self, def: &ScenarioDef) -> ScenarioDef {
        let mut def = def.clone();
        if self.player.movement {
            def.player.start = Some([self.position.x, self.position.z]);
            def.player.start_velocity = Some([self.velocity.right, self.velocity.forward]);
        }
        def
    }

    /// The definition a new run of `def` starts from. In a movement scenario the start
    /// fields record one run's countdown, so they are cleared for the next countdown to
    /// write its own; other scenarios are returned unchanged.
    pub fn fresh(def: &ScenarioDef) -> ScenarioDef {
        let mut def = def.clone();
        if def.player.movement {
            def.player.start = None;
            def.player.start_velocity = None;
        }
        def
    }
}

/// Where a session of a scenario with `player` settings puts the player's feet.
pub(super) fn start_position(player: &PlayerDef) -> Vec3 {
    let [x, z] = player.start.unwrap_or([0.0, 0.0]);
    Vec3::new(x, player.elevation, z)
}

/// The player's local velocity when a session of a scenario with `player` settings begins.
pub(super) fn start_velocity(player: &PlayerDef) -> LocalVelocity {
    player
        .start_velocity
        .map_or(LocalVelocity::default(), |[right, forward]| LocalVelocity {
            right,
            forward,
        })
}

/// One movement step of the player's feet inside the play area, shared by the countdown and
/// the session so both move exactly alike.
pub(super) fn walk(
    player: &PlayerDef,
    profile: &MovementProfile,
    (position, velocity): (Vec3, LocalVelocity),
    input: MoveInput,
    yaw_deg: f32,
    dt: f32,
) -> (Vec3, LocalVelocity) {
    let velocity = movement::step(profile, velocity, input, dt);
    let mut world = velocity.to_world(yaw_deg);
    let next = position + world * dt;
    let (bx, bz) = (player.bounds_x, player.bounds_z);
    let clamped = Vec3::new(
        next.x.clamp(bx.min, bx.max),
        player.elevation,
        next.z.clamp(bz.min, bz.max),
    );
    // Hitting an edge of the play area behaves like a wall: only the velocity pushing
    // into it is lost, so the player keeps sliding along the edge.
    if clamped.x != next.x {
        world.x = 0.0;
    }
    if clamped.z != next.z {
        world.z = 0.0;
    }
    (clamped, LocalVelocity::from_world(world, yaw_deg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::codec;
    use crate::math::{Range, ViewAngles};
    use crate::scenario::{Recording, Session, builtin};
    use crate::sim::{AimerProfile, SimAimer};

    fn movement_scenario() -> ScenarioDef {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.player.movement)
            .expect("a built-in movement scenario");
        assert!(def.validate().is_empty());
        def
    }

    fn strafe(right: f32, yaw_deg: f32, fire: bool) -> FrameInput {
        FrameInput {
            dt: 1.0 / 144.0,
            view: ViewAngles {
                yaw_deg,
                pitch_deg: 0.0,
            },
            fire_pressed: fire,
            fire_held: fire,
            movement: MoveInput {
                right,
                ..Default::default()
            },
        }
    }

    /// The countdown after `frames` frames of strafing right while turning and firing.
    fn after_countdown(def: &ScenarioDef, frames: usize) -> Prestart {
        let mut p = Prestart::new(def);
        for i in 0..frames {
            p.update(strafe(1.0, i as f32 * 0.05, true));
        }
        p
    }

    #[test]
    fn the_countdown_moves_the_player_inside_the_bounds_and_nothing_else() {
        let mut def = movement_scenario();
        def.player.bounds_x = Range::new(-1.0, 1.0);
        def.player.bounds_z = Range::new(-1.0, 1.0);
        let p = after_countdown(&def, 3 * 144);
        let at = p.position();
        assert!((at.x - 1.0).abs() < 1e-5, "held at the right edge: {at:?}");
        assert!((-1.0..=1.0).contains(&at.z) && at.y == def.player.elevation);
        assert_eq!(
            p.eye_position().y - at.y,
            HumanoidDims::default().eye_height
        );
        // Firing during the countdown was ignored: the run starts untouched.
        let s = Session::new(p.start(&def), 7);
        assert_eq!((s.time(), s.stats().shots), (0.0, 0));
        assert_eq!(s.targets().count(), Session::new(def, 7).targets().count());
    }

    #[test]
    fn without_movement_the_countdown_stays_put_and_changes_nothing() {
        let def = builtin::all()
            .into_iter()
            .find(|d| !d.player.movement)
            .unwrap();
        let p = after_countdown(&def, 144);
        assert_eq!(p.position(), Vec3::new(0.0, def.player.elevation, 0.0));
        assert_eq!(p.start(&def), def);
    }

    #[test]
    fn the_run_starts_where_the_countdown_left_the_player_and_keeps_moving_alike() {
        let def = movement_scenario();
        let mut p = after_countdown(&def, 100);
        let started = p.start(&def);
        assert!(started.validate().is_empty(), "{:?}", started.validate());
        let mut s = Session::new(started, 3);
        assert_eq!(s.player_position(), p.position());
        assert!(p.velocity().speed() > 1.0, "still moving at GO");
        for i in 0..30 {
            let input = strafe(-1.0, 5.0 + i as f32 * 0.1, false);
            p.update(input);
            s.update(input);
            assert_eq!(s.player_position(), p.position(), "frame {i}");
        }
    }

    #[test]
    fn a_run_started_after_the_countdown_replays_bit_identically_from_its_file() {
        let def = movement_scenario();
        let started = after_countdown(&def, 90).start(&def);
        let mut rec = Recording::new(started, 11);
        let mut live = rec.start();
        let mut aimer = SimAimer::new(AimerProfile::PRESETS[1], 11);
        while !live.is_finished() {
            let input = aimer.input(&live, 1.0 / 144.0);
            rec.inputs.push(input);
            live.update(input);
        }
        let decoded = codec::decode(&codec::encode(&rec).unwrap()).unwrap();
        assert_eq!(decoded, rec, "the start fields survive the file");
        let replayed = decoded.replay(|_, _| {});
        assert_eq!(replayed.result(), live.result());
        assert_eq!(replayed.stats(), live.stats());
        assert_eq!(replayed.player_position(), live.player_position());
    }

    #[test]
    fn a_new_run_drops_the_previous_countdown_start_of_movement_scenarios_only() {
        let def = movement_scenario();
        let started = after_countdown(&def, 60).start(&def);
        assert_ne!(started, def);
        assert_eq!(Prestart::fresh(&started), def);
        let mut still = builtin::all()
            .into_iter()
            .find(|d| !d.player.movement)
            .unwrap();
        still.player.start = Some([0.0, 0.0]);
        assert_eq!(Prestart::fresh(&still), still);
    }

    #[test]
    fn an_explicit_origin_start_plays_exactly_like_the_default() {
        let def = movement_scenario();
        let mut origin = def.clone();
        origin.player.start = Some([0.0, 0.0]);
        origin.player.start_velocity = Some([0.0, 0.0]);
        let play = |def: ScenarioDef| {
            let mut s = Session::new(def, 5);
            let mut aimer = SimAimer::new(AimerProfile::PRESETS[2], 5);
            while !s.is_finished() {
                let input = aimer.input(&s, 1.0 / 144.0);
                s.update(input);
            }
            (s.result(), s.stats().clone(), s.player_position())
        };
        assert_eq!(play(origin), play(def));
    }
}
