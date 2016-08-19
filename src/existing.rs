//! Examines existing (if any) copyright headers and parses the listed years.
//! This is useful for years that may have taken place before adding the file
//! to Git.

use std::fs::File;
use std::io::{self, BufReader};
use std::io::prelude::*;
use std::sync::mpsc;

use git_historian::PathSet;
use num_cpus;
use threadpool::ThreadPool;
use regex::Regex;

use common::{Year, YearMap};

pub fn get_year_map(paths: PathSet) -> YearMap {
    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    let thread_pool = ThreadPool::new(num_cpus::get());

    let (tx, rx) = mpsc::channel();

    for path in paths.into_iter() { // Consume our paths
        let tx_clone = tx.clone();
        thread_pool.execute(move || {
            let result = scan_file(&path);
            tx_clone.send((path, result)).unwrap()
        });
    }
    // Dropping the original tx here means rx.recv() will fail
    // once all senders have finished.
    drop(tx);

    let mut ret = YearMap::new();

    // Slurp our our paths until there aren't any more
    while let Ok((path, result)) = rx.recv() {
        // scan_file succeeded or we should print the I/O error and move on.
        match result {
            Ok(v) => assert!(ret.insert(path, v).is_none()),
            Err(e) => writeln!(&mut io::stderr(), "Error reading {}: {}", path, e).unwrap()
        };
    }

    ret
}

fn scan_file(path: &str) -> io::Result<Vec<Year>> {
    // Open the file and read in the first line.
    let mut first_line = String::new();
    {
        let fh = try!(File::open(path));
        let mut br = BufReader::new(fh);
        try!(br.read_line(&mut first_line));
    }

    lazy_static!{
        static ref COPYRIGHT : Regex = Regex::new(
            r"^\s*/[/*].*[Cc]opyright").unwrap();
        static ref YEAR_OR_RANGE : Regex = Regex::new(
            r"((\d{4})\s*[-–—]\s*(\d{4}))|(\d{4})").unwrap();
    }

    let mut years : Vec<Year> = Vec::new();

    // The first line isn't a copyright line. Move on to the next file.
    if !COPYRIGHT.is_match(&first_line) {
        return Ok(years);
    }

    for cap in YEAR_OR_RANGE.captures_iter(&first_line) {
        match cap.at(1) {
            // A single year:
            None => { years.push(cap.at(4).unwrap().parse().unwrap()); },
            // A range of years (<yyyy>-<yyyy>):
            Some(_) => {
                let start : Year = cap.at(2).unwrap().parse().unwrap();
                let end : Year = cap.at(3).unwrap().parse().unwrap();

                for i in start .. end+1 {
                    years.push(i);
                }
            },
        };
    }

    Ok(years)
}
