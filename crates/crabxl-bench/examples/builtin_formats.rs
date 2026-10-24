//! Expose canonical format spelling for independent public-baseline verification.
fn main() {
    for (id, code) in crabxl::BUILTIN_NUMBER_FORMATS {
        println!("{id}\t{code}");
    }
}
