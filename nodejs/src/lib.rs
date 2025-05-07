use std::{fmt, sync::Arc, time::Duration};

use napi::{
    bindgen_prelude::*,
    threadsafe_function::{ThreadsafeFunction},
};
use napi_derive::*;
use timesimp::{Request, Response, SignedDuration, Timesimp as _};
use tokio::sync::Mutex;

/// Simple sans-io timesync client and server.
///
/// Timesimp is based on the averaging method described in [Simpson (2002), A Stream-based Time
/// Synchronization Technique For Networked Computer Games][paper], but with a corrected delta
/// calculation. Compared to NTP, it's a simpler and less accurate time synchronisation algorithm
/// that is usable over network streams, rather than datagrams. Simpson asserts they were able to
/// achieve accuracies of 100ms or better, which is sufficient in many cases; my testing gets
/// accuracies well below 5ms. The main limitation of the algorithm is that round-trip-time is
/// assumed to be symmetric: if the forward trip time is different from the return trip time, then
/// an error is induced equal to the value of the difference in trip times.
///
/// This library provides a sans-io implementation: you bring in your transport and your storage;
/// timesimp gives you time offsets. Internally, timesimp is implemented in Rust.
///
/// If the local clock goes backward during a synchronisation, the invalid delta is discarded; this
/// may cause the sync attempt to fail, especially if the `samples` count is lowered to its minimum
/// of 3. This is a deliberate design decision: you should handle failure and retry, and the sync
/// will proceed correctly when the clock is stable.
///
/// [paper]: https://web.archive.org/web/20160310125700/http://mine-control.com/zack/timesync/timesync.html
#[napi]
#[derive(Debug, Clone)]
pub struct Timesimp(Arc<Mutex<TimesimpImpl>>);

type LoadFn = ThreadsafeFunction<(), Promise<Option<i64>>, (), true, true>;
type StoreFn = ThreadsafeFunction<(i64,), Promise<()>, (i64,), true, true>;
type QueryFn = ThreadsafeFunction<(Buffer,), Promise<Buffer>, (Buffer,),  true, true>;
pub struct TimesimpImpl {
    load: LoadFn,
    store: StoreFn,
    query: QueryFn,
}

impl fmt::Debug for TimesimpImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Timesimp")
            .field("load", &"ThreadsafeFunction")
            .field("store", &"ThreadsafeFunction")
            .field("query", &"ThreadsafeFunction")
            .finish()
    }
}

fn add_context(ctx: &'static str, line: u32) -> impl Fn(Error) -> Error {
    move |mut err| {
        err.reason = format!("{}\nin {}:{line} at {ctx}", err.reason, file!());
        err
    }
}

impl timesimp::Timesimp for TimesimpImpl {
    type Err = Error<Status>;

    async fn load_offset(&self) -> Result<Option<SignedDuration>> {
        Ok(self
            .load
            .call_async(Ok(()))
            .await
            .map_err(add_context("load_offset", line!()))?
            .await
            .map_err(add_context("load_offset", line!()))?
            .map(SignedDuration::from_micros))
    }

    async fn store_offset(&mut self, offset: SignedDuration) -> Result<()> {
        self.store
            .call_async(Ok((offset.as_micros() as i64,)))
            .await
            .map_err(add_context("store_offset", line!()))?
            .await
            .map_err(add_context("store_offset", line!()))
    }

    async fn query_server(&self, request: Request) -> Result<Response> {
        let buf = Buffer::from(request.to_bytes().to_vec());
        let res = self
            .query
            .call_async(Ok((buf,)))
            .await
            .map_err(add_context("query_server", line!()))?
            .await
            .map_err(add_context("query_server", line!()))?;
        Response::try_from(res.as_ref())
            .map_err(|err| Error::new(Status::GenericFailure, err))
            .map_err(add_context("query_server", line!()))
    }

    async fn sleep(duration: Duration) {
        tokio::time::sleep(duration).await
    }
}

