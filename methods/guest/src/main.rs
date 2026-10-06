//! Logos Names guest program. All logic lives in `names_core` so it can be
//! unit-tested on the host; this binary only wires it to the LEZ runtime.

use lee_core::program::run_program;
use names_core::{apply, plan};

fn main() {
    run_program(plan, apply)
}
