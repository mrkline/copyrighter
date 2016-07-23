extern crate getopts;
extern crate git_historian;
extern crate num_cpus;
extern crate threadpool;
extern crate regex;

#[macro_use]
extern crate lazy_static;

mod common;
mod history;
mod existing;

use std::env;
use std::process::exit;
use std::str;
use std::sync::Arc;

use getopts::Options;
use git_historian::PathSet;

use common::YearMap;

fn print_usage(opts: &Options, code: i32) {
    println!("{}", opts.usage("Usage: gsr [options] <file>"));
    exit(code);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut opts = Options::new();
    opts.optflag("h", "help", "Print this help menu");
    let matches = opts.parse(&args[1..]).unwrap();

    if matches.opt_present("h") {
        print_usage(&opts, 0);
    }


    // Assume free arguments are paths we want to examine
    let mut paths = PathSet::new();
    for path in matches.free {
        paths.insert(path);
    }

    // We're going to start passing this set around threads,
    // so let's start refcounting it.
    let paths = Arc::new(paths);

    let git_years_handle = history::get_year_map(paths.clone());
    let header_years_handle = existing::get_year_map(paths);

    let header_years : YearMap =  header_years_handle.join().unwrap();
    let git_years : YearMap =  git_years_handle.join().unwrap();

    let all_years = combine_year_maps(header_years, git_years);

    println!("{:?}", all_years);
}

fn combine_year_maps(header_years: YearMap, git_years: YearMap) -> YearMap {
    // Merge the smaller map into the larger to try to avoid one realloc-ing.
    let mut larger;
    let mut smaller;
    if git_years.len() > header_years.len() {
        larger = git_years;
        smaller = header_years;
    }
    else {
        larger = header_years;
        smaller = git_years;
    }

    // Transfer all of smaller's entries into larger.
    for (k, mut v) in smaller.drain() {
        let e = larger.entry(k).or_insert(Vec::new());
        e.append(&mut v);
        e.sort();
        e.dedup();
        // Once sorted and deduped, we won't be modifying this anymore,
        // so free up any memory we aren't using.
        e.shrink_to_fit();
    }
    // Ditto for the hashmap itself
    larger.shrink_to_fit();

    larger
}
