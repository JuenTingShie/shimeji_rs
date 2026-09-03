pub mod weighted;

use crate::format::animation::{Animation, AnimationSchema, Direction, Edge, SurfaceType};
use rand::RngExt;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Ground,
    Wall,
    Ceiling,
    Air,
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceContext {
    pub kind: SurfaceKind,
    pub edge_hit: Option<Edge>,
    /// The y coordinate of the floor this query resolved (whichever monitor or window is
    /// actually under the mascot). A falling mascot's per-tick dy is a fixed animation value
    /// that will almost never divide evenly into "distance to floor", so the tick that reports
    /// `edge_hit: Some(Edge::Bottom)` typically lands a few pixels short of `floor_y`, not
    /// exactly on it. Callers that need to land exactly on the floor (see `app::step_one_mascot`)
    /// snap to this value instead of trusting the animation's own dy to get there.
    pub floor_y: i32,
    /// The y coordinate of the ceiling this query resolved (the underside of whichever window
    /// is above the mascot, or the top of whichever monitor is), symmetric to `floor_y` for a
    /// mascot climbing up a wall toward `Edge::Top`.
    pub ceiling_y: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct StepOutput {
    pub sprite_index: u32,
    pub dx: i32,
    pub dy: i32,
    pub changed_animation: bool,
}

#[derive(Debug, Clone, Copy)]
struct TimerTarget {
    tick: u32,
    resolved: bool,
}

pub struct StateMachine<'a> {
    schema: &'a AnimationSchema,
    by_key: HashMap<&'a str, &'a Animation>,
    current: &'a Animation,
    frame_index: usize,
    frame_ticks_remaining: u32,
    ticks_in_animation: u32,
    facing: Direction,
    timer_targets: Vec<TimerTarget>,
    max_duration_target: Option<u32>,
}

/// A `StateMachine<'a>` borrows the `AnimationSchema` it interprets, so it can't be stored
/// alongside the schema it borrows from in a longer-lived owner (e.g. a per-mascot struct that
/// also owns the schema) — that's a self-referential struct, which Rust can't express. This
/// snapshot is the plain-data workaround: it captures everything `step`/`apply_event` need to
/// resume exactly where they left off, so the owner can hold a `StateMachineSnapshot` between
/// calls and rebuild a fresh `StateMachine` from it each time via `from_snapshot`.
///
/// Earlier code in this crate rebuilt a `StateMachine` each tick via `new()` + `force_animation()`
/// instead of a real snapshot — `force_animation` (via `enter_animation`) unconditionally resets
/// `frame_index`/`ticks_in_animation` to 0, so every tick replayed frame 0 of the current
/// animation from scratch. For single-frame animations that's invisible, but it silently broke
/// two real things: multi-frame animations (e.g. `walk_left`'s 4-frame cycle) never advanced past
/// frame 0, and `onTimer`/`maxDurationTicks` transitions never fired because `ticks_in_animation`
/// never exceeded 1. This was caught by watching a real spawned mascot fall forever off the
/// bottom of the screen instead of landing.
#[derive(Debug, Clone)]
pub struct StateMachineSnapshot {
    pub current_key: String,
    frame_index: usize,
    frame_ticks_remaining: u32,
    ticks_in_animation: u32,
    pub facing: Direction,
    timer_targets: Vec<TimerTarget>,
    max_duration_target: Option<u32>,
}

/// `new()` and the internal `on_finish()` fallback below need a source of randomness but aren't
/// reachable from a test-controlled `rng` argument (the public `step`/`apply_event` API takes the
/// caller's `rng` explicitly for testability). They use this instead. This only affects unweighted
/// single-choice cases in the fixture's `onFinish` lists, so it doesn't affect the determinism of
/// any test above, which all supply their own seeded `rng` directly to `step`/`apply_event`.
fn fresh_rng() -> rand::rngs::StdRng {
    use rand::SeedableRng;
    rand::rngs::StdRng::seed_from_u64(rand::random())
}

