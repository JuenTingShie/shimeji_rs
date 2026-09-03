use std::time::Instant;

const TAP_MAX_DISTANCE_PX: f64 = 6.0;
const TAP_MAX_DURATION_SECS: f64 = 0.25;

#[derive(Debug, Clone, Copy)]
pub struct PointerSample {
    pub time: Instant,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum ReleaseKind {
    Tap,
    Fling { vx: f64, vy: f64 },
}

pub fn classify_release(down: PointerSample, up: PointerSample) -> ReleaseKind {
    let dx = (up.x - down.x) as f64;
    let dy = (up.y - down.y) as f64;
    let distance = (dx * dx + dy * dy).sqrt();
    let duration = up.time.duration_since(down.time).as_secs_f64().max(1.0 / 1000.0);

    if distance <= TAP_MAX_DISTANCE_PX && duration <= TAP_MAX_DURATION_SECS {
        ReleaseKind::Tap
    } else {
        ReleaseKind::Fling { vx: dx / duration, vy: dy / duration }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sample_at(t: Instant, x: i32, y: i32) -> PointerSample {
        PointerSample { time: t, x, y }
    }

    #[test]
    fn a_quick_small_movement_is_a_tap() {
        let t0 = Instant::now();
        let down = sample_at(t0, 100, 100);
        let up = sample_at(t0 + Duration::from_millis(120), 103, 98);
        assert!(matches!(classify_release(down, up), ReleaseKind::Tap));
    }

    #[test]
    fn a_fast_large_movement_is_a_fling_with_velocity() {
        let t0 = Instant::now();
        let down = sample_at(t0, 100, 100);
        let up = sample_at(t0 + Duration::from_millis(100), 300, 50);
        match classify_release(down, up) {
            ReleaseKind::Fling { vx, vy } => {
                assert!(vx > 0.0, "expected rightward velocity, got {vx}");
                assert!(vy < 0.0, "expected upward velocity, got {vy}");
            }
            ReleaseKind::Tap => panic!("expected a fling"),
        }
    }

    #[test]
    fn a_slow_large_movement_is_still_a_drag_release_not_a_tap() {
        let t0 = Instant::now();
        let down = sample_at(t0, 100, 100);
        let up = sample_at(t0 + Duration::from_secs(2), 400, 100);
        assert!(matches!(classify_release(down, up), ReleaseKind::Fling { .. }));
    }
}
