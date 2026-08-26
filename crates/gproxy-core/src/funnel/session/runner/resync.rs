const DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Default)]
pub(super) struct Resync {
    started: Option<web_time::Instant>,
    failures: u8,
}

impl Resync {
    pub(super) fn start(&mut self) {
        self.started = Some(web_time::Instant::now());
    }

    pub(super) fn ready(&mut self) {
        self.started = None;
        self.failures = 0;
    }

    pub(super) fn expired(&self) -> bool {
        self.started
            .is_some_and(|started| started.elapsed() >= DEADLINE)
    }

    pub(super) fn failure_limit_reached(&mut self) -> bool {
        self.failures += 1;
        self.failures >= 3
    }

    pub(super) fn wake_in(&self, renew_in: std::time::Duration) -> std::time::Duration {
        self.started.map_or(renew_in, |started| {
            renew_in.min(DEADLINE.saturating_sub(started.elapsed()))
        })
    }
}
