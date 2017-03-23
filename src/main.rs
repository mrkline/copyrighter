//! Copyrighter uses Git history and existing copyright notices to generate updated
//! ones for files.
//!
//! # Usage:
//!
//! ```text
//! copyrighter -o <organization> -i <commits>
//! ```
//!
//! where
//!
//! ```
//! --organization, -o
//!   The organization claiming the copyright, and any following text
//!
//! --ignore-commits, -i <commit1[,commit2,...]>
//!   Ignore the listed commits when examining history.
//!   Commits are looked up using git rev-parse
//! ```
//!
//!
//! # Example
//!
//! To update all .cpp and .h files in a project,
//!
//! ```sh
//! $ cd my_project
//! $ find -type f \( -name '*.cpp' -or -name '*.h'\) \
//!     -exec copyrighter --organization "Fluke Corporation. All rights reserved." {} +
//! ```

extern crate getopts;
extern crate itertools;
extern crate libc;
extern crate num_cpus;
extern crate regex;
extern crate threadpool;

#[macro_use]
extern crate lazy_static;

#[macro_use]
mod stderr;

mod common;
mod existing;
mod git;
mod history;
mod update;

use std::borrow::Borrow;
use std::collections::HashSet;
use std::env;
use std::io::prelude::*;
use std::process::exit;
use std::thread;

use getopts::Options;

use common::*;
use git::*;

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
    opts.optflag("h", "help", "Print this help text.");
    opts.optopt("o", "organization",
                "The organization claiming the copyright, and any following text",
                "<org>");
    opts.optopt("i", "ignore-commits",
                "Ignore the listed commits when examining history",
                "<commit1[,commit2,...]>");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => {
            // If the user messes up the args, print the error and usage string.
            stderr!("{}", e.to_string());
            print_usage(&opts, 1);
        }
    };

    if matches.opt_present("h") { // Print help as-desired.
        print_usage(&opts, 0);
    }

    let organization = match matches.opt_str("o") {
        Some(o) => o,
        None => { // -o is mandatory.
            stderr!("Required option 'organization' is missing.");
            print_usage(&opts, 1);
        }
    };

    assert_at_repo_top();

    // Get the SHAs of commits we want to ignore
    let ignores = get_commits_to_ignore(matches.opt_str("i"));

    // Grab the first year of the commit so we can use it later.
    // (If we do it now, we can skip all the work below if it fails).
    let first_git_year = get_first_commit_year();

    // Assume free arguments are paths we want to examine
    let mut paths = PathSet::with_capacity(matches.free.len());
    for path in matches.free {
        paths.insert(path);
    }

    // Kick off two threads: one gets when files were modified via Git history,
    // and the other searches the files themselves for existing copyright info.
    let pc = paths.clone();
    let git_years_handle =
        thread::spawn(move || history::get_year_map(&pc, ignores));
    let header_years_handle =
        thread::spawn(|| existing::get_year_map(paths));

    // Let them finish.
    let mut header_years : YearMap = header_years_handle.join().unwrap();
    let git_years : YearMap = git_years_handle.join().unwrap();

    // Strip header-provided years that overlap with Git history.
    trim_header_years(&mut header_years, first_git_year);

    let all_years = combine_year_maps(header_years, git_years);

    // Take all the info we've learned, and update (or create) copyright headers.
    update::update_headers(all_years, organization);
}

fn get_commits_to_ignore<S: Borrow<str>>(ignore_arg: Option<S>) -> HashSet<SHA1> {
    let ignore_arg = match ignore_arg {
        Some(a) => a,
        None => return HashSet::new()
    };

    ignore_arg.borrow().split(',').filter(|s| !s.is_empty())
        .map(|c| commit_ish_into_sha(c.trim())).collect()
}

fn trim_header_years(header_years: &mut YearMap, first_year: Year) {
    // We trust Git history more than we do copyright comments,
    // so discard all years after the year of the first Git commit
    // from the ones we parsed out of the files.
    //
    // Unless the first commit was made at 00:00:00 on January 1,
    // there's a chance changes were made that year before Git,
    // so we keep the first year around.
    for val in header_years.values_mut() {
        val.retain(|&y| y <= first_year);
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
    for (k, mut v) in smaller {
        let e = larger.entry(k).or_insert_with(Vec::new);
        e.append(&mut v);
    }

    // Sort and dedup our master map.
    for v in larger.values_mut() {
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
