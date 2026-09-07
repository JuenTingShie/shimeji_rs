use crate::format::animation::Frame;

/// shimeji-ee's default gravity for com.group_finity.mascot.action.Jump when the action
/// doesn't declare its own -- Jump only ever exposes VelocityParam in the mascots inspected,
/// never a Gravity override, so this baked implementation reuses shimeji-ee's own well-known
/// default gravity constant.
const JUMP_GRAVITY: f64 = 1.0;

/// Replicates com.group_finity.mascot.action.Fall's per-tick integration: velocity
/// accumulates downward gravity every tick, decaying by the configured resistance factors.
/// Runs for a fixed `ticks` budget (chosen by the caller) rather than until the mascot
/// reaches the floor -- this module has no floor-position feedback -- and the state
/// machine's existing border-transition handling takes over once the window actually
/// reaches the floor, ending the fall well before the budget is likely to run out.
pub fn bake_fall(gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    simulate(0.0, 0.0, gravity, resistance_x, resistance_y, sprite, ticks)
}

/// Replicates com.group_finity.mascot.action.Jump: an initial upward velocity (from the
/// action's VelocityParam) decaying under gravity, producing a rise-then-fall arc.
pub fn bake_jump(velocity_param: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    simulate(0.0, -velocity_param, JUMP_GRAVITY, 0.0, 0.0, sprite, ticks)
}

fn simulate(vx0: f64, vy0: f64, gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    let mut vx = vx0;
    let mut vy = vy0;
    let mut frames = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        vy += gravity;
        vx *= 1.0 - resistance_x;
        vy *= 1.0 - resistance_y;
        frames.push(Frame { sprite, dx: vx.round() as i32, dy: vy.round() as i32, duration_ticks: 1 });
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_fall_accelerates_downward_under_gravity() {
        let frames = bake_fall(2.0, 0.0, 0.0, 4, 5);
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0].dy, 2);
        assert_eq!(frames[1].dy, 4);
        assert_eq!(frames[4].dy, 10);
        assert!(frames.iter().all(|f| f.sprite == 4 && f.duration_ticks == 1 && f.dx == 0));
    }

    #[test]
    fn bake_fall_with_zero_gravity_never_moves() {
        let frames = bake_fall(0.0, 0.0, 0.0, 4, 3);
        assert!(frames.iter().all(|f| f.dx == 0 && f.dy == 0));
    }

    #[test]
    fn bake_fall_resistance_decays_velocity() {
        // 50% resistance each tick roughly halves the accumulated velocity every step,
        // so it must grow much slower than the zero-resistance case.
        let with_resistance = bake_fall(4.0, 0.0, 0.5, 0, 4);
        let without_resistance = bake_fall(4.0, 0.0, 0.0, 0, 4);
        assert!(with_resistance[3].dy < without_resistance[3].dy);
    }

    #[test]
    fn bake_jump_rises_before_falling() {
        let frames = bake_jump(20.0, 7, 60);
        assert!(frames[0].dy < 0, "jump must start moving upward, got {}", frames[0].dy);
        assert!(
            frames.last().unwrap().dy > 0,
            "should be falling (positive dy) again by the last simulated tick, got {}",
            frames.last().unwrap().dy
        );
        assert!(
            frames.windows(2).all(|w| w[1].dy >= w[0].dy),
            "gravity should monotonically increase dy (decelerating the rise, then accelerating the fall) every tick"
        );
        assert!(frames.iter().all(|f| f.sprite == 7 && f.duration_ticks == 1 && f.dx == 0));
    }
}
