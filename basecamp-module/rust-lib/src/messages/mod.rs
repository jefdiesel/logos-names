//! Names Messages in the Basecamp wallet: chats between LNS names that work with the phone app and the Names web
//! app (the names format version 1, the phone's padding and safety codes), over Basecamp's own `delivery_module`
//! on logos.test. Kept apart from the rest of the module so it merges cleanly.

pub mod api;
pub mod crypto;
pub mod files;
pub mod service;
pub mod store;
#[cfg(test)]
mod probe;

/// The messaging node the module uses: Basecamp's delivery_module; in unit tests, one that sends nowhere.
#[cfg(not(test))]
pub type Node = service::Delivery;
#[cfg(test)]
pub type Node = service::Offline;

#[cfg(not(test))]
pub fn node() -> Node {
    service::Delivery
}
#[cfg(test)]
pub fn node() -> Node {
    service::Offline
}
