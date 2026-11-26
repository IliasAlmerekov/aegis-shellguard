use time::OffsetDateTime;

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock(OffsetDateTime);

impl FixedClock {
    pub fn new(timestamp: OffsetDateTime) -> Self {
        Self(timestamp)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0
    }
}
