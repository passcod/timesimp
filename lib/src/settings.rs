use std::time::Duration;

/// Settings for a [`Timesimp`](crate::Timesimp).
///
/// Values set will be clamped to acceptable ones before use (e.g. setting samples to 10 will
/// result in a value of 11 being selected).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Settings {
    /// How many samples to gather for synchronisation.
    ///
    /// Must be odd, minimum 3, default 5.
    pub samples: u8,

    /// The maximum amount of time between taking two samples.
    ///
    /// The actual value will be random.
    ///
    /// Must be more than 10µs, less than 10s, default 100ms.
    pub jitter: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            samples: 5,
            jitter: Duration::from_secs(2),
        }
    }
}

impl Settings {
    /// Clamp to acceptable values.
    pub(crate) fn clamp(self) -> Self {
        Self {
            samples: if self.samples % 2 == 0 {
                self.samples.saturating_add(1)
            } else {
                self.samples
            }
            .clamp(3, 255),
            jitter: self
                .jitter
                .clamp(Duration::from_micros(10), Duration::from_secs(10)),
        }
    }
}
