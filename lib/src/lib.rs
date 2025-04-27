//! Simple sans-io timesync client and server.
//!
//! Timesimp is based on the averaging method described in [Simpson (2002), A Stream-based Time
//! Synchronization Technique For Networked Computer Games][paper], but with a corrected delta
//! calculation. Compared to NTP, it's a simpler and less accurate time synchronisation algorithm
//! that is usable over network streams, rather than datagrams. Simpson asserts they were able to
//! achieve accuracies of 100ms or better, which is sufficient in many cases; my testing gets
//! accuracies well below 1ms. The main limitation of the algorithm is that round-trip-time is
//! assumed to be symmetric: if the forward trip time is different from the return trip time, then
//! an error is induced equal to the value of the difference in trip times.
//!
//! This library provides a sans-io implementation: you bring in your async runtime, your transport,
//! and your storage; timesimp gives you time offsets.
//!
//! If the local clock goes backward during a synchronisation, the invalid delta is discarded; this
//! may cause the sync attempt to fail, especially if the `samples` count is lowered to its minimum
//! of 3. This is a deliberate design decision: you should handle failure and retry, and the sync
//! will proceed correctly when the clock is stable.
//!
//! [paper]: https://web.archive.org/web/20160310125700/http://mine-control.com/zack/timesync/timesync.html
//!
//! # Example
//!
//! ```no_run
//! use std::{convert::Infallible, time::Duration};
//! use reqwest::{Client, Url};
//! use timesimp::{SignedDuration, Timesimp};
//!
//! struct ServerSimp;
//! impl Timesimp for ServerSimp {
//!     type Err = Infallible;
//!
//!     async fn load_offset(&self) -> Result<Option<SignedDuration>, Self::Err> {
//!         // server time is correct
//!         Ok(Some(SignedDuration::ZERO))
//!     }
//!
//!     async fn store_offset(&mut self, _offset: SignedDuration) -> Result<(), Self::Err> {
//!         // as time is correct, no need to store offset
//!         unimplemented!()
//!     }
//!
//!     async fn query_server(
//!         &self,
//!         _request: timesimp::Request,
//!     ) -> Result<timesimp::Response, Self::Err> {
//!         // server has no upstream timesimp
//!         unimplemented!()
//!     }
//!
//!     async fn sleep(duration: std::time::Duration) {
//!         tokio::time::sleep(duration).await;
//!     }
//! }
//!
//! // Not shown: serving ServerSimp from a URL
//!
//! struct ClientSimp {
//!     offset: Option<SignedDuration>,
//!     url: Url,
//! }
//!
//! impl Timesimp for ClientSimp {
//!     type Err = reqwest::Error;
//!
//!     async fn load_offset(&self) -> Result<Option<SignedDuration>, Self::Err> {
//!         Ok(self.offset)
//!     }
//!
//!     async fn store_offset(&mut self, offset: SignedDuration) -> Result<(), Self::Err> {
//!         self.offset = Some(offset);
//!         Ok(())
//!     }
//!
//!     async fn query_server(
//!         &self,
//!         request: timesimp::Request,
//!     ) -> Result<timesimp::Response, Self::Err> {
//!         let resp = Client::new()
//!             .post(self.url.clone())
//!             .body(request.to_bytes().to_vec())
//!             .send()
//!             .await?
//!             .error_for_status()?
//!             .bytes()
//!             .await?;
//!         Ok(timesimp::Response::try_from(&resp[..]).unwrap())
//!     }
//!
//!     async fn sleep(duration: std::time::Duration) {
//!         tokio::time::sleep(duration).await;
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut client = ClientSimp {
//!         offset: None,
//!         url: "https://timesimp.server".try_into().unwrap(),
//!     };
//!
//!     loop {
//!         if let Ok(Some(offset)) = client.attempt_sync(Default::default()).await {
//!             println!(
//!                 "Received offset: {offset:?}; current time is {}",
//!                 client.adjusted_timestamp().await.unwrap(),
//!             );
//!         }
//!         tokio::time::sleep(Duration::from_secs(300)).await;
//!     }
//! }
//! ```

use std::time::Duration;

pub use jiff::{SignedDuration, Timestamp};

mod delta;
use delta::*;

mod messages;
pub use messages::*;

mod settings;
pub use settings::*;

/// A time sync client and/or server.
///
/// You must implement the four required functions and not override the others.
///
/// Then, use `answer_client()` to implement a time sync server, and/or use `attempt_sync()` to
/// implement a time sync client.
#[allow(async_fn_in_trait)]
pub trait Timesimp {
    /// Error for your required methods.
    type Err: std::error::Error;

    /// Load the current time offset.
    ///
    /// This must return the current stored time offset, or `None` if no time offset is currently
    /// stored.
    ///
    /// As this expects a `SignedDuration`, you are free to load whatever precision you wish, so
    /// long as `store_offset()` agrees. Microseconds should be enough for most purposes.
    async fn load_offset(&self) -> Result<Option<SignedDuration>, Self::Err>;

    /// Store the current time offset.
    ///
    /// This must store the given time offset, typically in some kind of database.
    ///
    /// As this is given a `SignedDuration`, you are free to store whatever precision you wish, so
    /// long as `load_offset()` agrees. Microseconds should be enough for most purposes.
    ///
    /// Additionally, once `store_offset` has been called once, `load_offset` should return `Some`.
    async fn store_offset(&mut self, offset: SignedDuration) -> Result<(), Self::Err>;

    /// Query a timesimp server endpoint.
    ///
    /// This must query in some manner a timesimp server, by sending the given [`Request`] and
    /// obtaining a [`Response`]. Both [`Request`] and [`Response`] can be parsed from and
    /// serialized to bytes. The query implementation should do as little else as possible to
    /// avoid adding unnecessary latency.
    ///
    /// If using a connecting protocol, such as TCP or QUIC, it's recommended to keep the
    /// connection alive if practicable, with a timeout longer than the
    /// [`Settings.jitter`](Settings) value. That should result in all but the first sample being
    /// approximately a single round trip, eliminating the handshake delay.
    async fn query_server(&self, request: Request) -> Result<Response, Self::Err>;

    /// Sleep for a [`Duration`].
    ///
    /// This is usually something like `tokio::time::sleep` or equivalent.
    async fn sleep(duration: Duration);

    /// Obtain an adjusted timestamp.
    ///
    /// Do not override.
    ///
    /// This simply loads the offset and applies it to the current local timestamp.
    ///
    /// It is provided as convenience for simple use; you may want to implement your own.
    async fn adjusted_timestamp(&self) -> Result<Timestamp, Self::Err> {
        let offset = self.load_offset().await?.unwrap_or_default();
        Ok(Timestamp::now() + offset)
    }

    /// The implementation of the server endpoint.
    ///
    /// Do not override.
    ///
    /// Use this in your server endpoint implementation. Both [`Request`] and [`Response`] can be
    /// parsed from and serialized to bytes. The endpoint should do as little else as possible to
    /// avoid adding unnecessary latency.
    async fn answer_client(&self, request: Request) -> Result<Response, Self::Err> {
        Ok(Response {
            client: request.client,
            server: self.adjusted_timestamp().await?,
        })
    }

    /// The main client state driver. Call this in a loop.
    ///
    /// You're expected to sleep for a while after calling this, or to run it on a schedule. Take
    /// care to compute your schedule on your raw system monotonic clock or equivalent, so it does
    /// not get influenced by the offset, which could make it jump around or even spin.
    ///
    /// If `load_offset()` returns `Ok(None)`, this method will attempt to `store_offset()` the
    /// first delta it gets from the server. This lets you get an "accurate enough" timestamp
    /// pretty quickly, instead of waiting for a full round of samples. Errors from that store are
    /// ignored silently.
    ///
    /// If this returns `Ok(None)`, not enough samples were obtained to have enough confidence in
    /// the result, likely because the `server_query()` method encountered an error for most tries.
    /// Errors from `server_query()` are not returned, but instead are logged using tracing.
    ///
    /// Do not override.
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     match simp.attempt_sync(Settings::default()).await {
    ///         Err(err) => eprintln!("{err}"),
    ///         Ok(None) => eprintln!("did not get enough samples to have confidence"),
    ///         Ok(Some(offset)) => {
    ///             println!("Obtained offset: {offset:?}");
    ///             println!("The adjusted time is {}", simp.adjusted_timestamp().unwrap());
    ///         }
    ///     }
    ///     sleep(Duration::from_secs(60));
    /// }
    /// ```
    async fn attempt_sync(
        &mut self,
        settings: Settings,
    ) -> Result<Option<SignedDuration>, Self::Err> {
        let Settings { samples, jitter } = settings.clamp();
        let current_offset = self.load_offset().await?.unwrap_or_default();
        tracing::trace!(?samples, ?current_offset, "starting delta collection");

        let mut gap = Duration::ZERO;
        let mut responses: Vec<Delta> = Vec::with_capacity(samples.into());
        for _ in 0..settings.samples {
            tracing::trace!(delay=?gap, max_jitter=?jitter, "sleeping to spread out requests");
            Self::sleep(gap).await;

            // compute the next gap before we query, so if query_server errors we don't immediately reloop
            gap = Duration::from_nanos(rand::random_range(
                0..=u64::try_from(jitter.as_nanos()).unwrap(),
            ));
            // UNWRAP: jitter has been clamped to 0..=10 seconds, so nanos will never reach u64::MAX

            let response = match self
                .query_server(Request {
                    client: Timestamp::now(),
                })
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    tracing::error!(?err, "query_server failed");
                    continue;
                }
            };

            let Some(packet) = Delta::new(response, Timestamp::now()) else {
                tracing::error!("local clock went backwards! skipping this sampling");
                continue;
            };

            tracing::trace!(latency=?packet.latency, delta=?packet.delta, "obtained raw offset from server");
            responses.push(packet);

            if self.load_offset().await?.is_none() {
                tracing::debug!(offset=?packet.delta, "no offset stored, storing initial delta");
                let _ = self.store_offset(packet.delta).await?;
            }
        }

        if responses.len() % 2 == 0 {
            // if we have an even number of responses, we need to discard one
            // the first response is most likely to be an outlier due to connection establishment
            responses.remove(0);
        }

        if responses.len() < 3 {
            tracing::debug!(
                count = responses.len(),
                "not enough responses for confidence"
            );
            return Ok(None);
        }

        responses.sort_by_key(|r| r.latency);
        let deltas = responses
            .iter()
            .map(|r| r.delta.as_millis_f64())
            .collect::<Vec<_>>();
        tracing::trace!(?deltas, "response deltas sorted by latency");

        let median_idx = deltas.len() / 2;
        let median = deltas[median_idx];

        let mean: f64 = deltas.iter().copied().sum::<f64>() / deltas.len() as f64;
        let variance: f64 = deltas
            .iter()
            .copied()
            .map(|d| (d - mean).powi(2))
            .sum::<f64>()
            / ((deltas.len() - 1) as f64);
        let stddev: f64 = variance.sqrt();
        tracing::trace!(
            ?median,
            ?mean,
            ?variance,
            ?stddev,
            "statistics about response deltas"
        );

        let inliers = deltas
            .iter()
            .copied()
            .filter(|d| *d >= median - stddev && *d <= median + stddev)
            .collect::<Vec<_>>();
        tracing::trace!(?inliers, "eliminated outliers");

        let offset = SignedDuration::from_micros(
            ((inliers.iter().sum::<f64>() / (inliers.len() as f64)) * 1000.0) as i64,
        );

        tracing::debug!(?offset, "storing calculated offset");
        self.store_offset(offset).await?;
        return Ok(Some(offset));
    }
}
