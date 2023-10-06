use std::time::{Instant, Duration};

pub struct IdleTracker {
    last_active: Instant,
    max_idle: Duration,
}

impl IdleTracker {
    pub fn new(max_idle_ms: u64) -> IdleTracker {
        let mut max_idle_ms_local = max_idle_ms as u64;
        if max_idle_ms_local ==0 {
            max_idle_ms_local = std::u64::MAX;
        }
        let last_active = Instant::now();
        return IdleTracker { 
            last_active,
            max_idle: Duration::from_millis(max_idle_ms_local)
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