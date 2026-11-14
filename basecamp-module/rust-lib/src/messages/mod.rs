//! Names Messages in the Basecamp wallet: chats between LNS names that work with the phone app and the Names web
//! app (the names format version 1, the phone's padding and safety codes), over Basecamp's own `delivery_module`
//! on logos.test. Kept apart from the rest of the module so it merges cleanly.

pub mod crypto;
#[cfg(test)]
mod probe;