impl<'a> StateMachine<'a> {
    pub fn new(schema: &'a AnimationSchema) -> Self {
        let by_key: HashMap<&str, &Animation> =
            schema.animations.iter().map(|a| (a.key.as_str(), a)).collect();
        let current = *by_key
            .get(schema.default_animation.as_str())
            .expect("bundle validation guarantees default_animation exists");
        // The default animation's own `direction` can be ANY (e.g. "fall", which plays the same
        // regardless of facing) — but facing must be a concrete LEFT/RIGHT for later
        // facing-gated border transitions (like fall's BOTTOM -> bounce_left/bounce_right) to ever
        // match, or a freshly spawned mascot on ANY facing would never land. Resolve ANY to a
        // random concrete facing up front, the same way `resolve_facing` treats "RANDOM".
        let mut rng = fresh_rng();
        let facing = match current.direction {
            Direction::Any => {
                if rng.random_bool(0.5) {
                    Direction::Left
                } else {
                    Direction::Right
                }
            }
            concrete => concrete,
        };
        let mut sm = StateMachine {
            schema,
            by_key,
            current,
            frame_index: 0,
            frame_ticks_remaining: current.frames[0].duration_ticks,
            ticks_in_animation: 0,
            facing,
            timer_targets: Vec::new(),
            max_duration_target: None,
        };
        sm.resolve_timers_for_current(&mut rng);
        sm
    }

    pub fn force_animation(&mut self, key: &str) {
        self.enter_animation(key, self.facing, &mut fresh_rng());
    }

    /// Captures the mid-animation progress `from_snapshot` needs to resume exactly where this
    /// `StateMachine` left off — the production alternative to `new()` + `force_animation()`,
    /// which resets frame/tick progress and should only be used to set up a specific starting
    /// animation in tests.
    pub fn snapshot(&self) -> StateMachineSnapshot {
        StateMachineSnapshot {
            current_key: self.current.key.clone(),
            frame_index: self.frame_index,
            frame_ticks_remaining: self.frame_ticks_remaining,
            ticks_in_animation: self.ticks_in_animation,
            facing: self.facing,
            timer_targets: self.timer_targets.clone(),
            max_duration_target: self.max_duration_target,
        }
    }

    /// Rebuilds a `StateMachine` resuming exactly where a prior instance's `snapshot()` left off
    /// (frame position, tick counts, and already-resolved timer targets all preserved).
    pub fn from_snapshot(schema: &'a AnimationSchema, snapshot: &StateMachineSnapshot) -> Self {
        let by_key: HashMap<&str, &Animation> =
            schema.animations.iter().map(|a| (a.key.as_str(), a)).collect();
        let current = *by_key
            .get(snapshot.current_key.as_str())
            .expect("snapshot's current_key must reference a valid animation in this schema");
        StateMachine {
            schema,
            by_key,
            current,
            frame_index: snapshot.frame_index,
            frame_ticks_remaining: snapshot.frame_ticks_remaining,
            ticks_in_animation: snapshot.ticks_in_animation,
            facing: snapshot.facing,
            timer_targets: snapshot.timer_targets.clone(),
            max_duration_target: snapshot.max_duration_target,
        }
    }

    /// The snapshot for a brand-new instance on `schema.default_animation` — what a freshly
    /// spawned mascot starts from.
    pub fn initial_snapshot(schema: &'a AnimationSchema) -> StateMachineSnapshot {
        StateMachine::new(schema).snapshot()
    }

    /// The `dx`/`dy` the current frame will apply on the next `step()` call, needed by callers
    /// that must know a mascot's pending movement before they can compute the `SurfaceContext`
    /// `step()` itself requires (e.g. detecting that a fall is about to reach the floor this tick).
    pub fn pending_movement(&self) -> (i32, i32) {
        let frame = &self.current.frames[self.frame_index];
        (frame.dx, frame.dy)
    }

    pub fn current_key(&self) -> &str {
        &self.current.key
    }

    pub fn facing(&self) -> Direction {
        self.facing
    }

