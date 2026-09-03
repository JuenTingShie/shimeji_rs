use super::Rect;
use crate::format::animation::Edge;
use crate::state_machine::{SurfaceContext, SurfaceKind};

const GROUND_TOLERANCE: i32 = 2;

/// The floor height (bottom y) of whichever physical monitor's horizontal span contains `x`,
/// or `None` if no monitor covers that x at all (off the edge of every screen). Looking this up
/// per-x, instead of using a single virtual-desktop bounding box, matters as soon as two
/// monitors differ in height: the bounding box's bottom is the *tallest* monitor's bottom, so a
/// mascot standing over a shorter monitor would treat empty space below it — past that
/// monitor's real edge, not corresponding to any screen — as walkable floor, and fall or walk
/// straight through into it.
fn floor_at(x: i32, monitors: &[Rect]) -> Option<i32> {
    monitors.iter().find(|m| m.left <= x && x < m.right).map(|m| m.bottom)
}

/// The ceiling height (top y) of whichever physical monitor's horizontal span contains `x`, or
/// `None` if none covers it. Symmetric to `floor_at` — a mismatched-height monitor pair has a
/// phantom-ceiling problem too, not just a phantom-floor one.
fn ceiling_at(x: i32, monitors: &[Rect]) -> Option<i32> {
    monitors.iter().find(|m| m.left <= x && x < m.right).map(|m| m.top)
}

