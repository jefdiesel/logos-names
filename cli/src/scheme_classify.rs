// SPDX-License-Identifier: MIT OR Apache-2.0
//! Which scheme an account's Names Wallet app secrets follow, with nothing pinned: ONE function,
//! shared byte for byte by the names CLI (`cli/src/scheme_classify.rs`, used by `private_call.rs`)
//! and the bridge's `lez-bridge` CLI (`lez-bridge/cli/src/scheme_classify.rs`, used by
//! `recover.rs`), pinned by the same vectors (`scheme-classify-v1.json` in both repositories).
//!
//! - **phrase** (values from the account's own key): a key-tree account, i.e. some node of the
//!   wallet's own trees gives the id under the account's kind (an id held in both maps counts), or
//!   a held key that the trees give within the next [`LOOK_AHEAD`] accounts they would make;
//! - **imported** (values from the wallet root `R` and the id): any other held key.

use lee_core::{PrivateAccountKind, account::AccountId};
use wallet::storage::key_chain::UserKeyChain;

/// How many accounts past each key tree's last one count as this phrase's own.
pub const LOOK_AHEAD: u32 = 20;

/// How the wallet holds an account, with its own 32-byte secret (a public account's signing key,
/// a private account's authorization secret key).
pub struct Held {
    pub own: [u8; 32],
    /// A node of the wallet's own key trees gives it.
    pub in_tree: bool,
    /// A private account's kind (`None` for a public account).
    pub kind: Option<PrivateAccountKind>,
}

/// The wallet's hold on `id`, or `None` when it holds no key for it (another wallet's account, a
/// shared or group account).
#[must_use]
pub fn held(kc: &UserKeyChain, private: bool, id: AccountId) -> Option<Held> {
    if !private {
        let key = kc.pub_account_signing_key(id)?;
        let in_tree = kc.public_account_ids().any(|(found, index)| found == id && index.is_some());
        return Some(Held { own: *key.value(), in_tree, kind: None });
    }
    let found = kc.private_accounts().find(|f| AccountId::for_private_account(&f.key_chain.nullifier_public_key, &f.key_chain.viewing_public_key, f.kind) == id)?;
    let kind = found.kind.clone();
    // Any tree node that gives this id under its kind (an imported account keeps the index it had
    // in the wallet it came from, and a kind it received at is not listed under the tree node).
    for (_, _, index) in kc.private_account_key_chains() {
        let Some(tree) = index.and_then(|index| kc.private_account_key_chain_by_index(index)) else { continue };
        if AccountId::for_private_account(&tree.nullifier_public_key, &tree.viewing_public_key, &kind) == id {
            return Some(Held { own: tree.private_key_holder.authorization_secret_key.0, in_tree: true, kind: Some(kind) });
        }
    }
    Some(Held { own: found.key_chain.private_key_holder.authorization_secret_key.0, in_tree: false, kind: Some(kind) })
}

/// Whether the next `n` accounts the wallet's trees would make give `id` (under `kind` for a
/// private account), worked out on a copy: nothing is stored.
#[must_use]
pub fn phrase_gives(kc: &UserKeyChain, private: bool, id: AccountId, kind: Option<&PrivateAccountKind>, n: u32) -> bool {
    let mut copy = kc.clone();
    for _ in 0..n {
        if private {
            let (made, index) = copy.generate_new_privacy_preserving_transaction_key_chain(None);
            if made == id {
                return true;
            }
            if let (Some(kind), Some(tree)) = (kind, copy.private_account_key_chain_by_index(&index)) {
                if AccountId::for_private_account(&tree.nullifier_public_key, &tree.viewing_public_key, kind) == id {
                    return true;
                }
            }
        } else if copy.generate_new_public_transaction_private_key(None).0 == id {
            return true;
        }
    }
    false
}

/// `phrase` or `imported` for a held account (see the module docs), with its own secret.
#[must_use]
pub fn classify(kc: &UserKeyChain, private: bool, id: AccountId) -> Option<(&'static str, [u8; 32])> {
    let h = held(kc, private, id)?;
    let phrase = h.in_tree || phrase_gives(kc, private, id, h.kind.as_ref(), LOOK_AHEAD);
    Some((if phrase { "phrase" } else { "imported" }, h.own))
}