    /// The "resting surface" edge the legacy schema's JUMP rules key off of via `when` — BOTTOM
    /// for standing on the ground, TOP for hanging from a ceiling, LEFT/RIGHT for climbing a
    /// wall on that side (inferred from the current WALL animation's own facing, since
    /// `climb_left` is always LEFT-facing and `climb_right` always RIGHT-facing — no separate
    /// geometry check needed). This answers a persistent "what am I on" question, unlike every
    /// other use of `edge_hit` (including this same schema's FLING_END rules), which answers
    /// "did I just cross an edge this tick". A mascot standing calmly has `edge_hit: None` from
    /// a live `query_surface` call, which would otherwise always fall through JUMP's rules to
    /// the schema's generic `to: fall` catch-all instead of a directional jump.
    pub fn resting_edge(&self) -> Option<Edge> {
        match self.current.kind {
            SurfaceType::Ground => Some(Edge::Bottom),
            SurfaceType::Ceiling => Some(Edge::Top),
            SurfaceType::Wall => match self.facing {
                Direction::Left => Some(Edge::Left),
                Direction::Right => Some(Edge::Right),
                Direction::Any => None,
            },
            SurfaceType::Air | SurfaceType::User => None,
        }
    }

    fn enter_animation<R: RngExt>(&mut self, key: &str, facing: Direction, rng: &mut R) {
        self.current = self.by_key.get(key).expect("caller guarantees key exists");
        self.frame_index = 0;
        self.frame_ticks_remaining = self.current.frames[0].duration_ticks;
        self.ticks_in_animation = 0;
        self.facing = facing;
        self.resolve_timers_for_current(rng);
    }

    fn resolve_timers_for_current<R: RngExt>(&mut self, rng: &mut R) {
        self.timer_targets.clear();
        self.max_duration_target = None;
        let Some(auto) = &self.current.auto else { return };
        for rule in &auto.on_timer {
            let tick = if rule.min_ticks >= rule.max_ticks {
                rule.min_ticks
            } else {
                rng.random_range(rule.min_ticks..=rule.max_ticks)
            };
            self.timer_targets.push(TimerTarget { tick, resolved: false });
        }
        if let Some(max) = &auto.max_duration_ticks {
            self.max_duration_target = Some(match max {
                crate::format::animation::MaxDurationTicks::Fixed(t) => *t,
                crate::format::animation::MaxDurationTicks::Range { min_ticks, max_ticks } => {
                    if min_ticks >= max_ticks {
                        *min_ticks
                    } else {
                        rng.random_range(*min_ticks..=*max_ticks)
                    }
                }
            });
        }
    }

    pub fn step<R: RngExt>(&mut self, surface: SurfaceContext, level: u8, rng: &mut R) -> StepOutput {
        // Border transitions take priority: they fire immediately on edge contact.
        if let Some(edge) = surface.edge_hit {
            for bt in &self.current.border_transitions {
                if bt.when == edge && bt.facing.map_or(true, |f| f == self.facing) {
                    if let Some(choice) = weighted::pick_weighted(&bt.choices, level, rng) {
                        let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, rng);
                        let to = choice.to.clone();
                        self.enter_animation(&to, facing, rng);
                        return self.frame_output(true);
                    }
                }
            }
        }

        let frame = &self.current.frames[self.frame_index];
        let dx = frame.dx;
        let dy = frame.dy;

        self.ticks_in_animation += 1;
        if self.frame_ticks_remaining > 1 {
            self.frame_ticks_remaining -= 1;
        } else {
            self.advance_frame();
        }

