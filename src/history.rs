//! Use git-historian to find the years in which our files were changed
//! according to Git history. See that library for details of how it works.

extern crate time;

use std::thread;
use std::sync::mpsc;

use git_historian::history::{gather_history, Link, HistoryNode};
use git_historian::parsing::{get_history, ParsedCommit};
use git_historian::PathSet;

use common::{Year, YearMap};

#[inline]
pub fn get_year_map(paths: PathSet) -> thread::JoinHandle<YearMap>
{
    thread::spawn(|| get_year_map_thread(paths))
}

fn get_year_map_thread(paths: PathSet) -> YearMap {
    // One thread reads output from git-log; this one consumes and parses it.
    let (tx, rx) = mpsc::sync_channel(0);

    let handle = thread::spawn(|| get_history(tx));

    let history = gather_history(&paths, &get_year, rx);

    let mut ret = YearMap::new();

    for (key, val) in history {
        let mut years : Vec<Year> = Vec::new();
        walk_history(&val, &mut years);
        // We're not going to sort or dedup here since we will later.
        // (See combine_year_maps())
        ret.insert(key, years);
    }
    handle.join().unwrap();
    ret
}

// The history we're given is a "tree" of nodes, containing per-commit info
// for the files we care about. Walk the nodes of the tree and store the years.
fn walk_history(node: &Link<HistoryNode<Year>>, append_to: &mut Vec<Year>) {
    let nb = node.borrow();
    append_to.push(nb.data);

    if let Some(ref prev) = nb.previous {
        walk_history(prev, append_to)
    }
}

// For the copyright, we only care to extract the year from each commit.
fn get_year(c: &ParsedCommit) -> Year {
    (time::at(c.when).tm_year + 1900) as Year
}
