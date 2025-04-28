# Time simp ⏰

_Simple sans-io timesync client and server._

## How does it work?

Timesimp is based on the averaging method described in [Simpson (2002),
A Stream-based Time Synchronization Technique For Networked Computer
Games][paper], but with a corrected delta calculation. Compared to NTP, it's a
simpler and less accurate time synchronisation algorithm that is usable over
network streams, rather than datagrams. Simpson asserts they were able to
achieve accuracies of 100ms or better, which is sufficient in many cases; my
testing gets accuracies well below 1ms. The main limitation of the algorithm is
that round-trip-time is assumed to be symmetric: if the forward trip time is
different from the return trip time, then an error is induced equal to the
value of the difference in trip times.

This library provides a sans-io implementation: you bring in your transport and
storage, you get time offsets. There's the core Rust crate and Node.js bindings.

[paper]: https://web.archive.org/web/20160310125700/http://mine-control.com/zack/timesync/timesync.html

## How do I use it?

### Rust

- [Crate][lib-rust]
- [API Docs][docs-rust]

[lib-rust]: https://lib.rs/crate/timesimp
[docs-rust]: https://docs.rs/timesimp

### Node.js

- [NPM][lib-node]
- [API Docs][docs-node]

[lib-node]: https://www.npmjs.com/package/timesimp
[docs-node]: https://passcod.github.io/timesimp/js/
