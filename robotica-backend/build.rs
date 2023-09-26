extern crate lalrpop;

fn main() {
    #[cfg(feature = "scheduler")]
    lalrpop::process_root().unwrap();
}
