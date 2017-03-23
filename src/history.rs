//! Use git-historian to find the years in which our files were changed
//! according to Git history. See that library for details of how it works.

extern crate time;

use std::collections::HashSet;
use std::sync::{Arc, mpsc};

use num_cpus;
use threadpool::ThreadPool;

use git::*;
use common::*;

pub fn get_year_map(paths: &PathSet, ignore_commits: HashSet<SHA1>) -> YearMap
{
    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    let thread_pool = ThreadPool::new(num_cpus::get());

    // One thread reads output from git-log; this one consumes and parses it.
    let (tx, rx) = mpsc::sync_channel(0);

    let ignores = Arc::new(ignore_commits);

    for path in paths {
        let path_clone = path.clone();
        let tx_clone = tx.clone();
        let ignores_clone = ignores.clone();
        thread_pool.execute(move || {
            let file_history = get_file_years(&path_clone, &ignores_clone);
            tx_clone.send((path_clone, file_history)).unwrap();
        });
    }
    // Dropping the original tx here means rx.recv() will fail
    // once all senders have finished.
    drop(tx);

    let mut ret = YearMap::new();

    // Slurp our our history for all paths
    for (path, file_history) in rx {
        ret.insert(path, file_history);
    }

    ret
}
