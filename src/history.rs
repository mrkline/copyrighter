extern crate time;

use std::collections::HashMap;
use std::thread;
use std::sync::{Arc, mpsc};

use git_historian::history::{gather_history, Link, HistoryNode};
use git_historian::parsing::{get_history, ParsedCommit};
use git_historian::PathSet;

pub type Year = u16;

pub type YearMap = HashMap<String, Vec<Year>>;

pub fn get_year_map(paths: Arc<PathSet>)
    -> thread::JoinHandle<YearMap>
{
    thread::spawn(|| get_year_map_thread(paths))
}

pub fn get_year_map_thread(paths: Arc<PathSet>) -> YearMap {
    let (tx, rx) = mpsc::sync_channel(0);

    thread::spawn(|| get_history(tx));

    let history = gather_history(&paths, &get_year, rx);

    let mut ret = YearMap::new();

    for (key, val) in history {
        let mut years : Vec<Year> = Vec::new();
        walk_history(&val, &mut years);

        years.sort();
        years.dedup();
        ret.insert(key, years);
    }
    ret
}

fn walk_history(node: &Link<HistoryNode<Year>>, append_to: &mut Vec<Year>) {
    let nb = node.borrow();
    append_to.push(nb.data);

    if let Some(ref prev) = nb.previous {
        walk_history(prev, append_to)
    }
}

fn get_year(c: &ParsedCommit) -> Year {
    (time::at(c.when).tm_year + 1900) as Year
}
