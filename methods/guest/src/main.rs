//! Logos Names guest program. All logic lives in `names_core` so it can be
//! unit-tested on the host; this binary only wires it to the LEZ runtime.

lee_core::actor_entrypoint!(names_core::handle);
