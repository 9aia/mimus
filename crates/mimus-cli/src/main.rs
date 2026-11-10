//! mimus — compat CLI driver. The engine is the `mimus` lib
//! (src/lib.rs); this binary is just `mimus::run()`.

fn main() {
    std::process::exit(mimus::run());
}
