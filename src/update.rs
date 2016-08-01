//! Updates copyright headers based on the information gathered.

use std::fs::{File, OpenOptions};
use std::io;
use std::io::prelude::*;
use std::sync::{Arc, Condvar, Mutex};
use std::os::unix::prelude::*;
use std::ptr;

use itertools::Itertools;
use libc;
use num_cpus;
use regex::Regex;
use threadpool::ThreadPool;

use common::{Year, YearMap};

struct SyncState {
    // Hold onto the string given to us via command line args
    organization: String,

    // We'll keep track of how many paths we have left in order to know
    // when to stop waiting for results.
    paths_remaining : Mutex<usize>,
    // We'll notify the CV when we have no remaining paths to process.
    cv : Condvar,
}

pub fn update_headers(map: YearMap, organization: String) {
    // Strap together an ARC for all our shared state
    let shared_state = Arc::new(
        SyncState{ organization: organization,
                   paths_remaining: Mutex::new(map.len()),
                   cv: Condvar::new() });

    // Let's paralellize! I'm assuming this process will be largely bottlenecked
    // by the I/O of actually reading the files, but we can let the OS'es I/O
    // scheduler figure that out.
    let thread_pool = ThreadPool::new(num_cpus::get());

    for (k, v) in map {
        let ss = shared_state.clone();
        thread_pool.execute(|| update_file(k, v, ss));
    }

    // Sleep until we've processed all paths.
    let mut remaining = shared_state.paths_remaining.lock().unwrap();
    while *remaining > 0 {
        remaining = shared_state.cv.wait(remaining).unwrap();
    }
}

/// Update the existing copyright notice of a file, or tack on a new one.
fn update_file(path: String, years : Vec<Year>, ss : Arc<SyncState>) {
    // Open the file with read and write perms.
    let mut fh = OpenOptions::new().read(true).write(true).open(&path)
        .expect(&("Error opening ".to_string() + &path));

    // Read in the existing first line (so we can look for an existing notice).
    let mut first_line_buff = String::new();
    {
        let mut br = io::BufReader::new(&fh);
        br.read_line(&mut first_line_buff)
          .expect(&("Error reading ".to_string() + &path));
    }

    // We don't want to mess with the newline (or trailing space).
    let old_first_line = first_line_buff.trim_right();

    lazy_static!{
        static ref COPYRIGHT_OPENER : Regex = Regex::new(
            r"^(\s*/[/*]).*[Cc]opyright").unwrap();
    }

    let mut new_first_line : String;
    let replacing_existing_notice : bool;

    match COPYRIGHT_OPENER.captures(&old_first_line) {
        // If there's an existing copyright notice, update that.
        Some(capture) => {
            // Preserve the existing // or /* and following whitespace.
            new_first_line = capture.at(1).unwrap().to_string();
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
    new_first_line.push_str(&ss.organization);
    new_first_line.push('\n');

    if !replacing_existing_notice {
        // Slide the existing contents forward, making way for the new notice.
        slide_file_contents(&fh, 0, new_first_line.len() as isize);
    }
    else {
        // Calculate the difference in length between the old notice and the new
        // one, then slide all contents *after* the old notice that distance.
        let slide_amount = new_first_line.len() as isize - old_first_line.len() as isize;
        slide_file_contents(&fh, old_first_line.len(), slide_amount);
    }

    // Rewind to the start and write our notice line.
    fh.seek(io::SeekFrom::Start(0))
      .expect(&("Error seeking in ".to_string() + &path));

    fh.write_all(new_first_line.as_bytes())
      .expect(&("Error writing to ".to_string() + &path));

    // Decrement the number of files to go.
    let mut remaining = ss.paths_remaining.lock().unwrap();
    *remaining -= 1;
    if *remaining == 0 { ss.cv.notify_all(); }
}

/// We slide file contents around using mmap and memmove, assuming
/// 1. We're on a Unix.
/// 2. This is simpler and faster than creating a temp file,
///    writing our copyright header, writing the remaining file contents,
///    then overwriting the existing file with the temp file.
/// 3. The file fits comfortably in memory space. Besides, if a *code* file
///    is more than a few dozen kilobytes, you have other problems.
fn slide_file_contents(rust_handle : &File, offset: usize, amount : isize) {
    // Don't let us slide contents past the start of the file.
    assert!(offset as isize + amount >= 0);

    // If we have nothing to do, go home early.
    if amount == 0 {
        return;
    }

    // Generally, casting a file length to a isize would be a terrible idea.
    // (usize is 32 bits on x86, and files can be much larger than 4GB.)
    // But we're trying to mmap it (so it should fit in our address space),
    // and if a code file is that big...
    let file_length_64 : u64 = rust_handle.metadata().unwrap().len();
    if file_length_64 > isize::max_value() as u64 {
        panic!("One of the given code files is > 2GB. Call a doctor...");
    }
    let file_length = file_length_64 as usize;

    let fd = rust_handle.as_raw_fd(); // Get our classic Unix int file handle.
    // How long will the file be once we're done with it?
    let new_length = (file_length as isize + amount) as libc::off_t;

    if amount < 0 { // We have to shrink the file.
        // Shift its contents over.
        let mut mapping = Mapping::open(fd, file_length).unwrap();
        unsafe { // memmove, a la Rust
            ptr::copy(mapping.ptr().offset(offset as isize),
                      mapping.mut_ptr().offset(offset as isize + amount),
                      file_length - offset);
        }
        drop(mapping);

        // Then shrink it.
        unsafe {
            assert!(libc::ftruncate(fd, new_length) == 0);
        }
    }
    else if amount > 0 { // We have to grow the file.
        // Use fallocate instead of ftruncate to ensure that we have the room
        // on disk. See the man pages for posix_fallocate and ftruncate.
        unsafe {
            assert!(libc::posix_fallocate(fd, 0, new_length) == 0);
        }

        // Shift the contents over.
        let mut mapping = Mapping::open(fd, file_length).unwrap();
        unsafe { // memmove, a la Rust
            ptr::copy(mapping.ptr().offset(offset as isize),
                      mapping.mut_ptr().offset(offset as isize + amount),
                      file_length - offset);
        }
    }
    else { // wat
        unreachable!("We should account for this case with an early return.");
    }
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
    aligned_length: libc::size_t,
}

// Sizes passed to mmap need to be page-aligned.
// See http://stackoverflow.com/a/3407254/713961
fn round_to_page(len: libc::size_t) -> libc::size_t {
    lazy_static!{ // The page size won't change under our feet. Ask once.
        static ref PAGE_SIZE : libc::size_t = unsafe {
            libc::sysconf(libc::_SC_PAGESIZE) as libc::size_t
        };
    }
    let remainder = len % *PAGE_SIZE;

    if remainder == 0 {
        len
    }
    else {
        len + *PAGE_SIZE - remainder
    }
}

impl Mapping {
    fn open(fd: RawFd, file_length: usize) -> io::Result<Mapping> {
        let aligned_length = round_to_page(file_length as libc::size_t);
        let mapping = unsafe {
            libc::mmap(ptr::null_mut(),
                       aligned_length,
                       libc::PROT_READ | libc::PROT_WRITE,
                       libc::MAP_SHARED,
                       fd, 0)
        };
        if mapping == libc::MAP_FAILED {
            Err(io::Error::last_os_error())
        }
        else {
            Ok(Mapping{ ptr: mapping, aligned_length: aligned_length })
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
            assert!(libc::munmap(self.ptr, self.aligned_length) == 0,
                    "munmap failed with {}", io::Error::last_os_error());
        }
    }
}
