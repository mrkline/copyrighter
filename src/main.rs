extern crate getopts;
extern crate git_historian;
extern crate itertools;
extern crate libc;
extern crate num_cpus;
extern crate regex;
extern crate threadpool;

#[macro_use]
extern crate lazy_static;

mod common;
mod history;
mod existing;
mod update;

use std::env;
use std::io::prelude::*;
use std::process::{Command, Stdio, exit};
use std::str;

use getopts::Options;
use git_historian::PathSet;

use common::YearMap;

// Print our usage string and exit the program with the given code.
// (This never returns.)
fn print_usage(opts: &Options, code: i32) -> ! {
    println!("{}", opts.usage("Usage: copyrighter [options] <file>"));
    exit(code);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Args parsing via getopts
    let mut opts = Options::new();
    opts.optflag("h", "help", "Print this help menu");
    opts.optopt("o", "organization",
                "The organization claiming the copyright, and any following text",
                "<org>");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => {
            // If the user messes up the args, print the error and usage string.
            writeln!(&mut std::io::stderr(), "{}", e.to_string()).unwrap();
            print_usage(&opts, 1);
        }
    };

    if matches.opt_present("h") { // Print help as-desired.
        print_usage(&opts, 0);
    }

    let organization = match matches.opt_str("o") {
        Some(o) => o,
        None => { // -o is mandatory.
            writeln!(&mut std::io::stderr(),
                     "Required option 'organization' is missing.").unwrap();
            print_usage(&opts, 1);
        }
    };

    assert_at_repo_top();

    // Assume free arguments are paths we want to examine
    let mut paths = PathSet::with_capacity(matches.free.len());
    for path in matches.free {
        paths.insert(path);
    }

    // Kick off two threads: one gets when files were modified via Git history,
    // and the other searches the files themselves for existing copyright info.
    let git_years_handle = history::get_year_map(paths.clone());
    let header_years_handle = existing::get_year_map(paths);

    // Let them finish.
    let header_years : YearMap =  header_years_handle.join().unwrap();
    let git_years : YearMap =  git_years_handle.join().unwrap();

    let all_years = combine_year_maps(header_years, git_years);

    // Take all the info we've learned, and update (or create) copyright headers.
    update::update_headers(all_years, organization);
}

fn assert_at_repo_top() {
    let child = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .stdout(Stdio::piped())
        .spawn().expect("Couldn't spawn `git rev-parse` to find top-level dir");

    let output = child.wait_with_output().expect("git rev-parse did not exit cleanly");

    if !output.status.success() {
        writeln!(&mut std::io::stderr(), "Error: not in a Git directory").unwrap();
        exit(1);
    }

    let tld = String::from_utf8(output.stdout)
        .expect("git rev-parse returned invalid UTF-8");

    let trimmed_tld = tld.trim();

    let cwd = env::current_dir().expect("Couldn't get current directory");

    if trimmed_tld != cwd.to_str().expect("Current directory is not valid UTF-8") {
        writeln!(&mut std::io::stderr(), "{}\n{}",
                 "Error: not at the top of a Git directory",
                 "(This makes reasoning about paths much simpler.)").unwrap();
        exit(1);
    }
}

fn combine_year_maps(header_years: YearMap, git_years: YearMap) -> YearMap {
    // Merge the smaller map into the larger to try to avoid a realloc
    let (mut larger, smaller) = if git_years.len() > header_years.len() {
        (git_years, header_years)
    }
    else {
        (header_years, git_years)
    };

    // Transfer all of smaller's entries into larger.
    for (k, mut v) in smaller.into_iter() {
        let e = larger.entry(k).or_insert_with(Vec::new);
        e.append(&mut v);
    }

    // Sort and dedup our master map.
    for (_, v) in &mut larger {
        v.sort();
        v.dedup();
        // Once sorted and deduped, we won't be modifying this anymore,
        // so free up any memory we aren't using.
        v.shrink_to_fit();
    }

    // Ditto for the hashmap itself
    larger.shrink_to_fit();

    larger
}
