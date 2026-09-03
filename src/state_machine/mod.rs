pub mod weighted;

use crate::format::animation::{Animation, AnimationSchema, Direction, Edge};
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
}

#[derive(Debug, Clone, Copy)]
pub struct StepOutput {
    pub sprite_index: u32,
    pub dx: i32,
    pub dy: i32,
    pub changed_animation: bool,
}

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
        let mut sm = StateMachine {
            schema,
            by_key,
            current,
            frame_index: 0,
            frame_ticks_remaining: current.frames[0].duration_ticks,
            ticks_in_animation: 0,
            facing: current.direction,
            timer_targets: Vec::new(),
            max_duration_target: None,
        };
        sm.resolve_timers_for_current(&mut fresh_rng());
        sm
    }

    pub fn force_animation(&mut self, key: &str) {
        self.enter_animation(key, self.facing, &mut fresh_rng());
    }

    pub fn current_key(&self) -> &str {
        &self.current.key
    }

    pub fn facing(&self) -> Direction {
        self.facing
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
    use crate::format::animation::AnimationSchema;
    use rand::prelude::*;

    fn schema() -> AnimationSchema {
        let json = std::fs::read_to_string("tests/fixtures/sample_animation.json").unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn no_edge() -> SurfaceContext {
        SurfaceContext { kind: SurfaceKind::Ground, edge_hit: None }
    }

    #[test]
    fn starts_on_the_default_animation() {
        let schema = schema();
        let sm = StateMachine::new(&schema);
        assert_eq!(sm.current_key(), "fall");
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

        let hit_left = SurfaceContext { kind: SurfaceKind::Ground, edge_hit: Some(crate::format::animation::Edge::Left) };
        sm.step(hit_left, 4, &mut rng);
        assert!(sm.current_key() == "climb_left" || sm.current_key() == "walk_right");
    }
}
