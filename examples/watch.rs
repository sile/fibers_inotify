extern crate fibers;
extern crate fibers_inotify;
extern crate futures;

use fibers::{Executor, InPlaceExecutor, Spawn};
use fibers_inotify::{InotifyService, WatchMask};
use futures::{Future, Stream};

fn main() {
    let service = InotifyService::new();
    let inotify = service.handle();

    let executor = InPlaceExecutor::new().unwrap();
    executor.spawn(service.map_err(|e| panic!("{}", e)));

    executor.spawn(
        inotify
            .watcher("/tmp/", WatchMask::CREATE | WatchMask::MODIFY)
            .for_each(|event| {
                println!("{:?}", event);
                Ok(())
            })
            .then(|_| Ok(())),
    );

    executor.run().unwrap();
}
