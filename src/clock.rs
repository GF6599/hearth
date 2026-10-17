use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The time since a scene began, for a scene that follows the clock and not
/// the tick. A tick is late when the terminal is busy, and `+` and `-`
/// change how many ticks a frame runs, so ticks do not count real time.
#[derive(Clone)]
pub struct Clock(Rc<dyn Fn() -> Duration>);

impl Clock {
    /// Real time from now, on the monotonic clock, so a change to the
    /// wall clock cannot move it.
    pub fn monotonic() -> Self {
        let start = Instant::now();
        Self(Rc::new(move || start.elapsed()))
    }

    pub fn new(elapsed: impl Fn() -> Duration + 'static) -> Self {
        Self(Rc::new(elapsed))
    }

    pub fn elapsed(&self) -> Duration {
        (self.0)()
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Clock").field(&self.elapsed()).finish()
    }
}
