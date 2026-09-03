use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub trait MonitorSource {
    fn monitors(&self) -> Vec<Rect>;
}

pub trait WindowSource {
    fn windows(&self) -> Vec<Rect>;
}

pub struct EnvironmentTracker<M: MonitorSource, W: WindowSource> {
    pub monitor_source: M,
    pub window_source: W,
    refresh_interval: Duration,
    last_refresh: Option<Instant>,
    screen_rect: Rect,
    window_rects: Vec<Rect>,
}

impl<M: MonitorSource, W: WindowSource> EnvironmentTracker<M, W> {
    pub fn new(monitor_source: M, window_source: W, refresh_interval: Duration) -> Self {
        EnvironmentTracker {
            monitor_source,
            window_source,
            refresh_interval,
            last_refresh: None,
            screen_rect: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            window_rects: Vec::new(),
        }
    }

    pub fn poll(&mut self, now: Instant) -> (&Rect, &[Rect]) {
        let due = match self.last_refresh {
            None => true,
            Some(last) => now.duration_since(last) >= self.refresh_interval,
        };
        if due {
            self.screen_rect = combine_rects(&self.monitor_source.monitors());
            self.window_rects = self.window_source.windows();
            self.last_refresh = Some(now);
        }
        (&self.screen_rect, &self.window_rects)
    }
}

fn combine_rects(rects: &[Rect]) -> Rect {
    rects.iter().fold(
        Rect { left: i32::MAX, top: i32::MAX, right: i32::MIN, bottom: i32::MIN },
        |acc, r| Rect {
            left: acc.left.min(r.left),
            top: acc.top.min(r.top),
            right: acc.right.max(r.right),
            bottom: acc.bottom.max(r.bottom),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    struct CountingSource {
        rects: Vec<Rect>,
        calls: Cell<u32>,
    }
    impl MonitorSource for CountingSource {
        fn monitors(&self) -> Vec<Rect> {
            self.calls.set(self.calls.get() + 1);
            self.rects.clone()
        }
    }
    impl WindowSource for CountingSource {
        fn windows(&self) -> Vec<Rect> {
            self.calls.set(self.calls.get() + 1);
            self.rects.clone()
        }
    }

    #[test]
    fn combines_monitor_rects_into_a_virtual_screen_rect() {
        let monitors = CountingSource {
            rects: vec![
                Rect { left: 0, top: 0, right: 1920, bottom: 1080 },
                Rect { left: 1920, top: 0, right: 3840, bottom: 1080 },
            ],
            calls: Cell::new(0),
        };
        let windows = CountingSource { rects: vec![], calls: Cell::new(0) };
        let mut tracker = EnvironmentTracker::new(monitors, windows, Duration::from_millis(150));

        let (screen, _) = tracker.poll(Instant::now());
        assert_eq!(*screen, Rect { left: 0, top: 0, right: 3840, bottom: 1080 });
    }

    #[test]
    fn does_not_requery_before_the_refresh_interval_elapses() {
        let monitors = CountingSource { rects: vec![Rect { left: 0, top: 0, right: 100, bottom: 100 }], calls: Cell::new(0) };
        let windows = CountingSource { rects: vec![], calls: Cell::new(0) };
        let mut tracker = EnvironmentTracker::new(monitors, windows, Duration::from_millis(150));

        let t0 = Instant::now();
        tracker.poll(t0);
        tracker.poll(t0 + Duration::from_millis(50));
        assert_eq!(tracker.monitor_source.calls.get(), 1);

        tracker.poll(t0 + Duration::from_millis(200));
        assert_eq!(tracker.monitor_source.calls.get(), 2);
    }
}
