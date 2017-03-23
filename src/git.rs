use std::env;
use std::io::prelude::*;
use std::process::{Command, Stdio, exit};
use std::str;

use git_historian::SHA1;

use common::Year;

pub fn assert_at_repo_top() {
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

pub fn commit_ish_into_sha(commit_ish: &str) -> SHA1 {
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

pub fn get_first_commit_year() -> Year {
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
