///! Examines existing (if any) copyright headers and parses the listed years.
///! This is useful for years that may have taken place before adding the file
///! to Git.

use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use git_historian::PathSet;
use num_cpus;
use threadpool::ThreadPool;

use common::{YearMap};

struct SharedState {
    // We'll keep track of how many paths we have left in order to know
    // when to stop waiting for results.
    paths_remaining: usize,

    // Option is used so we can extract the result at the end.
    // See http://stackoverflow.com/q/29177449/713961.
    result : Option<YearMap>,
}

struct SyncState {
    mutex : Mutex<SharedState>,
    // We'll notify the CV when we have no remaining paths to process.
    cv : Condvar
}

#[inline]
pub fn get_year_map(paths: Arc<PathSet>) -> thread::JoinHandle<YearMap> {
    thread::spawn(|| get_year_map_thread(paths))
}

fn get_year_map_thread(paths: Arc<PathSet>) -> YearMap {
    // Strap together an ARC for all our shared state
    let shared_state = Arc::new(
        SyncState{ mutex: Mutex::new(SharedState{paths_remaining: paths.len(),
                                                 result: Some(YearMap::new())}),
                   cv: Condvar::new() });

    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    let thread_pool = ThreadPool::new(num_cpus::get());

    for path in paths.iter() {
        // Cloning the path is a bit pessimal, but a copy is useful to have
        // if we end up inserting a result, and convincing the borrow checker
        // that the path outlives the closure is difficult since
        // ThreadPool::execute() wants `static lifetime.
        let p = path.clone();

        let ss = shared_state.clone(); // Bump the refcount
        thread_pool.execute(|| scan_file(p, ss));
    }

    // Sleep until we've processed all paths.
    let mut guard = shared_state.mutex.lock().unwrap();
    while guard.paths_remaining > 0 {
        guard = shared_state.cv.wait(guard).unwrap();
    }
    guard.result.take().unwrap()
}

fn scan_file(path: String, ss : Arc<SyncState>) {
    let fh = File::open(&path).unwrap();
    let mut br = BufReader::new(fh);
    let mut first_line = String::new();
    br.read_line(&mut first_line).unwrap();
    // TODO: Scan the first line


    // Take the lock on the shared state
    let mut guard = ss.mutex.lock().unwrap();
    guard.result.as_mut().unwrap().insert(path, vec![2001, 2048]);

    guard.paths_remaining -= 1;
    if guard.paths_remaining == 0 { ss.cv.notify_all(); }
}
