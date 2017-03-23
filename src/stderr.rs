
// Convenience macro to print to stderr
// See http://stackoverflow.com/a/32707058
#[macro_export]
macro_rules! stderr {
    ($($arg:tt)*) => (
        match writeln!(&mut ::std::io::stderr(), $($arg)* ) {
            Ok(_) => {},
            Err(x) => panic!("Unable to write to stderr (file handle closed?): {}", x),
        }
    )
}

