//! LNS board guest program. All logic lives in `board_core` so it can be
//! unit-tested on the host; this binary only wires it to the LEZ runtime.

lee_core::actor_entrypoint!(board_core::handle);
