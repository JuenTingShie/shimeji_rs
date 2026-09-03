use super::Rect;
use crate::format::animation::Edge;
use crate::state_machine::{SurfaceContext, SurfaceKind};

const GROUND_TOLERANCE: i32 = 2;

pub fn query_surface(
    mascot_x: i32,
    mascot_y: i32,
    mascot_width: i32,
    mascot_height: i32,
    dx: i32,
    dy: i32,
    screen: &Rect,
    windows: &[Rect],
) -> SurfaceContext {
    let feet_y = mascot_y + mascot_height;
    let center_x = mascot_x + mascot_width / 2;

    // The floor directly under the mascot: the topmost window edge the mascot's FEET haven't
    // already fallen past (comparing against feet_y, not mascot_y — a window between the
    // mascot's top and its feet has already been passed, not one still ahead to land on), else
    // the screen bottom.
    let hosting = windows
        .iter()
        .filter(|w| w.left <= center_x && center_x < w.right && w.top >= feet_y - GROUND_TOLERANCE)
        .min_by_key(|w| w.top)
        .copied();
    let floor_y = hosting.map(|w| w.top).unwrap_or(screen.bottom);

    let on_ground = (floor_y - feet_y).abs() <= GROUND_TOLERANCE;

    if on_ground {
        let floor = hosting.unwrap_or(*screen);
        let mut edge_hit = None;
        if dx < 0 && mascot_x + dx <= floor.left {
            edge_hit = Some(Edge::Left);
        } else if dx > 0 && mascot_x + mascot_width + dx >= floor.right {
            edge_hit = Some(Edge::Right);
        }
        return SurfaceContext { kind: SurfaceKind::Ground, edge_hit };
    }

    // Not resting on the floor yet — if this tick's fall would reach or pass it, report the
    // BOTTOM edge now so the caller's border transition (e.g. fall -> bounce) fires on landing,
    // rather than the mascot falling straight through and off the bottom of the world.
    if dy > 0 && feet_y + dy >= floor_y {
        return SurfaceContext { kind: SurfaceKind::Air, edge_hit: Some(Edge::Bottom) };
    }

    SurfaceContext { kind: SurfaceKind::Air, edge_hit: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };

    #[test]
    fn ground_with_no_windows_is_the_screen_floor() {
        let ctx = query_surface(500, 1080 - 64, 64, 64, 0, 0, &SCREEN, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn walking_left_off_the_screen_hits_left_edge() {
        let ctx = query_surface(0, 1080 - 64, 64, 64, -2, 0, &SCREEN, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn walking_right_off_the_screen_hits_right_edge() {
        let ctx = query_surface(1920 - 64, 1080 - 64, 64, 64, 2, 0, &SCREEN, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Right));
    }

    #[test]
    fn standing_on_top_of_another_window_uses_that_window_as_ground() {
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        let ctx = query_surface(500, 400 - 64, 64, 64, 0, 0, &SCREEN, &[browser]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn walking_off_the_left_edge_of_a_hosting_window() {
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        let ctx = query_surface(200, 400 - 64, 64, 64, -2, 0, &SCREEN, &[browser]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn falling_in_open_air_is_air_with_no_edge() {
        let ctx = query_surface(500, 300, 64, 64, 0, 15, &SCREEN, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Air);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn falling_past_the_screen_floor_this_tick_reports_bottom_edge() {
        // feet at 1075, falling by 15 would put feet at 1090 — past the 1080 floor.
        let ctx = query_surface(500, 1075 - 64, 64, 64, 0, 15, &SCREEN, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Bottom));
    }

    #[test]
    fn a_window_the_mascot_has_already_fallen_past_does_not_reappear_as_a_floor() {
        // Regression test for a real bug: comparing a candidate window's top against mascot_y
        // (the sprite's top) instead of feet_y (its bottom) let windows the mascot had already
        // fallen past keep registering as the floor, causing edge_hit to flicker on/off as the
        // mascot fell near real desktop windows instead of only landing on ones still ahead.
        let already_passed = Rect { left: 0, top: 300, right: 1920, bottom: 350 };
        // mascot top at 500, feet at 564 — well past the window's top (300) and bottom (350).
        let ctx = query_surface(500, 500, 64, 64, 0, 15, &SCREEN, &[already_passed]);
        assert_eq!(ctx.edge_hit, None, "a window already fallen past must not be treated as the floor");
    }

    #[test]
    fn falling_mascot_repeatedly_stepped_eventually_lands_instead_of_falling_forever() {
        // Regression test for a real bug: without BOTTOM edge detection, a mascot falling at
        // dy=15/tick never registered as reaching the floor and fell forever past the screen.
        let mut y = 100;
        let mut hit_bottom = false;
        for _ in 0..200 {
            let ctx = query_surface(500, y, 64, 64, 0, 15, &SCREEN, &[]);
            if ctx.edge_hit == Some(Edge::Bottom) {
                hit_bottom = true;
                break;
            }
            y += 15;
            assert!(y < SCREEN.bottom + 100, "mascot fell past the floor without ever hitting BOTTOM edge");
        }
        assert!(hit_bottom, "mascot never reported reaching the floor while falling");
    }
}