#[napi]
impl Timesimp {
    /// Create a new timesimp instance.
    ///
    /// `load()` must be an async function that returns the current offset in microseconds, or
    /// `null` if no offset is currently stored.
    ///
    /// `store()` must be an async function that stores a given offset in microseconds. Once
    /// `store()` has been called once, `load()` should no longer return `null` if it did.
    ///
    /// `query()` must be an async function that sends the `request` buffer to a timesimp server,
    /// and returns the bytes that the server sends back. If a transport error occurs, the function
    /// should throw. For example, this can be an HTTP POST using `fetch()`.
    ///
    /// Due to internal API limitations, all three of these have a first `err` argument; this
    /// must be immediately thrown if truthy:
    ///
    /// ```js
    /// new Timesimp(
    ///     async (err) => { // load
    ///         if (err) throw err;
    ///         return db.query("offset");
    ///     },
    ///     async (err, offset) => { // store
    ///         if (err) throw err;
    ///         await db.update("offset", offset);
    ///     },
    ///     async (err, request) => {
    ///         if (err) throw err;
    ///         const res = await fetch("https://timesimp.server", {
    ///             method: "POST",
    ///             body: request,
    ///         });
    ///         return res.blob();
    ///     }
    /// );
    /// ```
    #[napi(
        constructor,
        ts_args_type = "load: (err: Error) => Promise<number | null>, store: (err: Error, offset: number) => Promise<void>, query: (err: Error, request: Buffer) => Promise<Buffer>"
    )]
    pub fn new(
        load: LoadFn,
        store: StoreFn,
        query: QueryFn,
    ) -> Self {
        Self(Arc::new(Mutex::new(TimesimpImpl { load, store, query })))
    }

    /// The current time in microseconds since the epoch, adjusted with the offset.
    ///
    /// This is a convenience function that internally calls your `load()`. You may want to
    /// implement your own function, especially if you want to get a `Date` or `Temporal`, or if
    /// you’ve implemented some caching.
    #[napi]
    pub async fn microtime(&self) -> Result<i64> {
        let ts = self
            .0
            .lock()
            .await
            .adjusted_timestamp()
            .await
            .map_err(add_context("microtime", line!()))?;
        Ok(ts.as_microsecond())
    }

    /// The implementation of the server endpoint.
    ///
    /// Use this in your server endpoint implementation. The endpoint should do as little else as
    /// possible to avoid adding unpredictable latency.
    ///
    /// You should obtain some bytes from the request’s payload (in this version, 8 bytes), and
    /// this method will return some other bytes (in this version, 16 bytes), which you should
    /// send back to the client.
    #[napi]
    pub async fn answer_client(&self, request: Buffer) -> Result<Buffer> {
        let req = Request::try_from(request.as_ref())
            .map_err(|err| Error::new(Status::InvalidArg, err))
            .map_err(add_context("answer_client", line!()))?;
        let res = self
            .0
            .lock()
            .await
            .answer_client(req)
            .await
            .map_err(add_context("answer_client", line!()))?;
        Ok(Buffer::from(res.to_bytes().to_vec()))
    }

    /// The main client state driver. Call this in a loop.
    ///
    /// You’re expected to sleep for a while after calling this, or to run it on a schedule. Take
    /// care to compute your schedule on your raw system clock or equivalent, so it does not get
    /// influenced by the offset, which could make it jump around or even spin. `setInterval` or
    /// `setTimeout` are appropriate.
    ///
    /// If `load()` returns `null`, this method will attempt to `store()` the first delta it gets
    /// from the server. This lets you get an “accurate enough” timestamp pretty quickly, instead
    /// of waiting for a full round of samples. Errors from that store are ignored silently.
    ///
    /// If this returns `null`, not enough samples were obtained to have enough confidence in the
    /// result, likely because the `query()` function encountered an error for most tries. Errors
    /// from `query()` are not returned; you may want to catch them for logging before passing them
    /// on.
    ///
    /// On success, returns the calculated offset in microseconds.
    #[napi]
    pub async fn attempt_sync(&self, settings: Settings) -> Result<Option<i64>> {
        let defaults = timesimp::Settings::default();
        let settings = timesimp::Settings {
            samples: settings.samples.unwrap_or(defaults.samples),
            jitter: settings
                .jitter
                .map(|j| Duration::from_micros(j as _))
                .unwrap_or(defaults.jitter),
        };
        let res = self
            .0
            .lock()
            .await
            .attempt_sync(settings)
            .await
            .map_err(add_context("attempt_sync", line!()))?;
        Ok(res.map(|offset| offset.as_micros() as _))
    }
}

/// Settings for a synchronisation attempt.
#[derive(Debug, Clone, Copy)]
#[napi(object)]
pub struct Settings {
    /// How many samples to gather for synchronisation.
    pub samples: Option<u8>,

    /// The maximum amount of time in microseconds between taking two samples.
    pub jitter: Option<u32>,
}
