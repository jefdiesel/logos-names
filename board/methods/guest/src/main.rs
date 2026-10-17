//! LNS board guest program. All logic lives in `board_core` so it can be
//! unit-tested on the host; this binary only wires it to the LEZ runtime.

use board_core::{apply, plan};
use lee_core::program::run_program;

fn main() {
    run_program(plan, apply)
}