        // Cloning each rule (rather than holding a borrow of `self.current.auto` across the
        // `self.enter_animation(...)` calls below, which also mutate `self`) keeps the borrow checker happy.
        if self.current.auto.is_some() {
            for i in 0..self.timer_targets.len() {
                if self.timer_targets[i].resolved || self.ticks_in_animation < self.timer_targets[i].tick {
                    continue;
                }
                self.timer_targets[i].resolved = true;
                let rule = self.current.auto.as_ref().unwrap().on_timer[i].clone();
                if rng.random_bool(rule.chance) {
                    if let Some(choice) = weighted::pick_weighted(&rule.choices, level, rng) {
                        let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, rng);
                        let to = choice.to.clone();
                        self.enter_animation(&to, facing, rng);
                        return StepOutput { sprite_index: self.current.frames[0].sprite, dx, dy, changed_animation: true };
                    }
                }
            }
            if let Some(max) = self.max_duration_target {
                if self.ticks_in_animation >= max {
                    let default = self.schema.default_animation.clone();
                    self.enter_animation(&default, self.facing, rng);
                    return StepOutput { sprite_index: self.current.frames[0].sprite, dx, dy, changed_animation: true };
                }
            }
        }

        StepOutput { sprite_index: frame.sprite, dx, dy, changed_animation: false }
    }

    fn advance_frame(&mut self) {
        if self.frame_index + 1 < self.current.frames.len() {
            self.frame_index += 1;
            self.frame_ticks_remaining = self.current.frames[self.frame_index].duration_ticks;
        } else if matches!(self.current.loop_mode, crate::format::animation::LoopMode::Loop) {
            self.frame_index = 0;
            self.frame_ticks_remaining = self.current.frames[0].duration_ticks;
        } else {
            self.on_finish();
        }
    }

    fn on_finish(&mut self) {
        let Some(auto) = &self.current.auto else { return };
        if auto.on_finish.is_empty() {
            return;
        }
        let mut rng = fresh_rng();
        if let Some(choice) = weighted::pick_weighted(&auto.on_finish, u8::MAX, &mut rng) {
            let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, &mut rng);
            let to = choice.to.clone();
            self.enter_animation(&to, facing, &mut rng);
        }
    }

    fn frame_output(&self, changed_animation: bool) -> StepOutput {
        let frame = &self.current.frames[self.frame_index];
        StepOutput { sprite_index: frame.sprite, dx: frame.dx, dy: frame.dy, changed_animation }
    }

    pub fn current_kind(&self) -> SurfaceType {
        self.current.kind
    }

    pub fn apply_event<R: RngExt>(
        &mut self,
        event: crate::format::animation::EngineEventKind,
        surface: SurfaceContext,
        level: u8,
        rng: &mut R,
    ) -> bool {
        for rule in &self.schema.events {
            if rule.event != event {
                continue;
            }
            let from_matches = match &rule.from {
                Some(f) if f == "*" => true,
                Some(f) => f == self.current.key.as_str(),
                None => {
                    let level_ok = rule.min_level.map_or(true, |m| level >= m)
                        && rule.max_level.map_or(true, |m| level <= m);
                    let type_ok = rule
                        .allowed_types
                        .as_ref()
                        .map_or(true, |types| types.contains(&self.current.kind));
                    level_ok && type_ok
                }
            };
            if !from_matches {
                continue;
            }
            if let Some(when) = rule.when {
                if surface.edge_hit != Some(when) {
                    continue;
                }
            }
            if let Some(facing) = rule.facing {
                if facing != self.facing {
                    continue;
                }
            }

            if let Some(choices) = &rule.choices {
                if let Some(choice) = weighted::pick_weighted(choices, level, rng) {
                    let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, rng);
                    let to = choice.to.clone();
                    self.enter_animation(&to, facing, rng);
                    return true;
                }
            } else if let Some(to) = &rule.to {
                let facing = resolve_facing(rule.set_facing.as_deref(), self.facing, rng);
                let to = to.clone();
                self.enter_animation(&to, facing, rng);
                return true;
            }
        }
        false
    }
}

