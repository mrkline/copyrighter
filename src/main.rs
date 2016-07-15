extern crate getopts;
extern crate git_historian;

// A demo app that gets the --oneline of every commit for a given file.
// Since this does so once per diff per commit, it is hilariously inefficient,
// but very easy to validate by comparing a given file's history to
// `git log --follow --oneline <file>`.

use std::env;
use std::process::exit;
use std::str;
use std::sync::Arc;

use getopts::Options;
use git_historian::PathSet;

mod history;

fn print_usage(opts: &Options, code: i32) {
    println!("{}", opts.usage("Usage: gsr [options] <file>"));
    exit(code);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut opts = Options::new();
    opts.optflag("h", "help", "Print this help menu");
    let mut matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { panic!(f) }
    };

    if matches.opt_present("h") {
        print_usage(&opts, 0);
    }


    // Assume free arguments are paths we want to examine
    let mut paths = PathSet::new();
    for path in matches.free.drain(..) {
        paths.insert(path);
    }

    // We're going to start passing this set around threads,
    // so let's start refcounting it.
    let paths = Arc::new(paths);

    // TODO: Useme
    history::get_year_map(paths);
}
