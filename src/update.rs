//! Updates copyright headers based on the information gathered.

use std::fs::{File, OpenOptions};
use std::io;
use std::io::prelude::*;
use std::ptr;

use itertools::Itertools;
use lazy_static::lazy_static;
use memmap::MmapMut;
use rayon::prelude::*;
use regex::Regex;

use crate::common::{Year, YearMap};

pub fn update_headers(map: &YearMap, organization: &str) {
    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    map.par_iter().for_each(|(k, v)| {
        let result = update_file(&k, v, &organization);
        match result {
            Ok(()) => { /* Everything worked, nothing to do */ }
            Err(e) => eprintln!("Error updating {}: {}", k, e),
        }
    });
}

/// Update the existing copyright notice of a file, or tack on a new one.
fn update_file(path: &str, years: &[Year], organization: &str) -> io::Result<()> {
    // Open the file with read and write perms.
    let mut fh = OpenOptions::new().read(true).write(true).open(path)?;

    // Read in the existing first line (so we can look for an existing notice).
    let mut first_line_buff = String::new();
    {
        let mut br = io::BufReader::new(&fh);
        br.read_line(&mut first_line_buff)?;
    }

    // We don't want to mess with the newline (or trailing space).
    let old_first_line = first_line_buff.trim_right();

    lazy_static! {
        static ref COPYRIGHT_OPENER: Regex = Regex::new(r"^(\s*/[/*]).*[Cc]opyright").unwrap();
    }

    let mut new_first_line: String;
    let replacing_existing_notice: bool;

    match COPYRIGHT_OPENER.captures(old_first_line) {
        // If there's an existing copyright notice, update that.
        Some(capture) => {
            // Preserve the existing // or /* and following whitespace.
            new_first_line = capture.get(1).unwrap().as_str().to_owned();
            replacing_existing_notice = true;
        }
        // Otherwise we'll add one.
        None => {
            new_first_line = "//".to_string();
            replacing_existing_notice = false;
        }
    };

    new_first_line.push_str(" Copyright © ");
    // Insert a comma-separated list of years modified.
    // TODO: Also allow dashed ranges.
    new_first_line.push_str(&years.into_iter().map(|y| y.to_string()).join(","));
    new_first_line.push(' ');
    new_first_line.push_str(organization);

    if !replacing_existing_notice {
        // We need a newline if we're creating our own notice.
        new_first_line.push('\n');
        // Slide the existing contents forward, making way for the new notice.
        slide_file_contents(&fh, 0, new_first_line.len() as isize)?;
    } else {
        // Calculate the difference in length between the old notice and the new
        // one, then slide all contents *after* the old notice that distance.
        let slide_amount = new_first_line.len() as isize - old_first_line.len() as isize;
        slide_file_contents(&fh, old_first_line.len(), slide_amount)?;
    }

    // Rewind to the start and write our notice line.
    fh.seek(io::SeekFrom::Start(0))?;

    fh.write_all(new_first_line.as_bytes())
}

/// We slide file contents around using mmap and memmove, assuming
/// 1. This is simpler and faster than creating a temp file,
///    writing our copyright header, writing the remaining file contents,
///    then overwriting the existing file with the temp file.
/// 2. The file fits comfortably in memory space. Besides, if a *code* file
///    is more than a few dozen kilobytes, you have other problems.
fn slide_file_contents(fd: &File, offset: usize, amount: isize) -> io::Result<()> {
    // We simplify casting and math below if we can assume offset can be signed.
    assert!(offset <= isize::max_value() as usize);

    // Don't let us slide contents past the start of the file.
    assert!(offset as isize + amount >= 0);

    // If we have nothing to do, go home early.
    if amount == 0 {
        return Ok(());
    }

    // Generally, casting a file length to a isize would be a bad idea.
    // (usize is 32 bits on x86, and files can be much larger than 4GB.)
    // But we're trying to mmap it (so it should fit in our address space),
    // and if a code file is that big...
    let file_length_64 : u64 = fd.metadata()?.len();
    if file_length_64 > isize::max_value() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The file is too large to be mapped.",
        ));
    }
    let file_length = file_length_64 as usize;
    assert!(file_length > offset); // Return error instead of asserting?

    let new_length = (file_length as isize + amount) as u64;

    if amount < 0 {
        // We have to shrink the file.
        // Shift its contents over.
        unsafe {
            let mut mapping = MmapMut::map_mut(&fd)?;
            // memmove, a la Rust
            ptr::copy(
                mapping.as_ptr().add(offset),
                mapping.as_mut_ptr().add(offset).offset(amount),
                file_length - offset,
            );
        }

        // Then shrink it.
        fd.set_len(new_length)?;

    } else if amount > 0 {
        // We have to grow the file.
        fd.set_len(new_length)?;

        // Shift the contents over.
        unsafe {
            let mut mapping = MmapMut::map_mut(fd)?;
            // memmove, a la Rust
            ptr::copy(
                mapping.as_ptr().add(offset),
                mapping.as_mut_ptr().add(offset).offset(amount),
                file_length - offset,
            );
        }
    } else {
        // wat
        unreachable!("We should account for this case with an early return.");
    }
    Ok(())
}