fn resolve_facing<R: RngExt>(set_facing: Option<&str>, current: Direction, rng: &mut R) -> Direction {
    match set_facing {
        Some("LEFT") => Direction::Left,
        Some("RIGHT") => Direction::Right,
        Some("RANDOM") => if rng.random_bool(0.5) { Direction::Left } else { Direction::Right },
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::animation::{AnimationSchema, EngineEventKind, SurfaceType};
    use rand::prelude::*;

    fn schema() -> AnimationSchema {
        let json = std::fs::read_to_string("tests/fixtures/sample_animation.json").unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn no_edge() -> SurfaceContext {
        SurfaceContext { kind: SurfaceKind::Ground, edge_hit: None, floor_y: 0, ceiling_y: 0 }
    }

    /// A snapshot on `key` with an explicit `facing`, bypassing `StateMachine::new`'s randomly
    /// resolved initial facing (unseedable by design — see `fresh_rng`'s doc comment) and
    /// `force_animation`'s carry-forward of whatever facing was already set. Needed for tests
    /// that depend on a specific facing, like a WALL animation's climbing side.
    fn snapshot_at(schema: &AnimationSchema, key: &str, facing: Direction) -> StateMachineSnapshot {
        let anim = schema.animations.iter().find(|a| a.key == key).expect("test fixture must define this animation");
        StateMachineSnapshot {
            current_key: key.to_string(),
            frame_index: 0,
            frame_ticks_remaining: anim.frames[0].duration_ticks,
            ticks_in_animation: 0,
            facing,
            timer_targets: Vec::new(),
            max_duration_target: None,
        }
    }

    #[test]
    fn starts_on_the_default_animation() {
        let schema = schema();
        let sm = StateMachine::new(&schema);
        assert_eq!(sm.current_key(), "fall");
    }

    #[test]
    fn fresh_spawn_gets_a_concrete_facing_not_any() {
        // Regression test for a real bug: "fall"'s own `direction` is ANY, and StateMachine::new
        // used to copy it verbatim into `facing`. But fall's BOTTOM border transitions are gated
        // on facing == LEFT or facing == RIGHT specifically — ANY matches neither, so a freshly
        // spawned mascot could fall forever and never land. facing must resolve to a concrete
        // direction on spawn, the same way "RANDOM" resolves elsewhere.
        let schema = schema();
        let sm = StateMachine::new(&schema);
        assert!(sm.facing() == Direction::Left || sm.facing() == Direction::Right);
    }

    #[test]
    fn a_freshly_spawned_mascot_falling_past_the_floor_actually_lands() {
        // End-to-end regression test combining the facing fix above with the snapshot/resume
        // cycle and query_surface's BOTTOM edge detection: a brand new mascot dropped from y=100
        // must eventually leave "fall" once it reaches the floor, not fall forever.
        let schema = schema();
        let screen = crate::environment::Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        let monitors = [screen];
        let mut rng = StdRng::seed_from_u64(5);
        let mut y = 100;
        let mut snapshot = StateMachine::initial_snapshot(&schema);

        for _ in 0..200 {
            let mut sm = StateMachine::from_snapshot(&schema, &snapshot);
            let (_, dy) = sm.pending_movement();
            let surface = crate::environment::surface::query_surface(500, y, 64, 64, 0, dy, &screen, &monitors, &[]);
            let out = sm.step(surface, 4, &mut rng);
            y += out.dy;
            snapshot = sm.snapshot();
            if snapshot.current_key != "fall" {
                break;
            }
        }
        assert_ne!(snapshot.current_key, "fall", "mascot never left the fall animation — it fell forever");
        assert!(y < screen.bottom + 200, "mascot fell well past the floor without landing (y={y})");
    }

    #[test]
    fn oneshot_animation_advances_frames_by_duration_ticks() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("bounce_left");
        let mut rng = StdRng::seed_from_u64(7);

        let out1 = sm.step(no_edge(), 4, &mut rng);
        assert_eq!(out1.sprite_index, 17);
        for _ in 0..7 {
            sm.step(no_edge(), 4, &mut rng);
        }
        // after 8 ticks total, frame 2 (sprite 18) should be current
        let out_after_frame1 = sm.step(no_edge(), 4, &mut rng);
        assert_eq!(out_after_frame1.sprite_index, 18);
    }

    #[test]
    fn oneshot_animation_transitions_on_finish() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("bounce_left");
        let mut rng = StdRng::seed_from_u64(7);

        // bounce_left = 8 + 8 = 16 ticks total before onFinish fires.
        for _ in 0..16 {
            sm.step(no_edge(), 4, &mut rng);
        }
        assert!(sm.current_key() == "walk_left" || sm.current_key() == "walk_right");
    }

    #[test]
    fn border_transition_fires_immediately_on_edge_hit() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        let mut rng = StdRng::seed_from_u64(3);

        let hit_left = SurfaceContext { kind: SurfaceKind::Ground, edge_hit: Some(crate::format::animation::Edge::Left), floor_y: 0, ceiling_y: 0 };
        sm.step(hit_left, 4, &mut rng);
        assert!(sm.current_key() == "climb_left" || sm.current_key() == "walk_right");
    }

    #[test]
    fn drag_start_switches_to_drag_from_any_animation() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        let mut rng = StdRng::seed_from_u64(1);

        let changed = sm.apply_event(EngineEventKind::DragStart, no_edge(), 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "drag");
    }

    #[test]
    fn fling_end_on_left_edge_climbs() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("fling");
        let mut rng = StdRng::seed_from_u64(1);

        let left_edge = SurfaceContext { kind: SurfaceKind::Wall, edge_hit: Some(crate::format::animation::Edge::Left), floor_y: 0, ceiling_y: 0 };
        let changed = sm.apply_event(EngineEventKind::FlingEnd, left_edge, 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "climb_left");
    }

    #[test]
    fn tap_at_level_one_has_no_matching_rule() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        let mut rng = StdRng::seed_from_u64(1);

        let changed = sm.apply_event(EngineEventKind::Tap, no_edge(), 1, &mut rng);
        assert!(!changed);
        assert_eq!(sm.current_key(), "walk_left");
    }

    #[test]
    fn tap_at_level_four_on_ground_picks_a_response() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        assert_eq!(sm.current_kind(), SurfaceType::Ground);
        let mut rng = StdRng::seed_from_u64(1);

        let changed = sm.apply_event(EngineEventKind::Tap, no_edge(), 4, &mut rng);
        assert!(changed);
    }

    // Regression tests for a real bug: the schema's JUMP rules key `when` off which surface
    // the mascot is currently resting on (BOTTOM/TOP/LEFT/RIGHT = ground/ceiling/wall), not an
    // edge crossed this tick like every other rule that reads `edge_hit`. Right-clicking "Jump"
    // fires at an arbitrary moment while calmly resting (a live geometry query's edge_hit is
    // None then), so without resting_edge every JUMP fell through to the schema's generic
    // `to: fall` catch-all instead of a directional jump.

    #[test]
    fn resting_edge_on_ground_is_bottom() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        assert_eq!(sm.resting_edge(), Some(crate::format::animation::Edge::Bottom));
    }

    #[test]
    fn resting_edge_on_ceiling_is_top() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("climb_ceiling_left");
        assert_eq!(sm.resting_edge(), Some(crate::format::animation::Edge::Top));
    }

    #[test]
    fn resting_edge_climbing_a_left_facing_wall_is_left() {
        let schema = schema();
        let sm = StateMachine::from_snapshot(&schema, &snapshot_at(&schema, "climb_left", Direction::Left));
        assert_eq!(sm.resting_edge(), Some(crate::format::animation::Edge::Left));
    }

    #[test]
    fn resting_edge_climbing_a_right_facing_wall_is_right() {
        let schema = schema();
        let sm = StateMachine::from_snapshot(&schema, &snapshot_at(&schema, "climb_right", Direction::Right));
        assert_eq!(sm.resting_edge(), Some(crate::format::animation::Edge::Right));
    }

    #[test]
    fn resting_edge_while_airborne_is_none_so_jump_falls_through_to_fall() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("fall");
        assert_eq!(sm.resting_edge(), None);
    }

    #[test]
    fn jump_while_standing_on_the_ground_facing_left_jumps_up_left() {
        let schema = schema();
        let mut sm = StateMachine::from_snapshot(&schema, &snapshot_at(&schema, "walk_left", Direction::Left));
        let mut rng = StdRng::seed_from_u64(1);

        let resting = SurfaceContext { kind: SurfaceKind::Ground, edge_hit: sm.resting_edge(), floor_y: 0, ceiling_y: 0 };
        let changed = sm.apply_event(EngineEventKind::Jump, resting, 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "jump_up_left", "a live query's edge_hit: None here would instead fall through to 'fall'");
    }

    #[test]
    fn jump_while_climbing_a_left_facing_wall_jumps_off_to_the_right() {
        let schema = schema();
        let mut sm = StateMachine::from_snapshot(&schema, &snapshot_at(&schema, "climb_left", Direction::Left));
        let mut rng = StdRng::seed_from_u64(1);

        let resting = SurfaceContext { kind: SurfaceKind::Wall, edge_hit: sm.resting_edge(), floor_y: 0, ceiling_y: 0 };
        let changed = sm.apply_event(EngineEventKind::Jump, resting, 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "jump_right");
    }

    #[test]
    fn jump_while_airborne_falls_through_to_the_generic_fall_rule() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("fall");
        let mut rng = StdRng::seed_from_u64(1);

        let resting = SurfaceContext { kind: SurfaceKind::Air, edge_hit: sm.resting_edge(), floor_y: 0, ceiling_y: 0 };
        let changed = sm.apply_event(EngineEventKind::Jump, resting, 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "fall");
    }

    // Regression tests for the snapshot/resume mechanism: a real caller (the app's per-tick loop)
    // must rebuild a StateMachine from a schema it owns each tick (self-referential structs can't
    // be stored directly), so snapshot()/from_snapshot() have to actually preserve progress across
    // that rebuild — not just reset to frame 0 the way force_animation() does for tests.

    #[test]
    fn snapshot_and_resume_preserves_frame_progress_across_a_multi_frame_animation() {
        let schema = schema();
        let mut rng = StdRng::seed_from_u64(9);

        // walk_left has 4 frames of 6 ticks each (sprites 33, 1, 34, 2). Simulate the real
        // per-tick pattern: rebuild from the schema + last snapshot, step once, save the snapshot.
        let mut snapshot = {
            let mut sm = StateMachine::new(&schema);
            sm.force_animation("walk_left");
            sm.snapshot()
        };

        let mut sprites_seen = Vec::new();
        for _ in 0..24 {
            let mut sm = StateMachine::from_snapshot(&schema, &snapshot);
            let out = sm.step(no_edge(), 4, &mut rng);
            sprites_seen.push(out.sprite_index);
            snapshot = sm.snapshot();
        }

        // All 4 frames' sprites must have actually appeared — proves frame_index advanced past 0
        // across the rebuild-per-tick pattern instead of replaying frame 0 every time.
        assert!(sprites_seen.contains(&33));
        assert!(sprites_seen.contains(&1));
        assert!(sprites_seen.contains(&34));
        assert!(sprites_seen.contains(&2));
    }

    #[test]
    fn pending_movement_reports_the_current_frames_dx_dy_before_stepping() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("fall");
        assert_eq!(sm.pending_movement(), (0, 15));
    }

    #[test]
    fn resumed_state_machine_still_reaches_an_onfinish_transition() {
        let schema = schema();
        let mut rng = StdRng::seed_from_u64(11);

        // bounce_left is ONESHOT, 8+8=16 ticks total, then onFinish fires. Drive it entirely
        // through the snapshot/resume cycle, like a real per-tick caller would.
        let mut snapshot = {
            let mut sm = StateMachine::new(&schema);
            sm.force_animation("bounce_left");
            sm.snapshot()
        };
        let mut final_key = String::new();
        for _ in 0..16 {
            let mut sm = StateMachine::from_snapshot(&schema, &snapshot);
            sm.step(no_edge(), 4, &mut rng);
            snapshot = sm.snapshot();
            final_key = snapshot.current_key.clone();
        }
        assert!(final_key == "walk_left" || final_key == "walk_right");
    }
}
