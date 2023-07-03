use std::time::{Instant, Duration};

pub struct IdleTracker {
    last_active: Instant,
    max_idle: Duration,
}

impl IdleTracker {
    pub fn new(max_idle:Duration) -> IdleTracker {
        let last_active = Instant::now();
        return IdleTracker { 
            last_active,
            max_idle
         };
    }

    pub fn mark(&mut self) -> Instant {
        let result = self.last_active;
        self.last_active = Instant::now();
        return result;
    }

    pub fn max_idle(&self) -> Duration {
        self.max_idle
    }

    pub fn is_expired(&self) -> bool {
        return self.last_active.elapsed() > self.max_idle;
    }

    pub fn idled_for(&self) -> Duration {
        self.last_active.elapsed()
    }
}