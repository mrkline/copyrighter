//! Use git-historian to find the years in which our files were changed
//! according to Git history. See that library for details of how it works.

extern crate time;

use std::collections::HashSet;

use rayon::prelude::*;

use crate::git::*;
use crate::common::*;

pub fn get_year_map(paths: PathSet, ignore_commits: &HashSet<SHA1>) -> YearMap {
    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    let ret: YearMap = paths
        .into_par_iter()
        .map(|path| {
            let file_history = get_file_years(&path, &ignore_commits);
            (path, file_history)
        })
        .collect();

    ret
}