pub fn query_surface(
    mascot_x: i32,
    mascot_y: i32,
    mascot_width: i32,
    mascot_height: i32,
    dx: i32,
    dy: i32,
    screen: &Rect,
    monitors: &[Rect],
    windows: &[Rect],
) -> SurfaceContext {
    let feet_y = mascot_y + mascot_height;
    let head_y = mascot_y;
    let mascot_left = mascot_x;
    let mascot_right = mascot_x + mascot_width;
    let center_x = mascot_x + mascot_width / 2;

    // The floor directly under the mascot: the topmost window edge the mascot's FEET haven't
    // already fallen past (comparing against feet_y, not mascot_y — a window between the
    // mascot's top and its feet has already been passed, not one still ahead to land on), else
    // the real monitor floor under this x, else (no monitor covers it — shouldn't normally
    // happen) the virtual-desktop bounding box as a last resort.
    //
    // "Under" means any horizontal overlap between the mascot's body and the window (an AABB
    // overlap test), not merely the mascot's center point falling inside the window's span. A
    // center-point test disagrees with the edge_hit check below (which uses the mascot's full
    // width) as soon as the mascot's center has crossed a window's edge but part of its body
    // still overlaps it: hosting_floor would drop out early, the floor would jump to whatever's
    // under the monitor instead, and the mascot would appear to fall before it had actually
    // walked off — well before edge_hit's own full-body definition of "walked off" ever fires.
    let hosting_floor = windows
        .iter()
        .filter(|w| w.left < mascot_right && w.right > mascot_left && w.top >= feet_y - GROUND_TOLERANCE)
        .min_by_key(|w| w.top)
        .copied();
    let floor_y = hosting_floor
        .map(|w| w.top)
        .or_else(|| floor_at(center_x, monitors))
        .unwrap_or(screen.bottom);

    // The ceiling directly above the mascot: symmetric to the floor lookup above — the lowest
    // window edge the mascot's HEAD hasn't already climbed past (a window between the mascot's
    // head and feet has already been passed, not one still ahead to bump into), else the real
    // monitor ceiling above this x, else the bounding box as a last resort. Same overlap test as
    // the floor lookup, for the same reason.
    let hosting_ceiling = windows
        .iter()
        .filter(|w| w.left < mascot_right && w.right > mascot_left && w.bottom <= head_y + GROUND_TOLERANCE)
        .max_by_key(|w| w.bottom)
        .copied();
    let ceiling_y = hosting_ceiling
        .map(|w| w.bottom)
        .or_else(|| ceiling_at(center_x, monitors))
        .unwrap_or(screen.top);

    let on_ground = (floor_y - feet_y).abs() <= GROUND_TOLERANCE;
    let on_ceiling = (head_y - ceiling_y).abs() <= GROUND_TOLERANCE;

    if on_ground {
        if let Some(w) = hosting_floor {
            let mut edge_hit = None;
            if dx < 0 && mascot_x + dx <= w.left {
                edge_hit = Some(Edge::Left);
            } else if dx > 0 && mascot_x + mascot_width + dx >= w.right {
                edge_hit = Some(Edge::Right);
            }
            return SurfaceContext { kind: SurfaceKind::Ground, edge_hit, floor_y, ceiling_y };
        }

        // Standing on a monitor's own desktop floor, not a window: this is an edge only if
        // stepping past it lands somewhere with a *different* floor height (a real cliff — a
        // monitor boundary with no floor, or an adjacent monitor whose floor doesn't line up),
        // not merely a different monitor. Same-height neighbors (the common side-by-side case)
        // stay walkable straight across the seam, exactly like the old single-bounding-box
        // floor did — only a genuine height mismatch (this bug's report: a vertical + a
        // horizontal monitor) now reads as an edge instead of phantom floor/empty air.
        let mut edge_hit = None;
        if dx < 0 {
            let leading_x = mascot_x + dx;
            let continues = matches!(floor_at(leading_x, monitors), Some(h) if (h - floor_y).abs() <= GROUND_TOLERANCE);
            if !continues {
                edge_hit = Some(Edge::Left);
            }
        } else if dx > 0 {
            let leading_x = mascot_x + mascot_width + dx;
            let continues = matches!(floor_at(leading_x, monitors), Some(h) if (h - floor_y).abs() <= GROUND_TOLERANCE);
            if !continues {
                edge_hit = Some(Edge::Right);
            }
        }
        return SurfaceContext { kind: SurfaceKind::Ground, edge_hit, floor_y, ceiling_y };
    }

    if on_ceiling {
        if let Some(w) = hosting_ceiling {
            let mut edge_hit = None;
            if dx < 0 && mascot_x + dx <= w.left {
                edge_hit = Some(Edge::Left);
            } else if dx > 0 && mascot_x + mascot_width + dx >= w.right {
                edge_hit = Some(Edge::Right);
            }
            return SurfaceContext { kind: SurfaceKind::Ceiling, edge_hit, floor_y, ceiling_y };
        }

        // Hanging from a monitor's own top edge, not a window: same seam logic as the ground
        // case — only a genuine height mismatch between neighboring monitors' ceilings reads as
        // an edge, not merely crossing into the next monitor's x-range.
        let mut edge_hit = None;
        if dx < 0 {
            let leading_x = mascot_x + dx;
            let continues = matches!(ceiling_at(leading_x, monitors), Some(h) if (h - ceiling_y).abs() <= GROUND_TOLERANCE);
            if !continues {
                edge_hit = Some(Edge::Left);
            }
        } else if dx > 0 {
            let leading_x = mascot_x + mascot_width + dx;
            let continues = matches!(ceiling_at(leading_x, monitors), Some(h) if (h - ceiling_y).abs() <= GROUND_TOLERANCE);
            if !continues {
                edge_hit = Some(Edge::Right);
            }
        }
        return SurfaceContext { kind: SurfaceKind::Ceiling, edge_hit, floor_y, ceiling_y };
    }

    // Not resting on either surface yet — if this tick's movement would reach or pass one,
    // report the edge now so the caller's border transition (e.g. fall -> bounce, or
    // climb -> hang) fires on contact, rather than the mascot passing straight through and off
    // the edge of the world (previously true of BOTTOM; climbing up with nothing to stop it at
    // the top had exactly the same bug, just unnoticed since nothing fell through it visibly —
    // a mascot climbing a wall just drifted off into empty space above the screen instead).
    if dy > 0 && feet_y + dy >= floor_y {
        return SurfaceContext { kind: SurfaceKind::Air, edge_hit: Some(Edge::Bottom), floor_y, ceiling_y };
    }
    if dy < 0 && head_y + dy <= ceiling_y {
        return SurfaceContext { kind: SurfaceKind::Air, edge_hit: Some(Edge::Top), floor_y, ceiling_y };
    }

    // Same idea, horizontally: a purely (or diagonally) horizontal airborne animation -- e.g.
    // this schema's jump_right, dx: 10 / dy: 0, looping forever with no timer and its only exit a
    // RIGHT border transition into climb_right -- needs LEFT/RIGHT edges reported while airborne
    // too, or that transition can never fire and the mascot just jumps straight off the edge of
    // the screen forever instead of resuming the climb. Symmetric to floor_at/ceiling_at's use in
    // the grounded seam checks above: off the edge means no monitor covers the leading x at all.
    if dx < 0 {
        let leading_x = mascot_x + dx;
        if floor_at(leading_x, monitors).is_none() {
            return SurfaceContext { kind: SurfaceKind::Air, edge_hit: Some(Edge::Left), floor_y, ceiling_y };
        }
    } else if dx > 0 {
        let leading_x = mascot_x + mascot_width + dx;
        if floor_at(leading_x, monitors).is_none() {
            return SurfaceContext { kind: SurfaceKind::Air, edge_hit: Some(Edge::Right), floor_y, ceiling_y };
        }
    }

    SurfaceContext { kind: SurfaceKind::Air, edge_hit: None, floor_y, ceiling_y }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
    const MONITORS: [Rect; 1] = [SCREEN];

    #[test]
    fn ground_with_no_windows_is_the_screen_floor() {
        let ctx = query_surface(500, 1080 - 64, 64, 64, 0, 0, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn walking_left_off_the_screen_hits_left_edge() {
        let ctx = query_surface(0, 1080 - 64, 64, 64, -2, 0, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn walking_right_off_the_screen_hits_right_edge() {
        let ctx = query_surface(1920 - 64, 1080 - 64, 64, 64, 2, 0, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Right));
    }

    #[test]
    fn standing_on_top_of_another_window_uses_that_window_as_ground() {
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        let ctx = query_surface(500, 400 - 64, 64, 64, 0, 0, &SCREEN, &MONITORS, &[browser]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn walking_off_the_left_edge_of_a_hosting_window() {
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        let ctx = query_surface(200, 400 - 64, 64, 64, -2, 0, &SCREEN, &MONITORS, &[browser]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn a_mascot_mostly_off_a_window_but_still_partially_overlapping_stays_hosted_by_it() {
        // Regression test for a real bug: hosting_floor required the mascot's CENTER point to
        // fall within the window's x-range, so a mascot whose center had already passed the
        // window's right edge -- but whose body still partially overlapped it -- lost the window
        // as its floor and fell back to the (much lower) monitor floor, well before edge_hit's
        // own full-body definition of "walked off" fired. Any overlap must keep it hosted.
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        // mascot spans [880, 944) -- center (912) is past the window's right edge (900), but
        // 20px of the mascot's body (880..900) still overlaps it.
        let ctx = query_surface(880, 400 - 64, 64, 64, 0, 0, &SCREEN, &MONITORS, &[browser]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.floor_y, 400, "must still use the window's floor while any part of the mascot overlaps it");
    }

    #[test]
    fn a_mascot_mostly_off_a_ceiling_window_but_still_partially_overlapping_stays_hosted_by_it() {
        // Ceiling analogue of the floor regression above.
        let shelf = Rect { left: 200, top: 200, right: 900, bottom: 700 };
        // mascot spans [880, 944) -- center (912) is past the window's right edge (900), but
        // 20px of the mascot's body (880..900) still overlaps it.
        let ctx = query_surface(880, 700, 64, 64, 0, 0, &SCREEN, &MONITORS, &[shelf]);
        assert_eq!(ctx.kind, SurfaceKind::Ceiling);
        assert_eq!(ctx.ceiling_y, 700, "must still use the window's ceiling while any part of the mascot overlaps it");
    }

    #[test]
    fn an_airborne_mascot_moving_purely_horizontally_off_the_left_edge_reports_left() {
        // Regression test for a real bug: the "not resting on anything" branch only ever checked
        // dy for TOP/BOTTOM, never dx for LEFT/RIGHT. An airborne animation that moves purely (or
        // diagonally) sideways -- like this schema's jump_right/jump_left, whose only exit is
        // their own LEFT/RIGHT border transition back into climbing -- could never trigger it and
        // would just jump straight off the edge of the screen forever.
        let ctx = query_surface(2, 400, 64, 64, -5, 0, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Air);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn an_airborne_mascot_moving_purely_horizontally_off_the_right_edge_reports_right() {
        let ctx = query_surface(1920 - 64 - 2, 400, 64, 64, 5, 0, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Air);
        assert_eq!(ctx.edge_hit, Some(Edge::Right));
    }

    #[test]
    fn falling_in_open_air_is_air_with_no_edge() {
        let ctx = query_surface(500, 300, 64, 64, 0, 15, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Air);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn falling_past_the_screen_floor_this_tick_reports_bottom_edge() {
        // feet at 1075, falling by 15 would put feet at 1090 — past the 1080 floor.
        let ctx = query_surface(500, 1075 - 64, 64, 64, 0, 15, &SCREEN, &MONITORS, &[]);
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
        let ctx = query_surface(500, 500, 64, 64, 0, 15, &SCREEN, &MONITORS, &[already_passed]);
        assert_eq!(ctx.edge_hit, None, "a window already fallen past must not be treated as the floor");
    }

    #[test]
    fn falling_mascot_repeatedly_stepped_eventually_lands_instead_of_falling_forever() {
        // Regression test for a real bug: without BOTTOM edge detection, a mascot falling at
        // dy=15/tick never registered as reaching the floor and fell forever past the screen.
        let mut y = 100;
        let mut hit_bottom = false;
        for _ in 0..200 {
            let ctx = query_surface(500, y, 64, 64, 0, 15, &SCREEN, &MONITORS, &[]);
            if ctx.edge_hit == Some(Edge::Bottom) {
                hit_bottom = true;
                break;
            }
            y += 15;
            assert!(y < SCREEN.bottom + 100, "mascot fell past the floor without ever hitting BOTTOM edge");
        }
        assert!(hit_bottom, "mascot never reported reaching the floor while falling");
    }

    // Regression tests for a real bug: a two-monitor setup with one horizontal (1920x1080) and
    // one vertical (1080x1920) monitor side by side used to compute the "floor" from
    // combine_rects' virtual-desktop bounding box, whose bottom is the *taller* monitor's
    // bottom (1920) everywhere — including over the horizontal monitor, where nothing exists
    // below y=1080. A mascot there would fall through its own monitor's real floor into that
    // phantom space, or use the wrong landing height entirely.

    const HORIZONTAL: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
    const VERTICAL: Rect = Rect { left: 1920, top: 0, right: 3000, bottom: 1920 };
    const MISMATCHED: [Rect; 2] = [HORIZONTAL, VERTICAL];

    #[test]
    fn falling_over_the_shorter_monitor_lands_on_its_own_floor_not_the_taller_neighbor() {
        // Bounding box bottom would be 1920 (the vertical monitor's); the real floor here,
        // over the horizontal monitor, is 1080.
        let ctx = query_surface(500, 1075 - 64, 64, 64, 0, 15, &HORIZONTAL, &MISMATCHED, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Bottom), "must land at the horizontal monitor's own floor (1080), not fall through toward 1920");
    }

    #[test]
    fn a_mascot_resting_on_the_shorter_monitor_is_not_reported_as_still_airborne() {
        let ctx = query_surface(500, 1080 - 64, 64, 64, 0, 0, &HORIZONTAL, &MISMATCHED, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Ground, "feet at the horizontal monitor's own floor (1080) must read as grounded");
    }

    #[test]
    fn walking_from_the_horizontal_monitor_toward_the_taller_vertical_one_hits_an_edge() {
        // Standing at the right edge of the horizontal monitor (floor 1080), about to step onto
        // the vertical monitor's x-range, whose floor (1920) is 840px lower — not a walkable
        // continuation, so this must read as an edge rather than silently teleporting down.
        let ctx = query_surface(1920 - 64, 1080 - 64, 64, 64, 2, 0, &HORIZONTAL, &MISMATCHED, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Right), "a floor-height mismatch between neighboring monitors must read as an edge");
    }

    #[test]
    fn walking_between_two_same_height_monitors_stays_seamless() {
        // Two ordinary side-by-side monitors at the same height must NOT trigger a spurious
        // edge at the seam — only an actual height mismatch should.
        let left_monitor = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        let right_monitor = Rect { left: 1920, top: 0, right: 3840, bottom: 1080 };
        let monitors = [left_monitor, right_monitor];
        let ctx = query_surface(1920 - 64, 1080 - 64, 64, 64, 2, 0, &left_monitor, &monitors, &[]);
        assert_eq!(ctx.edge_hit, None, "same-height neighboring monitors must stay walkable across the seam");
    }

    // Regression tests for a real bug: query_surface never resolved a ceiling at all — a mascot
    // climbing a wall (dy < 0) had nothing to stop it at the top, so it just drifted straight
    // through the top of the screen into empty space above the monitor forever, instead of
    // reaching climb_left/climb_right's own TOP border transition into climb_ceiling_*.

    #[test]
    fn climbing_past_the_screen_ceiling_this_tick_reports_top_edge() {
        // head at y=5, climbing by -8 would put the head at -3 — past the ceiling at 0.
        let ctx = query_surface(500, 5, 64, 64, 0, -8, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Top));
    }

    #[test]
    fn a_mascot_resting_at_the_ceiling_is_not_reported_as_still_airborne() {
        let ctx = query_surface(500, 0, 64, 64, 0, 0, &SCREEN, &MONITORS, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Ceiling, "head at the screen's own ceiling (0) must read as hanging, not airborne");
    }

    #[test]
    fn climbing_mascot_repeatedly_stepped_eventually_reaches_the_ceiling_instead_of_climbing_forever() {
        let mut y = 500;
        let mut hit_top = false;
        for _ in 0..200 {
            let ctx = query_surface(500, y, 64, 64, 0, -8, &SCREEN, &MONITORS, &[]);
            if ctx.edge_hit == Some(Edge::Top) {
                hit_top = true;
                break;
            }
            y -= 8;
            assert!(y > SCREEN.top - 100, "mascot climbed past the ceiling without ever hitting TOP edge");
        }
        assert!(hit_top, "mascot never reported reaching the ceiling while climbing");
    }

    #[test]
    fn hanging_from_a_window_uses_that_windows_underside_as_ceiling() {
        // A window from y=200 to y=700 sitting above the mascot: its underside (700) is the
        // ceiling, not the screen's own top (0).
        let shelf = Rect { left: 200, top: 200, right: 900, bottom: 700 };
        let ctx = query_surface(500, 700, 64, 64, 0, 0, &SCREEN, &MONITORS, &[shelf]);
        assert_eq!(ctx.kind, SurfaceKind::Ceiling);
        assert_eq!(ctx.ceiling_y, 700);
    }

    #[test]
    fn walking_off_the_left_edge_of_a_hosting_ceiling_window() {
        let shelf = Rect { left: 200, top: 200, right: 900, bottom: 700 };
        let ctx = query_surface(200, 700, 64, 64, -2, 0, &SCREEN, &MONITORS, &[shelf]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn a_window_the_mascot_has_already_climbed_past_does_not_reappear_as_a_ceiling() {
        // Mirrors the equivalent floor regression: a window whose bottom is well below where the
        // mascot's head already is must not register as still-ahead ceiling.
        let already_passed = Rect { left: 0, top: 750, right: 1920, bottom: 800 };
        // mascot head at 500 — above (climbed past) the window's bottom (800).
        let ctx = query_surface(500, 500, 64, 64, 0, -8, &SCREEN, &MONITORS, &[already_passed]);
        assert_eq!(ctx.edge_hit, None, "a window already climbed past must not be treated as the ceiling");
    }

    #[test]
    fn climbing_over_the_shorter_monitor_hits_its_own_ceiling_not_the_taller_neighbors() {
        // Reuses the mismatched horizontal/vertical monitor pair from the floor regression
        // tests above, but this time both monitors are top-aligned (top: 0) — so this test
        // instead checks a monitor pair whose TOPS mismatch (deliberately offset), the ceiling
        // analogue of the earlier floor-height mismatch.
        let low_top_monitor = Rect { left: 0, top: 200, right: 1920, bottom: 1080 };
        let high_top_monitor = Rect { left: 1920, top: 0, right: 3000, bottom: 1080 };
        let monitors = [low_top_monitor, high_top_monitor];
        // head at y=205, climbing by -8 would reach 197 — past this monitor's own ceiling (200).
        let ctx = query_surface(500, 205, 64, 64, 0, -8, &low_top_monitor, &monitors, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Top), "must hit its own monitor's ceiling (200), not fall through toward the neighbor's (0)");
    }

    #[test]
    fn climbing_toward_a_monitor_with_a_different_ceiling_height_hits_an_edge() {
        let low_top_monitor = Rect { left: 0, top: 200, right: 1920, bottom: 1080 };
        let high_top_monitor = Rect { left: 1920, top: 0, right: 3000, bottom: 1080 };
        let monitors = [low_top_monitor, high_top_monitor];
        // Hanging at the right edge of low_top_monitor's ceiling (200), about to step onto
        // high_top_monitor's x-range, whose ceiling (0) is 200px higher — not a walkable
        // continuation.
        let ctx = query_surface(1920 - 64, 200, 64, 64, 2, 0, &low_top_monitor, &monitors, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Right), "a ceiling-height mismatch between neighboring monitors must read as an edge");
    }

    #[test]
    fn hanging_between_two_same_height_ceilings_stays_seamless() {
        let left_monitor = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        let right_monitor = Rect { left: 1920, top: 0, right: 3840, bottom: 1080 };
        let monitors = [left_monitor, right_monitor];
        let ctx = query_surface(1920 - 64, 0, 64, 64, 2, 0, &left_monitor, &monitors, &[]);
        assert_eq!(ctx.edge_hit, None, "same-height neighboring ceilings must stay walkable across the seam");
    }
}
