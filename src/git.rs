use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::prelude::*;
use std::process::{exit, Command, Stdio};
use std::str;

/// A 20-byte SHA1 hash, used for identifying objects in Git.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SHA1 {
    bytes: [u8; 20],
}

#[derive(Copy, Clone, Debug)]
pub enum SHA1ParseError {
    IncorrectLength,
    InvalidHexadecimal,
}

impl Error for SHA1ParseError {
    fn description(&self) -> &str {
        match *self {
            SHA1ParseError::IncorrectLength => "String is not 40 characters long",
            SHA1ParseError::InvalidHexadecimal => "String is not valid hexadecimal",
        }
    }
}

impl Display for SHA1ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.description().fmt(f)
    }
}

impl SHA1 {
    /// Parses a SHA1 from a 40 character hex string
    pub fn parse(s: &str) -> Result<SHA1, SHA1ParseError> {
        if s.len() != 40 {
            return Err(SHA1ParseError::IncorrectLength);
        }

        let mut ret = SHA1::default();

        for i in 0..20 {
            let char_index = i * 2;
            ret.bytes[i] = match u8::from_str_radix(&s[char_index..char_index + 2], 16) {
                Ok(b) => b,
                _ => {
                    return Err(SHA1ParseError::InvalidHexadecimal);
                }
            };
        }

        Ok(ret)
    }
}

impl Display for SHA1 {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for b in &self.bytes {
            match write!(f, "{:02x}", b) {
                Ok(()) => {}
                err => {
                    return err;
                }
            };
        }
        Ok(())
    }
}

impl Default for SHA1 {
    fn default() -> SHA1 {
        SHA1 { bytes: [0; 20] }
    }
}

use common::Year;

pub fn assert_at_repo_top() {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .expect("Couldn't run `git rev-parse` to find top-level dir");

    if !output.status.success() {
        stderr!("Error: not in a Git directory");
        exit(1);
    }

    let tld = String::from_utf8(output.stdout).expect("git rev-parse returned invalid UTF-8");

    let trimmed_tld = tld.trim();

    let cwd = env::current_dir().expect("Couldn't get current directory");

    if trimmed_tld != cwd.to_str().expect("Current directory is not valid UTF-8") {
        stderr!(
            "{}\n{}",
            "Error: not at the top of a Git directory",
            "(This makes reasoning about paths much simpler.)"
        );
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
        .output()
        .expect("Couldn't spawn `git rev-parse` to parse ignored commit");

    if !output.status.success() {
        stderr!("Error: git rev-parse failed to parse {:?}", commit_ish);
        exit(1);
    }

    let sha_slice = str::from_utf8(&output.stdout)
        .expect("git rev-parse returned invalid UTF-8")
        .trim();

    SHA1::parse(sha_slice).expect("git rev-parse didn't return a valid SHA1")
}

fn year_from_iso_8601(iso: &str) -> Year {
    let dash_index = iso.find('-').expect("Didn't find dash in ISO-8601 output");
    iso[..dash_index]
        .parse()
        .expect("Couldn't parse first commit year")
}

pub fn get_first_commit_year() -> Year {
    let output = Command::new("git")
        .arg("log")
        .arg("--max-parents=0")
        .arg("--format=%aI")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .expect("Couldn't spawn `git log` to get first commit timestamp");

    if !output.status.success() {
        stderr!("Error: Couldn't run Git to find the first commit date");
        exit(1);
    }

    // ISO-8601: The year is everything before the first dash.
    let date_string = str::from_utf8(&output.stdout)
        .expect("git log returned invalid UTF-8")
        .trim()
        .split('\n')
        .last()
        .unwrap();

    // Find the dash
    year_from_iso_8601(date_string)
}

fn should_ignore_commit(sha: &str, commits: &HashSet<SHA1>) -> bool {
    let sha = SHA1::parse(sha).expect("Git provided an invalid hash");
    commits.contains(&sha)
}

pub fn get_file_years(path: &str, ignoring_commits: &HashSet<SHA1>) -> Vec<Year> {
    let output = Command::new("git")
        .arg("log")
        .arg("--follow")
        .arg("-M")
        .arg("-C")
        .arg("--format=%H %ai")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .expect("Couldn't spawn `git log` to get commit timestamps");

    if !output.status.success() {
        stderr!("Error: Couldn't run Git to find commit timestamps");
        exit(1);
    }

    let lines = str::from_utf8(&output.stdout)
        .expect("git log returned invalid UTF-8")
        .trim()
        .split('\n');

    let mut ret = Vec::<Year>::new();

    for line in lines {
        let mut space_split = line.split(' ');

        let sha = space_split.next().expect("Unexpected `git log` output");
        let date = space_split.next().expect("Unexpected `git log` output");

        if should_ignore_commit(sha, ignoring_commits) {
            continue;
        }

        ret.push(year_from_iso_8601(date));
    }

    // Do some cleanup.
    // (We'll do more later when these are combined with what the file comments
    // claimed, but no reason to hold onto a bunch of duplicates in the meantime.
    ret.sort();
    ret.dedup();

    ret
}
