//! Prints each command-line argument on its own line, `Debug`-escaped, so Windows terminal
//! quoting tests can check how arguments survive `cmd.exe`.

fn main() {
    for argument in std::env::args_os().skip(1) {
        println!("{argument:?}");
    }
}
