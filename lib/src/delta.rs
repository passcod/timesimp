use std::time::Duration;

use jiff::{SignedDuration, SpanRelativeTo, Timestamp};

use crate::Response;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Delta {
    pub(crate) latency: Duration,
    pub(crate) delta: SignedDuration,
}

impl Delta {
    /// The delta calculation for a single return packet.
    ///
    /// The idea is to compute the round trip time, then {half that + the sent time} calculates the
    /// local time at the moment the server stamped the response. Then comparing that moment to the
    /// server time gives us the delta to apply to the local clock.
    ///
    /// The tests below have diagrams that may make things clearer.
    ///
    /// Returns None if latency is negative, ie local clock went backwards.
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new(response: Response, current: Timestamp) -> Option<Self> {
        let latency = (current - response.client)
            .to_duration(SpanRelativeTo::days_are_24_hours())
            .unwrap()
            / 2;
        let local_at_midpoint = response.client + latency;
        let delta = (response.server - local_at_midpoint)
            .to_duration(SpanRelativeTo::days_are_24_hours())
            .unwrap();
        tracing::trace!(
            ?latency,
            ?local_at_midpoint,
            ?delta,
            "response processing internals"
        );

        Duration::try_from(latency)
            .ok()
            .map(|latency| Self { latency, delta })
    }
}

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use super::*;

    #[test]
    fn client_ahead_of_server() {
        /*
            c=3 | \   |
                |  \  |
                | 3 \ |
                |    \|
            c=6 |-----| s=2
            s=2 |    /|       -- offset=-4
                | 3 / |
                |  /  |
                | /   |
                |/    |
        */

        let client_time = Timestamp::new(0, 300).unwrap();
        let server_time = Timestamp::new(0, 200).unwrap();
        let round_trip = SignedDuration::from_nanos(600);

        let response = Response {
            client: client_time,
            server: server_time,
        };

        let processed = Delta::new(response, client_time + round_trip).unwrap();

        assert_eq!(processed.latency, Duration::from_nanos(300), "latency");
        assert_eq!(processed.delta, SignedDuration::from_nanos(-400), "delta");
    }

    #[test]
    fn client_behind_server() {
        /*
            c=5 | \   |
                |  \  |
                | 4 \ |
                |    \|
            c=9 |-----| s=12
            s=12|    /|       -- offset=+3
                | 4 / |
                |  /  |
                | /   |
                |/    |
        */

        let client_time = Timestamp::new(0, 500).unwrap();
        let server_time = Timestamp::new(0, 1200).unwrap();
        let round_trip = SignedDuration::from_nanos(800);

        let response = Response {
            client: client_time,
            server: server_time,
        };

        let processed = Delta::new(response, client_time + round_trip).unwrap();

        assert_eq!(processed.latency, Duration::from_nanos(400), "latency");
        assert_eq!(processed.delta, SignedDuration::from_nanos(300), "delta");
    }

    #[test]
    fn client_equal_server() {
        /*
            c=5 | \   |
                |  \  |
                | 2 \ |
                |    \|
            c=7 |-----| s=7
            s=7 |    /|       -- offset=0
                | 2 / |
                |  /  |
                | /   |
                |/    |
        */

        let client_time = Timestamp::new(0, 500).unwrap();
        let server_time = Timestamp::new(0, 700).unwrap();
        let round_trip = SignedDuration::from_nanos(400);

        let response = Response {
            client: client_time,
            server: server_time,
        };

        let processed = Delta::new(response, client_time + round_trip).unwrap();

        assert_eq!(processed.latency, Duration::from_nanos(200), "latency");
        assert_eq!(processed.delta, SignedDuration::from_nanos(0), "delta");
    }

    #[test]
    fn clock_went_backwards() {
        let sent_time = Timestamp::new(0, 500).unwrap();
        let server_time = Timestamp::new(0, 700).unwrap();
        let arrive_time = Timestamp::new(0, 200).unwrap();

        let response = Response {
            client: sent_time,
            server: server_time,
        };

        let proc = Delta::new(response, arrive_time);
        assert!(proc.is_none(), "{proc:?}");
    }

    #[test]
    fn with_sleep() {
        let sent_time = Timestamp::now();
        sleep(Duration::from_millis(10));
        let server_time = Timestamp::now();
        sleep(Duration::from_millis(10));
        let arrive_time = Timestamp::now();

        let response = Response {
            client: sent_time,
            server: server_time,
        };

        let processed = Delta::new(response, arrive_time).unwrap();

        if cfg!(target_os = "linux") {
            assert!(
                processed.latency > Duration::from_millis(9)
                    && processed.latency < Duration::from_millis(11),
                "latency {:?}",
                processed.latency
            );
            assert!(
                processed.delta >= SignedDuration::from_micros(-100)
                    && processed.delta <= SignedDuration::from_micros(100),
                "delta {:?}",
                processed.delta
            );
        } else {
            // on Mac and windows the sleep timer is too rough and those precise intervals fail
            assert!(
                processed.latency > Duration::from_millis(1)
                    && processed.latency < Duration::from_millis(100),
                "latency {:?}",
                processed.latency
            );
            assert!(
                processed.delta >= SignedDuration::from_millis(-20)
                    && processed.delta <= SignedDuration::from_millis(20),
                "delta {:?}",
                processed.delta
            );
        }
    }
}
