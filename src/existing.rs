//! Examines existing (if any) copyright headers and parses the listed years.
//! This is useful for years that may have taken place before adding the file
//! to Git.

use std::fs::File;
use std::io::{self, BufReader};
use std::io::prelude::*;

use lazy_static::lazy_static;
use rayon::prelude::*;
use regex::Regex;

use crate::common::*;

pub fn get_year_map(paths: PathSet) -> YearMap {
    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    paths
        .into_par_iter()
        .filter_map(|path| match scan_file(&path) {
            Ok(v) => Some((path, v)),
            Err(e) => {
                eprintln!("Error reading {}: {}", path, e);
                None
            }
        })
        .collect()
}

fn scan_file(path: &str) -> io::Result<Vec<Year>> {
    // Open the file and read in the first line.
    let mut first_line = String::new();
    {
        let fh = File::open(path)?;
        let mut br = BufReader::new(fh);
        br.read_line(&mut first_line)?;
    }

    lazy_static! {
        static ref COPYRIGHT: Regex = Regex::new(r"^\s*/[/*].*[Cc]opyright").unwrap();
        static ref YEAR_OR_RANGE: Regex =
            Regex::new(r"((\d{4})\s*[-–—]\s*(\d{4}))|(\d{4})").unwrap();
    }

    let mut years: Vec<Year> = Vec::new();

    // The first line isn't a copyright line. Move on to the next file.
    if !COPYRIGHT.is_match(&first_line) {
        return Ok(years);
    }

    for cap in YEAR_OR_RANGE.captures_iter(&first_line) {
        match cap.get(1) {
            // A single year:
            None => {
                years.push(cap.get(4).unwrap().as_str().parse().unwrap());
            }
            // A range of years (<yyyy>-<yyyy>):
            Some(_) => {
                let start: Year = cap.get(2).unwrap().as_str().parse().unwrap();
                let end: Year = cap.get(3).unwrap().as_str().parse().unwrap();

                for i in start..=end {
                    years.push(i);
                }
            }
        };
    }

    Ok(years)
}
