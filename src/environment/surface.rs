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

    let hosting = windows
        .iter()
        .filter(|w| w.left <= center_x && center_x < w.right && (w.top - feet_y).abs() <= GROUND_TOLERANCE)
        .min_by_key(|w| w.right - w.left)
        .copied();

    let on_ground = if let Some(floor_window) = hosting {
        (floor_window.top - feet_y).abs() <= GROUND_TOLERANCE
    } else {
        (screen.bottom - feet_y).abs() <= GROUND_TOLERANCE
    };

    if !on_ground {
        return SurfaceContext { kind: SurfaceKind::Air, edge_hit: None };
    }

    let floor = hosting.unwrap_or(*screen);
    let mut edge_hit = None;
    if dx < 0 && mascot_x + dx <= floor.left {
        edge_hit = Some(Edge::Left);
    } else if dx > 0 && mascot_x + mascot_width + dx >= floor.right {
        edge_hit = Some(Edge::Right);
    }

    SurfaceContext { kind: SurfaceKind::Ground, edge_hit }
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
}
