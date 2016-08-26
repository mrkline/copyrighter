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
    opts.optopt("i", "ignore-commits",
                "Ignore the listed commits when examining history",
                "<commit1[,commit2,...]>");

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

    // Get the SHAs of commits we want to ignore
    let ignores = get_commits_to_ignore(matches.opt_str("i"));

    // Assume free arguments are paths we want to examine
    let mut paths = PathSet::with_capacity(matches.free.len());
    for path in matches.free {
        paths.insert(path);
    }

    // Kick off two threads: one gets when files were modified via Git history,
    // and the other searches the files themselves for existing copyright info.
    let pc = paths.clone();
    let git_years_handle =
        thread::spawn(|| history::get_year_map(pc, ignores));
    let header_years_handle =
        thread::spawn(|| existing::get_year_map(paths));

    // Let them finish.
    let header_years : YearMap =  header_years_handle.join().unwrap();
    let git_years : YearMap =  git_years_handle.join().unwrap();

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
        writeln!(&mut std::io::stderr(),
                 "Error: git rev-parsed failed to parse {:?}",
                 commit_ish).unwrap();
        exit(1);
    }

    let sha_slice = str::from_utf8(&output.stdout)
        .expect("git rev-parse returned invalid UTF-8")
        .trim();

    SHA1::parse(sha_slice).expect("git rev-parsed didn't return a valid SHA1")
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
