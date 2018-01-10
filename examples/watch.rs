extern crate clap;
extern crate fibers;
extern crate fibers_inotify;
extern crate futures;
#[macro_use]
extern crate trackable;

use clap::{App, Arg};
use fibers::{Executor, InPlaceExecutor, Spawn};
use fibers_inotify::{Error, InotifyService, WatchMask};
use futures::{Future, Stream};

fn main() {
    let matches = App::new("watch")
        .arg(Arg::with_name("PATH").index(1).required(true))
        .get_matches();
    let path = matches.value_of("PATH").unwrap();
    let mask = WatchMask::CREATE | WatchMask::MODIFY | WatchMask::DELETE_SELF | WatchMask::DELETE
        | WatchMask::MOVE | WatchMask::MOVE_SELF;

    let inotify_service = InotifyService::new();
    let inotify_handle = inotify_service.handle();

    let mut executor = InPlaceExecutor::new().unwrap();
    executor.spawn(inotify_service.map_err(|e| panic!("{}", e)));

    let fiber = executor.spawn_monitor(inotify_handle.watch(path, mask).for_each(|event| {
        println!("{:?}", event);
        Ok(())
    }));
    track_try_unwrap!(executor.run_fiber(fiber).unwrap().map_err(Error::from));
}
