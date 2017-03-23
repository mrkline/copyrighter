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

use std::borrow::Borrow;
use std::collections::HashSet;
use std::env;
use std::io::prelude::*;
use std::process::{Command, Stdio, exit};
use std::str;
use std::thread;

use getopts::Options;
use git_historian::{PathSet, SHA1};

use common::{Year, YearMap};

// Convenience macro to print to stderr
// See http://stackoverflow.com/a/32707058
macro_rules! stderr {
    ($($arg:tt)*) => (
        match writeln!(&mut ::std::io::stderr(), $($arg)* ) {
            Ok(_) => {},
            Err(x) => panic!("Unable to write to stderr (file handle closed?): {}", x),
        }
    )
}

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
        thread::spawn(move || history::get_year_map(&pc, &ignores));
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

fn assert_at_repo_top() {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output().expect("Couldn't run `git rev-parse` to find top-level dir");

    if !output.status.success() {
        stderr!("Error: not in a Git directory");
        exit(1);
    }

    let tld = String::from_utf8(output.stdout)
        .expect("git rev-parse returned invalid UTF-8");

    let trimmed_tld = tld.trim();

    let cwd = env::current_dir().expect("Couldn't get current directory");

    if trimmed_tld != cwd.to_str().expect("Current directory is not valid UTF-8") {
        stderr!("{}\n{}",
                "Error: not at the top of a Git directory",
                "(This makes reasoning about paths much simpler.)");
        exit(1);
    }
}

fn get_commits_to_ignore<S: Borrow<str>>(ignore_arg: Option<S>) -> HashSet<SHA1> {
    let ignore_arg = match ignore_arg {
        Some(a) => a,
        None => return HashSet::new()
    };

    ignore_arg.borrow().split(',').filter(|s| !s.is_empty())
        .map(|c| commit_ish_into_sha(c.trim())).collect()
}

fn commit_ish_into_sha(commit_ish: &str) -> SHA1 {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--verify")
        .arg(commit_ish)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output().expect("Couldn't spawn `git rev-parse` to parse ignored commit");

    if !output.status.success() {
        stderr!("Error: git rev-parse failed to parse {:?}", commit_ish);
        exit(1);
    }

    let sha_slice = str::from_utf8(&output.stdout)
        .expect("git rev-parse returned invalid UTF-8")
        .trim();

    SHA1::parse(sha_slice).expect("git rev-parse didn't return a valid SHA1")
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

fn get_first_commit_year() -> Year {
    let output = Command::new("git")
        .arg("log")
        .arg("--max-parents=0")
        .arg("--format=%aI")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output().expect("Couldn't spawn `git log` to get first commit timestamp");

    if !output.status.success() {
        stderr!("Error: Couldn't run Git to find the first commit date");
        exit(1);
    }

    // ISO-8601: The year is everything before the first dash.
    let date_string = str::from_utf8(&output.stdout)
        .expect("git log returned invalid UTF-8")
        .trim()
        .split('\n')
        .last().unwrap();

    // Find the dash
    let dash_index = date_string.find('-').expect("Didn't find dash in ISO-8601 output");
    date_string[.. dash_index].parse().expect("Couldn't parse first commit year")
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
