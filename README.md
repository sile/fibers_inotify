fibers_inotify
==============

[![fibers_inotify](https://img.shields.io/crates/v/fibers_inotify.svg)](https://crates.io/crates/fibers_inotify)
[![Documentation](https://docs.rs/fibers_inotify/badge.svg)](https://docs.rs/fibers_inotify)
[![Build Status](https://travis-ci.org/sile/fibers_inotify.svg?branch=master)](https://travis-ci.org/sile/fibers_inotify)
[![Code Coverage](https://codecov.io/gh/sile/fibers_inotify/branch/master/graph/badge.svg)](https://codecov.io/gh/sile/fibers_inotify/branch/master)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A [futures] friendly [inotify] wrapper for [fibers] crate.

[Documentation](https://docs.rs/fibers_inotify).

[futures]: https://crates.io/crates/futures
[fibers]: https://crates.io/crates/fibers
[inotify]: https://en.wikipedia.org/wiki/Inotify

Examples
---------

Watches `/tmp` directory:

```rust
use fibers::{Executor, InPlaceExecutor, Spawn};
use fibers_inotify::{InotifyService, WatchMask};
use futures::{Future, Stream};

let inotify_service = InotifyService::new();
let inotify_handle = inotify_service.handle();

let mut executor = InPlaceExecutor::new().unwrap();
executor.spawn(inotify_service.map_err(|e| panic!("{}", e)));

let fiber = executor.spawn_monitor(
   inotify_handle
       .watch("/tmp/", WatchMask::CREATE | WatchMask::DELETE)
       .for_each(|event| Ok(println!("# EVENT: {:?}", event)))
       .map_err(|e| panic!("{}", e)),
   );

let _ = executor.run_fiber(fiber).unwrap();
```
