//! Updates copyright headers based on the information gathered.

use std::fs::{File, OpenOptions};
use std::io;
use std::io::prelude::*;
use std::sync::{Arc, mpsc};
use std::os::unix::prelude::*;
use std::ptr;

use itertools::Itertools;
use libc;
use num_cpus;
use regex::Regex;
use threadpool::ThreadPool;

use common::{Year, YearMap};

pub fn update_headers(map: YearMap, organization: String) {
    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    let thread_pool = ThreadPool::new(num_cpus::get());

    let (tx, rx) = mpsc::channel();

    let organization = Arc::new(organization);

    for (k, v) in map {
        let tx_clone = tx.clone();
        let org_clone = organization.clone();
        thread_pool.execute(move || {
            let result = update_file(&k, v, &org_clone);
            tx_clone.send((k, result)).unwrap();
        });
    }
    // Dropping the original tx here means rx.recv() will fail
    // once all senders have finished.
    drop(tx);


    // Slurp our our paths until there aren't any more
    for (path, result) in rx {
        match result {
            Ok(()) => { /* Everything worked, nothing to do */ },
            Err(e) => writeln!(&mut io::stderr(), "Error updating {}: {}", path, e).unwrap()
        }
    }
}

/// Update the existing copyright notice of a file, or tack on a new one.
fn update_file(path: &str, years : Vec<Year>, organization: &str) -> io::Result<()> {
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

    lazy_static!{
        static ref COPYRIGHT_OPENER : Regex = Regex::new(
            r"^(\s*/[/*]).*[Cc]opyright").unwrap();
    }

    let mut new_first_line : String;
    let replacing_existing_notice : bool;

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
    }
    else {
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
/// 1. We're on a Unix.
/// 2. This is simpler and faster than creating a temp file,
///    writing our copyright header, writing the remaining file contents,
///    then overwriting the existing file with the temp file.
/// 3. The file fits comfortably in memory space. Besides, if a *code* file
///    is more than a few dozen kilobytes, you have other problems.
fn slide_file_contents(rust_handle : &File, offset: usize, amount : isize) -> io::Result<()> {
    // We simplify casting and math below if we can assume offset can be signed.
    assert!(offset <= isize::max_value() as usize);

    // Don't let us slide contents past the start of the file.
    assert!(offset as isize + amount >= 0);

    // If we have nothing to do, go home early.
    if amount == 0 {
        return Ok(());
    }

    // Generally, casting a file length to a isize would be a terrible idea.
    // (usize is 32 bits on x86, and files can be much larger than 4GB.)
    // But we're trying to mmap it (so it should fit in our address space),
    // and if a code file is that big...
    let file_length_64 : u64 = rust_handle.metadata()?.len();
    if file_length_64 > isize::max_value() as u64 {
        return Err(io::Error::new(io::ErrorKind::InvalidData,
                                  "The file is too large to be mapped."));
    }
    let file_length = file_length_64 as usize;

    let fd = rust_handle.as_raw_fd(); // Get our classic Unix int file handle.
    // How long will the file be once we're done with it?
    let new_length = (file_length as isize + amount) as libc::off_t;

    if amount < 0 { // We have to shrink the file.
        // Shift its contents over.
        let mut mapping = Mapping::open(fd, file_length)?;
        unsafe { // memmove, a la Rust
            ptr::copy(mapping.ptr().offset(offset as isize),
                      mapping.mut_ptr().offset(offset as isize + amount),
                      file_length - offset);
        }
        drop(mapping);

        // Then shrink it.
        unsafe {
            assert_eq!(libc::ftruncate(fd, new_length), 0);
        }
    }
    else if amount > 0 { // We have to grow the file.
        // Use fallocate instead of ftruncate to ensure that we have the room
        // on disk. See the man pages for posix_fallocate and ftruncate.
        unsafe {
            assert_eq!(libc::posix_fallocate(fd, 0, new_length), 0);
        }

        // Shift the contents over.
        let mut mapping = Mapping::open(fd, new_length as usize)?;
        unsafe { // memmove, a la Rust
            ptr::copy(mapping.ptr().offset(offset as isize),
                      mapping.mut_ptr().offset(offset as isize + amount),
                      file_length - offset);
        }
    }
    else { // wat
        unreachable!("We should account for this case with an early return.");
    }
    Ok(())
}

// A whole crate exists for cross-platform memory mapping
// (https://github.com/danburkert/memmap-rs),
// but for now I only care about the Posix case with no offset,
// which is easy to do. No need to pull in another dependency.

/// A RAII type for mmap
struct Mapping {
    // mmap gives a pointer.
    ptr: *mut libc::c_void,
    // munmap wants the size we asked for with mmap.
    file_length: libc::size_t,
}

impl Mapping {
    fn open(fd: RawFd, file_length: usize) -> io::Result<Mapping> {
        let mapping = unsafe {
            libc::mmap(ptr::null_mut(),
                       file_length,
                       libc::PROT_READ | libc::PROT_WRITE,
                       libc::MAP_SHARED,
                       fd, 0)
        };
        if mapping == libc::MAP_FAILED {
            Err(io::Error::last_os_error())
        }
        else {
            Ok(Mapping{ ptr: mapping, file_length: file_length })
        }
    }

    fn ptr(&self) -> *const u8 {
        self.ptr as *const u8
    }

    fn mut_ptr(&mut self) -> *mut u8 {
        self.ptr as *mut u8
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            assert_eq!(libc::munmap(self.ptr, self.file_length), 0,
                    "munmap failed with {}", io::Error::last_os_error());
        }
    }
}
