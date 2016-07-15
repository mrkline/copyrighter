extern crate time;

use std::collections::HashMap;
use std::thread;
use std::sync::{Arc, mpsc};

use git_historian::history::{gather_history, Link, HistoryNode};
use git_historian::parsing::{get_history, ParsedCommit};
use git_historian::PathSet;

pub type Year = u16;

pub fn get_year_map(paths: Arc<PathSet>)
    -> thread::JoinHandle<HashMap<String, Vec<Year>>>
{
    thread::spawn(move || get_year_map_thread(paths))
}

pub fn get_year_map_thread(paths: Arc<PathSet>) -> HashMap<String, Vec<Year>> {
    let (tx, rx) = mpsc::sync_channel(0);

    thread::spawn(|| get_history(tx));

    let history = gather_history(&paths, &get_year, rx);

    let mut ret = HashMap::new();

    /*
    for (key, val) in history {
        // TODO
    }
    */
    ret
}

fn get_year(c: &ParsedCommit) -> Year {
    (time::at(c.when).tm_year + 1900) as Year
}
