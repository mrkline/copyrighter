///! Common types and functions used by the rest of the binary.

use rayon_hash::{HashMap, HashSet};

pub type Year = u16;

pub type YearMap = HashMap<String, Vec<Year>>;

pub type PathSet = HashSet<String>;
