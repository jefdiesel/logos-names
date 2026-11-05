//! Logos Names module: lets any Logos app resolve and manage `.lez` names
//! with one call, against registry v5 (SPEC.md).
//!
//! The trait below is the module's contract; the builder derives the `.lidl`
//! from it. Every method that answers a question returns JSON text in the
//! shape Logos modules use elsewhere:
//!
//! ```json
//! { "success": true,  "value": <answer>, "error": null }
//! { "success": false, "value": null,     "error": "why" }
//! ```
//!
//! A wallet's send field needs only `recipient`.
//!
//! Names are private: the chain holds an identifier derived from each name
//! (Argon2id, then SHA-256) and a sealed profile, never the name, and there is
//! no list of names. Whoever knows a name can resolve it; nobody can browse.
//! A record names no holder either, only a tag of the holder's account bound
//! to that one name.
//!
//! Registry v5 holds a name by a refundable deposit: it never lapses, and the
//! deposit comes back to whoever releases it. One- and two-character names are
//! locked, and three-character names take a higher deposit; the module proves a
//! name's length tier from a built-in copy of the short-name list. Claiming a
//! name is commit, then reveal at least 15 minutes later (see `register_name`).
//!
//! Reading needs nothing but the sequencer. Changing a name needs a signature.
//! The keys stay in the person's own `names` CLI wallet, and this module hands
//! the CLI each public change it builds (`wallet.rs`).
//!
//! A name held privately is changed by that same CLI, run in the background as
//! a job (`cli_*` and `job`; see `cli.rs`): its changes are proven on this
//! machine with the CLI's own keys.

mod avatar;
mod chain;
mod changes;
mod cli;
mod page_view;
mod provider;
mod spell;
mod registry;
mod shortlist;
mod wallet;

use changes::{Change, Mention};
use registry::{AccountId, Derived, Registry, Settings};
use wallet::Held;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const DEFAULT_SEQUENCER: &str = "https://testnet.lez.logos.co";
/// The canonical registry on the default network: the one SPEC.md section 11
/// lists, registry v5 in deposit mode.
const DEFAULT_REGISTRY: &str = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";
/// The snapshot of the earlier chain, published as a static file with the web app.
const CLAIMS_SNAPSHOT: &str = "https://logos-names.wrapit.workers.dev/claims/lez-0.3.json";
/// Registries a saved configuration may still name whose names use an older
/// derivation and can't be read by this version (v2, v3, v4.0 and v4.1 on the
/// public testnet). They are ignored.
const RETIRED_REGISTRIES: [&str; 4] = [
    "DugXYzVqYsXJumqsjaGtoZeJ742WmdRHGpByi8pNJm2z",
    "66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad",
    "E6oXebf6sicLtYBpEMijFdB8mqbXg3HRsWfYoJBgjvdj",
    "2PXg3SSmh5hvECTz4xcfV2d5YvxwyF6yVhpnjTokzCkX",
];

pub trait NamesModule: Send + 'static {
    /// Where to send a payment for `name` (`jef.lez` or `jef`). The value is
    /// `{name, display, public, public_hex, private}`: `private` is
    /// `{nullifier_public_key, viewing_public_key}` in hex or null, and
    /// `public` is an account ID or null. Prefer `private` when it is there.
    /// A name that forwards (its `forward` entry, SPEC.md §7) is paid where it
    /// points, one hop: then `name` is the target and `forwarded_from` the name
    /// asked for, which a payer must be shown. A null value means the name is
    /// not registered.
    fn recipient(&mut self, name: String) -> String;

    /// Everything `name` resolves to: `{name, display, owner_tag, expires,
    /// never, lapsed, deposit, account, npk, vpk, texts}`. A name held by a
    /// deposit never expires: `expires` is null, `never` true and `lapsed`
    /// false; `deposit` is what its deposit account holds (decimal text), which
    /// a release refunds. In fee mode `expires` is in milliseconds and `lapsed`
    /// is true once it has passed. `owner_tag` is the holder's tag (hex): the
    /// chain never names the holder. A null value means it is not registered,
    /// which includes a fee-mode name that lapsed more than 30 days ago.
    /// `forward` is the name it points to (SPEC.md §7, Forwarding), or null:
    /// `resolve` doesn't follow it, so the name's holder can still edit it.
    fn resolve(&mut self, name: String) -> String;

    /// The page published under `name`: `{content_type, bytes, text}`, where
    /// `text` is the body if it is valid UTF-8. A null value means no page.
    /// The page is written by the name's owner: treat it as untrusted content.
    fn page(&mut self, name: String) -> String;

    /// The page published under `name`, made safe to show inside an app:
    /// `{content_type, bytes, kind, title, html, text, truncated}`. `kind` is
    /// `html`, `text` or `file`. For an HTML page, `html` is the body rewritten
    /// to an allowlist Qt's rich text can draw (headings, paragraphs, lists,
    /// tables, emphasis, code, and links to `http:`/`https:` URLs only; no
    /// scripts, styles, images, attributes or comments), and `title` is its
    /// `<title>` as plain text; both are null otherwise. `text` is the page's
    /// own source, as in `page`, to be shown only as plain text. At most 64 KB
    /// of a page is read; `truncated` says it was cut. A null value means no page.
    fn page_view(&mut self, name: String) -> String;

    /// `body`, a page not yet published, as `page_view` would show it: an
    /// HTML body (one that opens with a tag) sanitized the same way, anything
    /// else as plain text. For previewing a page while it is written.
    fn page_preview(&mut self, body: String) -> String;

    /// The picture in `name`'s `avatar` entry, decoded, for an app that can't
    /// load a `data:` URL: `{width, height, pixels}`, where `pixels` is base64
    /// of RGBA8 bytes, row by row from the top left. At most 128 pixels a side
    /// (a larger picture is scaled down, nearest neighbour). Only PNG is
    /// decoded. A null value means the name has no avatar this can decode.
    fn avatar(&mut self, name: String) -> String;

    /// The name `account` (base58) has published for itself, or null. The
    /// answer is checked against the name's own record, so it cannot be spoofed.
    fn reverse(&mut self, account: String) -> String;

    /// The bare name for what a person typed (`Jef.LEZ` gives `jef`), or an
    /// empty string if it is not a valid name. Makes no network call.
    fn normalize(&mut self, input: String) -> String;

    /// The open LEZ wallet: `{accounts: [{account, balance, funded, name}],
    /// private_accounts: [account]}`. `accounts` are its public accounts,
    /// funded ones first; `balance` is decimal text and `name` is the name the
    /// account has published, or null. Fails if the LEZ wallet is not
    /// installed or not open.
    fn wallet(&mut self) -> String;

    /// The names this device remembers that the open wallet holds, and the
    /// registrations it has under way: `{names: [{name, display, owner,
    /// expires, never, lapsed}], pending: [{name, display, owner, stage,
    /// ready_at, tx}]}`. `owner` is the wallet's account that holds the name.
    /// A pending `stage` is `committed` (call `register_name` again from
    /// `ready_at`, in milliseconds) or `revealed` (sent; it shows in `names`
    /// once it lands). The chain keeps no list of names, so this is the
    /// device's own memory: every name registered from here, and every name
    /// passed to `keep`.
    fn mine(&mut self) -> String;

    /// Remembers `name` on this device, if the open wallet holds it, so `mine`
    /// keeps returning it. Fails for a name the wallet does not hold. The value is the bare name.
    fn keep(&mut self, name: String) -> String;

    /// What registering `name` takes: `{name, display, tier, deposit, mode,
    /// claims_until}`.
    /// `tier` is `normal`, `three` (a three-character name, at the higher
    /// deposit) or `locked` (one or two characters: only the registry's admin
    /// may register it, and `deposit` is null). `deposit` is decimal text in
    /// the native token's base units, held while the name is and refunded on
    /// release. `mode` is `deposit`, `fee` or `unconfigured`; in fee mode
    /// `deposit` is 0 and `fee` gives the price of a one-year term instead.
    /// `claims_until` is null, or while names from the earlier chain can still be
    /// claimed, when that ends, in milliseconds of chain time: until then a holder's
    /// claim replaces a registration of their name, and whoever registered it loses
    /// the deposit to them. An app must say so first (SPEC.md section 8.4).
    /// Makes one network read, besides deriving the name.
    fn deposit(&mut self, name: String) -> String;

    /// The registry's settings: `{mode, deposits: {normal, three}, admin,
    /// treasury, prices: {30d, 1y, 5y}, claims_until}`, amounts as decimal text,
    /// `claims_until` as in `deposit`. A null value
    /// means the registry has no settings yet (it charges nothing, and locks
    /// one- and two-character names).
    fn settings(&mut self) -> String;

    // Changing a name. Each method below checks the change against the chain,
    // has the LEZ wallet sign it with the account that must authorise it, and
    // sends it. The value is `{name, signer, tx}`, plus what the method lists.
    // A sent transaction can still be refused, so confirm by calling `resolve`
    // until the change shows; it normally takes one block. Send one change per
    // account at a time.

    /// Registers `name` to `owner` (base58), a public account in the open
    /// wallet, which pays the deposit (see `deposit`). Registering is two
    /// steps, so that nobody watching pending transactions can take the name
    /// first: the first call sends a commitment, which hides the name, and
    /// answers `stage: "committed"` with `ready_at` (milliseconds, chain time,
    /// about 15 minutes on). Calling `register_name` again from then on sends the
    /// reveal that claims the name (`stage: "revealed"`), and once that has
    /// landed, answers `stage: "registered"` without sending anything. A call
    /// before `ready_at` sends nothing. The commitment is kept on this device
    /// and stays usable for 24 hours. The value also has `deposit` (or `fee`)
    /// and `tier`; `tx` is null when nothing was sent.
    /// `private_account` makes the name payable privately from the start: one
    /// of the wallet's private accounts (base58), `auto` for its first one
    /// (made if it has none), or empty to set nothing. It is used by the
    /// reveal; given at commit time it is remembered for it.
    fn register_name(&mut self, name: String, owner: String, private_account: String) -> String;

    /// Makes `name` payable privately and only privately: publishes the
    /// receiving keys of `private_account` (as for `register_name`, `auto` allowed)
    /// and removes any public account from the name.
    fn receive_privately(&mut self, name: String, private_account: String) -> String;

    /// Points `name` at a public account (base58). Anyone who resolves the
    /// name then sees that account, its balance and its history.
    fn set_account(&mut self, name: String, account: String) -> String;

    /// Publishes the keys for private payments to `name`, both in hex: a
    /// 32-byte nullifier public key and a 1184-byte viewing public key.
    fn set_private_keys(&mut self, name: String, nullifier_public_key: String, viewing_public_key: String) -> String;

    /// Removes the public account and the private keys from `name`.
    fn clear_addresses(&mut self, name: String) -> String;

    /// Sets the text entry `key` on `name`; an empty value removes it.
    fn set_text(&mut self, name: String, key: String, value: String) -> String;

    /// Writes many text entries on `name` in one transaction, as the CLI's
    /// `names profile` does. `entries_json` is a JSON object of key to value
    /// (text); an empty value (or null) removes that key. The card keys are
    /// `display`, `bio`, `avatar` (a `data:image/...` URL), `theme` (`light`,
    /// `dark` or `auto`), `color` (`#rrggbb`), `font` (`sans`, `serif`, `mono`
    /// or `rounded`) and `cid` (a Logos Storage content ID); any other key of
    /// 1-32 characters of a-z, 0-9, `-`, `_` or `.` works too. With `replace`,
    /// every existing entry is dropped first, so the object becomes the whole
    /// set; the name's account and payment keys stay either way. A name holds
    /// at most 16 entries and its sealed profile at most 12 KiB.
    fn set_profile(&mut self, name: String, entries_json: String, replace: bool) -> String;

    /// Publishes `body` as the page of `name`; an empty body removes the page.
    fn set_page(&mut self, name: String, content_type: String, body: String) -> String;

    /// Hands `name` to `new_owner` (base58); only a tag of it reaches the
    /// chain. The addresses, entries and page the current owner set are
    /// emptied. The deposit stays with the name, for whoever releases it.
    fn transfer(&mut self, name: String, new_owner: String) -> String;

    /// Gives `name` up. Its record and page are emptied, anyone may register
    /// it again, and its deposit comes back to the holder: the value has
    /// `refund`, the deposit account's whole balance (decimal text).
    fn release(&mut self, name: String) -> String;

    /// Publishes `name` as what `account` (base58, in the open wallet) is
    /// shown as; an empty name withdraws it. This is the one change that puts
    /// a name on the chain in the clear.
    fn set_primary(&mut self, account: String, name: String) -> String;

    /// The sequencer URL, registry program ID and wallet files in use.
    fn get_config(&mut self) -> String;

    /// Use the registry with this program account ID (base58), and remember it
    /// across restarts. Returns false and changes nothing if it is not an
    /// account ID.
    fn set_registry(&mut self, program_id: String) -> bool;

    /// Read from the sequencer at this URL, and remember it across restarts.
    /// Returns false and changes nothing unless it is an http or https URL.
    fn set_sequencer(&mut self, url: String) -> bool;

    /// Names the wallet to open when the LEZ wallet module is running with no
    /// wallet open: the paths of its configuration and storage files, as the
    /// LEZ Wallet app writes them. Remembered across restarts; two empty paths
    /// forget it. Returns false and changes nothing if a file does not exist.
    fn set_wallet_files(&mut self, config_path: String, storage_path: String) -> bool;

    // Names held through the person's own `names` CLI. A name held by a
    // private account is changed with a proof made on this machine, which the
    // CLI does with the keys in its own wallet directory. Each change below
    // runs the CLI in the background, as a job, and answers at once with
    // `{id, name, kind, state, started}`; `job` says how it is going. Proving
    // takes a few minutes. At most one job runs per name. The CLI is given
    // this module's registry and sequencer, and never run through a shell.

    /// Takes back `name`, held on the earlier chain by an account of the CLI's wallet,
    /// for free, with its profile and page as they were: starts `names claim` as a job.
    /// `snapshot` is the snapshot the registry's claims list was made from: a file's full
    /// path, an `http` or `https` URL, or empty for the one published with the web app.
    /// Until `claims_until` this replaces a registration of the name. Only a name held by
    /// a public account can be claimed: LEZ 0.4 derives other private account IDs.
    fn cli_claim(&mut self, name: String, snapshot: String) -> String;

    /// Brings back a wallet from before LEZ 0.4, which no longer opens, from its recovery
    /// phrase. `cli_status` says when one is needed (`old_wallet`). The old wallet's files
    /// stay beside the new ones. The value is `{public: [account], private, moved: [path]}`:
    /// the first public accounts the phrase derives, since a new chain shows none of them
    /// as used, and a new private account. Tokens are not carried over.
    fn cli_restore_wallet(&mut self, phrase: String) -> String;

    /// The names the CLI's wallet holds, as `names mine` lists them: `[{name,
    /// display, signer, state, lapsed}]`, `signer` being `cli`. A wallet
    /// directory with no wallet in it holds none (and the CLI is not run, so
    /// it makes none). Waits for the CLI, at most 15 seconds.
    fn cli_mine(&mut self) -> String;

    /// Starts `names profile` for `name`: `entries_json` is a JSON object of
    /// key to text, as for `set_profile`. `display`, `bio`, `theme`, `color`,
    /// `font` and `cid` go to their own options, any other key to `--set`, and
    /// an empty value (or null) removes the key. `avatar` is passed only when
    /// it is the full path of a PNG file on this machine; any other avatar
    /// value is left as it is (with `replace`, a `data:image/` URL is kept),
    /// and the answer's `skipped` lists it.
    fn cli_profile(&mut self, name: String, entries_json: String, replace: bool) -> String;

    /// Starts `names set-page` with `body` (written to a file in the jobs
    /// directory, removed once the job ends); an empty body starts `names
    /// clear-page`.
    fn cli_set_page(&mut self, name: String, content_type: String, body: String) -> String;

    /// Starts `names set-text`; an empty value removes the entry.
    fn cli_set_text(&mut self, name: String, key: String, value: String) -> String;

    /// Registers `name` through the names CLI, from its private account, so
    /// the holder never appears on chain. Starts a job like `cli_set_text`;
    /// it ends once the commitment is made (a proof of a few minutes), and
    /// the CLI finishes the registration in the background about 15 minutes
    /// later. `cli_mine` lists it with `registering` true until then. With
    /// `public`, the CLI then moves it to a new public account funded privately
    /// (`names make-public`), so it posts in one block and nothing links it to you.
    fn cli_register(&mut self, name: String, as_public: bool) -> String;

    // ── The LNS board (see board/ and docs/BOARD-PLAN.md) ──────────────

    /// The board's recent posts, newest first, read through the names CLI:
    /// `{posts, updating, error}`, where `posts` is `[{post, name, n, parent,
    /// time, text, cids, likes, hidden, block}]` (`post` and `parent` written
    /// `<name>/<n>`). Reading takes the CLI many seconds, so this answers at once
    /// with the last result and fetches a newer one on its own thread; while
    /// `updating` is true, ask again shortly. With `following` true, only posts
    /// by the names this device follows.
    fn board_feed(&mut self, following: bool) -> String;

    /// Ten posts by `name`, newest first, below post number `before` (its newest
    /// when negative), read straight from the board: answered as `board_feed`
    /// is, with `posts` holding `{total, posts}`.
    fn board_posts_of(&mut self, name: String, before: i64) -> String;

    /// A post (`<name>/<n>`) and its replies, oldest first, answered as `board_feed` is.
    fn board_thread(&mut self, post: String) -> String;

    /// Posts `text` as `name`, which the CLI's wallet holds privately, with up to
    /// four Logos Storage CIDs (`cids_json`, a JSON list) and, if `reply` is a
    /// post, as a reply to it. A CLI job, as `cli_set_text`; about 4 minutes.
    fn board_post(&mut self, name: String, text: String, cids_json: String, reply: String) -> String;

    /// Likes `post` as `name` (or takes the like back with `undo`). A CLI job.
    fn board_like(&mut self, name: String, post: String, undo: bool) -> String;

    /// Takes down `post`, one of the CLI wallet's names' posts. A CLI job.
    fn board_hide(&mut self, post: String) -> String;

    /// The names this device follows, a JSON list.
    fn board_follows(&mut self) -> String;

    /// Follows `name` (or stops, with `undo`); kept on this device only.
    fn board_follow(&mut self, name: String, undo: bool) -> String;

    /// The name this device posts, likes and pays as, or null if none is set.
    fn main_name(&mut self) -> String;

    /// Sets the name this device posts, likes and pays as.
    fn set_main_name(&mut self, name: String) -> String;

    /// The names CLI wallet's anonymous name (`names anon`), or null if it has none yet.
    fn anon_name(&mut self) -> String;

    /// Makes the wallet's anonymous name, a CLI job (`names anon`): a private
    /// transfer of a few minutes, then a registration of about 20 that the CLI
    /// finishes in the background.
    fn make_anon(&mut self, as_public: bool) -> String;

    /// The CLI jobs still running, `[{id, name, kind, state, started}]`, so an app
    /// opened mid-change can show it and wait for it.
    fn running_jobs(&mut self) -> String;

    /// Moves `name` from the CLI wallet's private account to a new public account
    /// in it (`names make-public`), so its posts take one block and pay gas. A CLI
    /// job of about 9 minutes; the name keeps its profile, page and receiving keys.
    fn make_public(&mut self, name: String) -> String;

    /// The posts this device's names have liked, `<name>/<n>`.
    fn board_likes(&mut self) -> String;

    /// Which name each chat conversation is with: `{convo_id: name}`.
    fn dm_names(&mut self) -> String;

    /// Remembers that chat conversation `convo_id` is with `name`; an empty
    /// name forgets it.
    fn set_dm_name(&mut self, convo_id: String, name: String) -> String;

    /// Which name each chat address belongs to: `{address: name}`.
    fn chat_names(&mut self) -> String;

    /// Remembers that chat address `address` is `name`'s.
    fn set_chat_name(&mut self, address: String, name: String) -> String;

    /// The words in `text` that look misspelled, `[{word, suggestions}]`, checked
    /// on this machine against the system's word list. Names, links and CIDs are skipped.
    fn spell_check(&mut self, text: String) -> String;

    /// Likes `post` as `name`, or takes the like back if this device liked it.
    /// A CLI job; the like is remembered here once it is started.
    fn board_toggle_like(&mut self, name: String, post: String) -> String;

    /// Pays `amount` (base units, decimal text) to `name` from the CLI wallet's
    /// private account, privately (`names pay`). A CLI job; about 4 minutes.
    fn cli_pay(&mut self, name: String, amount: String) -> String;

    /// The names CLI's default private account and its balance:
    /// `{account, balance, public}` (decimal text), or null if it has none.
    /// `public` is its default public account the same way, `{account,
    /// balance}`, or null: that is the account `cli_bridge` credits. Runs
    /// `names balance`, which first catches up with the chain (seconds to a
    /// minute), so ask with a long timeout.
    fn cli_balance(&mut self) -> String;

    /// Bridges `amount` (base units, decimal digits) from the notes of the
    /// Logos Blockchain public key `from_key` (64 hex characters) into LEZ,
    /// credited to the names CLI wallet's default public account, through the
    /// person's own Logos Blockchain node at `node` (`http(s)://127.0.0.1:<port>`
    /// or `localhost:<port>`; empty is Basecamp's, `http://127.0.0.1:8080`).
    /// Runs `names bridge` and waits for it (no proving; at most two minutes).
    /// Without `send` it only shows the plan and moves nothing; with `send`
    /// the deposit is made and cannot be undone. LEZ credits it once the Logos
    /// Blockchain block holding it is final, about 12 hours on the testnet.
    /// The value is `{sent, lines, hash, total, notes, fee, to}`: `lines` are
    /// what the CLI printed, `total` what the whole notes picked add up to
    /// (a little more than `amount`, at most 30 notes), `hash` the deposit's
    /// (null for a plan). The key and node are remembered (`get_config`'s
    /// `bridge_key` and `bridge_node`). Every argument is checked first.
    fn cli_bridge(&mut self, amount: String, from_key: String, node: String, send: bool) -> String;

    // ── Getting started without a terminal ─────────────────────────────

    /// Asks the Names faucet to pay the wallet's default public account a
    /// little LEZ (2,000,000 on the testnet: a name's 1,000,000 deposit and as
    /// much to use), once a day per account and per internet address. `{account, amount, tx}` once the faucet has sent
    /// it; it arrives within a minute or two (`cli_balance` shows it). The
    /// faucet's own refusal (funded today, empty, busy) comes back as the error.
    fn cli_faucet(&mut self) -> String;


    /// Where the names CLI stands, read without running it: `{cli, prover,
    /// installing, wallet_dir, wallet, public, private, busy}`. `cli` is the
    /// program the module would run (null if none is found), `prover` whether
    /// RISC Zero's prover is beside it, `installing` the running `cli_install`
    /// job or null, `wallet` whether its wallet folder holds a wallet,
    /// `public` and `private` the accounts `names setup` made (null until
    /// then), and `busy` the running wallet job (`cli_shield` or
    /// `cli_pay_account`) or null. Answers at once.
    fn cli_status(&mut self) -> String;

    /// Installs the names CLI and RISC Zero's prover into the module's own
    /// folder (`~/.config/logos-names/bin`, man pages in `.../man`), by running
    /// the official installer built into the module: it checks every file's
    /// signature against the release key and its SHA-256, and changes nothing
    /// else (not the shell's startup files). A CLI that already works is left
    /// alone. The value is `{installed, path, version, prover, job}`: with
    /// `installed` true there is nothing to do; otherwise `job` is the install
    /// job (`{id, name, kind: "install", state, started}`, seconds to a minute, most
    /// of it the 70 MB prover). `job` on it says how it goes; once it is
    /// `done`, its state carries `version` and `path` and the module uses that
    /// CLI from then on (saved as `cli`, with `cli_wallet` the CLI's default
    /// wallet folder if none was set). A failed one carries `problem`, in plain
    /// words (no internet, signature check failed, unsupported computer), and a
    /// done one whose prover didn't download carries `warning`.
    fn cli_install(&mut self) -> String;

    /// Makes the names CLI's wallet, with a public and a private account, if
    /// it has none: `names setup` moving nothing, so it never proves. Waits
    /// for it, a few seconds (at most three minutes if an old wallet catches
    /// up with the chain). The value is `{public, private, phrase}`: `phrase`
    /// is the new wallet's recovery phrase, the only way to get the wallet
    /// back, shown here once and never logged; it is null if the wallet already
    /// existed. If the CLI failed after making the wallet, the value also has
    /// `problem`, so the phrase isn't lost.
    fn cli_create_wallet(&mut self) -> String;

    /// Moves `amount` (empty: 5,000,000, enough for a few names' deposits)
    /// from the wallet's public account to its private one, so names can be
    /// held and paid privately: `names setup` as a job that proves for a few
    /// minutes. Once it ends, its `job` state has `moved` and `problem`: the
    /// CLI moves nothing, and says so, when the public account holds too little.
    fn cli_shield(&mut self, amount: String) -> String;

    /// Pays `amount` to the public account `account`, written `Public/<id>`
    /// (a name is paid with `cli_pay`), as a job: `names wallet auth-transfer
    /// send`. From the wallet's private account with `from_private` (a proof
    /// of a few minutes; the chain shows the account paid and the amount, not
    /// the payer) or else its public one (one block; the chain shows the payer
    /// too).
    fn cli_pay_account(&mut self, account: String, amount: String, from_private: bool) -> String;

    /// How job `id` is going: `{id, name, kind, state, started, finished,
    /// exit, log_tail}`. `state` is `running`, `done` (the CLI exited 0: the
    /// change was sent and included; `resolve` shows it) or `failed`;
    /// `log_tail` is the end of the CLI's output, whose last line says why it
    /// failed. Times are in milliseconds. `fee` is what the changes it sent
    /// cost, `{about, at_most, paid}` (decimal text; `paid` null until one
    /// applied, or if it couldn't be measured), or null if it sent none that
    /// the CLI prices (a payment to a bare account does not).
    fn job(&mut self, id: String) -> String;

    /// Runs the `names` CLI at this path, and remembers it across restarts;
    /// an empty path goes back to the default (`~/.local/bin/names`, then the
    /// first `names` on PATH). Returns false and changes nothing if it is not
    /// an executable file.
    fn set_cli(&mut self, path: String) -> bool;

    /// Has the CLI use the wallet in this directory (`LEE_WALLET_HOME_DIR`),
    /// and remembers it across restarts; an empty path goes back to the
    /// default, `~/.lee/wallet`. Returns false and changes nothing if it is
    /// not a directory.
    fn set_cli_wallet(&mut self, dir: String) -> bool;

    // ── Tokens of a token program (docs/wallet/TOKENS.md) ──────────────
    //
    // All through the names CLI's wallet (`names token`), with the token
    // program set here. No token program runs on the public LEZ testnet yet,
    // so none is built in. Names app only.

    /// Has every token method use the token program `program_id` (base58),
    /// and remembers it across restarts (`get_config`'s `token_program`); an
    /// empty ID forgets it. Returns false and changes nothing if it is not an
    /// account ID, or is the native token.
    fn set_token_program(&mut self, program_id: String) -> bool;

    /// What the names CLI's accounts hold of each token, public and private:
    /// `{token_program, holdings: [{account, account_kind, definition, name,
    /// balance}], updating, error}`, only `account`'s (written `Public/<id>`
    /// or `Private/<id>`) unless it is empty. `name` is the definition's own
    /// name (the token maker's text, cleaned) or null; amounts are decimal
    /// text in base units. Read in the background (a private account's
    /// holdings need the wallet to catch up with the chain, up to a minute or
    /// two): the answer comes at once with the last read, null until the
    /// first ends, and `updating` while a newer one is on the way.
    fn cli_token_holdings(&mut self, account: String) -> String;

    /// The token credits sent to the wallet's private keys and not yet
    /// received: `{token_program, pending: [{position, definition, name,
    /// amount, to}], updating, error}`, `to` the private account they credit.
    /// From the same read as `cli_token_holdings`. `cli_token_receive` takes them.
    fn cli_token_pending(&mut self) -> String;

    /// What the token `definition` (its definition's account ID) is:
    /// `{definition, name, supply, authority}`; `authority` may mint more,
    /// null for a fixed supply. Read from the chain through the CLI (seconds),
    /// then kept for 30 seconds.
    fn cli_token_info(&mut self, definition: String) -> String;

    /// Sends `amount` (base units, decimal text up to 2^128 - 1) of the token
    /// `definition` (its definition's account ID) to `to`: a name, a public
    /// account written `Public/<id>`, or one of the wallet's own
    /// `Private/<id>`. From the wallet's main private account with
    /// `from_private`, else its main public one. To a name it goes to the
    /// name's private receiving keys, where it waits until its holder receives
    /// it, unless `to_public` (or the name has only a public account). A
    /// wallet job (`names token send`): one block from the public account to
    /// a public account; otherwise proven on this computer, 7-12 minutes.
    /// Once it ends, its `job` state has `tx`, `block` and, if it failed,
    /// `problem`. Everything is checked before the CLI runs.
    fn cli_token_send(&mut self, definition: String, amount: String, to: String, from_private: bool, to_public: bool) -> String;

    /// Receives every token credit waiting for the wallet's private keys
    /// (`names token receive`), as a wallet job: one proven transaction per
    /// credit, 7-12 minutes each, paid from a private account's LEZ. A job
    /// stops after 45 minutes; credits it didn't reach stay pending and the
    /// next receive takes them. Its ended `job` state has `applied` (how many
    /// were received), `nothing` (there was nothing to receive) and `problem`.
    fn cli_token_receive(&mut self) -> String;

    // ── Accounts and fees: the names CLI's wallet. Names app only. ──────

    /// Every account of the names CLI's wallet: `[{account, kind, label,
    /// balance, default, public_key}]`. `account` is written `Public/<id>` or
    /// `Private/<id>`; `label` the nickname given here, or null; `balance`
    /// decimal text (a public account's read from the chain, a private one's as
    /// the wallet last saw it), or null if it couldn't be read; `default` marks
    /// the account of its kind the wallet pays and registers from. `public_key`
    /// (hex, public accounts only) is what someone needs to pay a public
    /// account the chain hasn't seen yet. The wallet catches up with the chain
    /// first: seconds to a minute or two, so ask with a long timeout.
    fn cli_accounts(&mut self) -> String;

    /// Makes a new `kind` (`public` or `private`) account in the wallet, with
    /// the nickname `label` (empty for none, at most 40 characters), and gives
    /// it as a `cli_accounts` entry. Nothing is sent and nothing is paid. A
    /// wallet without a default account of that kind makes it the default.
    /// Refused while a CLI job runs, since a job saves the wallet when it ends.
    fn cli_account_new(&mut self, kind: String, label: String) -> String;

    /// Gives the wallet's `account` (`Public/<id>` or `Private/<id>`) the
    /// nickname `label`; an empty one clears it. `{account, label}`.
    fn cli_account_label(&mut self, account: String, label: String) -> String;

    /// Pays and registers from `account` (`Public/<id>` or `Private/<id>`)
    /// from now on: it becomes the default of its kind, and `cli_balance`,
    /// `cli_pay`, `cli_register` and the rest use it. Names already held stay
    /// with the accounts holding them; payments to a name go where the name
    /// points, as before. Apps keep the accounts they were granted. Gives the
    /// account as a `cli_accounts` entry. Refused while a CLI job runs.
    fn cli_account_use(&mut self, account: String) -> String;

    /// The chain's fees now: `{height, base_fee_exec, base_fee_stor,
    /// public_transfer: {about, at_most}, private_transfer: {about,
    /// at_most}}`, amounts in base units as decimal text. `about` is what a
    /// transfer costs at the current base fees, `at_most` what is held for it
    /// (a public transfer) or reserved in its proof (a private one) should
    /// fees climb before it lands. A job's state has `fee` for what it sent:
    /// `{about, at_most, paid}`, `paid` measured once it applied.
    fn cli_fee(&mut self) -> String;

    // ── The Logos Blockchain (LP-0022): Basecamp's own node. Names app only. ──
    //
    // Asked of `blockchain_module` over Logos Core; the node holds the keys and
    // proves each transfer. Each fails with advice when the node isn't running.

    /// The node: `{height, slot, lib_slot, mode, epoch}`; `lib_slot` is the
    /// last final block's slot, `mode` the node's (`Online` once synced).
    fn chain_status(&mut self) -> String;

    /// The keys the node's wallet holds: `[{key, balance}]`, `balance`
    /// decimal text or null if it couldn't be read.
    fn chain_accounts(&mut self) -> String;

    /// Stake: `{staked, notes, keys, epoch, next_epoch_in_s, claimable,
    /// vouchers, reward_per_voucher}`, amounts as decimal text. `staked` is
    /// what the node's notes put in the leader lottery (any note on its keys,
    /// once in an epoch's snapshot); `next_epoch_in_s` is null when the node
    /// doesn't say. To unstake, send to a key the node doesn't hold.
    fn chain_stake(&mut self) -> String;

    /// Where `to` sends: `{key, name, forwarded_from}`. A key (64 hex) is
    /// itself; a name is the key its `blockchain` entry publishes, following
    /// a `forward` one hop (`forwarded_from` is then the name asked for).
    fn chain_recipient(&mut self, to: String) -> String;

    /// Sends `amount` (base units: digits, commas only between groups of
    /// three, so `1,000` but never `1,5`) from the node's key
    /// `from_key` to `to` (a key, or a name as `chain_recipient` reads it).
    /// Without `send` it checks everything and gives the plan `{from, to,
    /// name, forwarded_from, amount, balance}`; with `send` the transfer is
    /// made (it cannot be undone) and `hash` and `fee` (what it paid, decimal
    /// text, or null if it couldn't be read) are added. The node proves it,
    /// which can take a minute, so ask with a long timeout. Keys the node lists
    /// but its wallet doesn't hold are left out of `chain_accounts`.
    fn chain_send(&mut self, from_key: String, to: String, amount: String, send: bool) -> String;

    /// Whether transaction `hash`, sent with `chain_send` on this device (kept
    /// across restarts), is in a block: `{hash, state, fee}`. `state` is
    /// `pending` until its input notes leave the sender's wallet, then
    /// `in_block`. A hash not sent from here fails, saying why.
    fn chain_tx(&mut self, hash: String) -> String;

    /// The node's leader rewards: without `send`, `{claimable, vouchers,
    /// reward_per_voucher}`. With `send`, one transaction claims ONE voucher
    /// (the node's `leader_claim` makes a single claim op) and pays its own
    /// fee from the node's leader funding key, which can be more than the
    /// reward: `{hash, claimed, fee, vouchers_left, claimable_left}`, with
    /// `warning` when the fee was more than `claimed`. Fails when there is
    /// nothing to claim.
    fn chain_claim(&mut self, send: bool) -> String;

    // ── Wallet Provider SDK: for any app (docs/wallet/PROVIDER-SDK.md) ──
    //
    // Any app may ask; the person decides in the Names app. The asking app
    // is the one Basecamp names as the caller, never one an argument names.
    // Each request answers `{request, kind, state, ...}` (see
    // `request_status`); `state` starts `pending` and the app polls it.

    /// Asks to see some of the person's accounts. `reason` (at most 280
    /// characters) is shown in the prompt as the app's own words. If the app
    /// already has access the value is `{request: null, state: "approved",
    /// accounts}` and nobody is asked; a request already waiting is returned
    /// rather than a second one made.
    fn request_access(&mut self, reason: String) -> String;

    /// The accounts this app has been given: `{connected, accounts:
    /// [{account, account_kind}]}`, where `account` is `Public/<id>` or
    /// `Private/<id>` and `account_kind` is `public` or `private`. Asks nobody.
    fn granted_accounts(&mut self) -> String;

    /// The LEZ balance of each account this app has been given, `[{account,
    /// account_kind, balance, updating, error}]`; `balance` is decimal text
    /// or null until first read. Balances are read in the background and the
    /// answer comes at once with the last one read (`updating` while a newer
    /// one is on the way); ask again for a fresh one. A private account's
    /// balance is only here if the person gave this app that account.
    fn account_balances(&mut self) -> String;

    /// Asks to send `amount` (base units, decimal digits) of `asset` to
    /// `destination`: a name, or a public account written `Public/<id>`.
    /// `asset` is `LEZ`, the native token (empty also means LEZ), or a token
    /// of the wallet's token program, written as its definition's account ID
    /// (base58); the prompt then shows the token's name and what the Program
    /// Registry says about the token program. The person chooses the account
    /// it comes from. Checked now, so a bad amount, destination or token
    /// fails here.
    fn propose_transfer(&mut self, asset: String, amount: String, destination: String) -> String;

    /// Asks to run program `program_id` (base58) with the message
    /// `instruction_hex` (the bytes its transaction carries, on LEZ 0.4
    /// `borsh(Message::Instruction(..))`; at most 2,048 bytes), signed by the
    /// account the person chooses. That account's own actor of the program
    /// comes first; `accounts_json` lists the up to 16 other actors it
    /// reaches, in order: a public account ID (the program's data on it), or
    /// `{"account", "seed", "balance"}`, where `seed` (64 hex) names one of
    /// the program's own accounts (and lets the call create it), `balance:
    /// true` means its LEZ balance rather than the program's data, and
    /// `"account": "signer"` (with `balance: true`) is the chosen account's
    /// LEZ, which only then the program may move.
    /// `summary` (at most 280 characters) is shown as the app's own words;
    /// the prompt also says what the call can do and whether the program's
    /// source is verified (from `program_registry`, when installed).
    fn propose_call(&mut self, program_id: String, accounts_json: String, instruction_hex: String, summary: String) -> String;

    /// How this app's request `request_id` stands: `{request, kind, state,
    /// created, expires, decided, account, tx, error, ...}`. `state` is
    /// `pending` (waiting for the person), `approved` (access given;
    /// `accounts` lists it), `running` (a transfer under way), `done` (a
    /// transfer included in a block, or a call sent: `tx` is its hash),
    /// `failed` (`error` says why), `rejected`, `expired` (nobody answered
    /// in 10 minutes) or `cancelled`. `account` is the account the person
    /// chose. Only the app that made a request can read it.
    fn request_status(&mut self, request_id: String) -> String;

    /// Withdraws this app's own request while it is still pending.
    fn cancel_request(&mut self, request_id: String) -> String;

    /// Gives up the access this app was given. `{revoked}`.
    fn drop_access(&mut self) -> String;

    // ── Wallet Provider SDK: the Names app decides ─────────────────────

    /// The requests waiting for the person, oldest first, each with
    /// everything its prompt shows and `choices`, the accounts it may use:
    /// `[{account, account_kind, pays, label, granted}]`, all in the names
    /// CLI's wallet: a payment comes from its main public or private account
    /// (`pays`), a call is signed by any of its public accounts (those beyond
    /// the main one are listed from a read in the background, so they may
    /// appear a moment later). Names app only.
    fn pending_requests(&mut self) -> String;

    /// Approves request `request_id` with `account`, one of its `choices`
    /// (for access, one or more separated by commas). Access is given at
    /// once; a transfer starts as a names CLI job (`job` in the value); a
    /// call is tried, signed and sent by the names CLI (`names module send`)
    /// and the answer waits for its block (up to two minutes). If the wallet
    /// is busy the request stays pending and the error says so. Names app only.
    fn approve_request(&mut self, request_id: String, account: String) -> String;

    /// Rejects request `request_id`. Names app only.
    fn reject_request(&mut self, request_id: String) -> String;

    /// Which apps have access to which accounts: `[{app, accounts, since}]`.
    /// Names app only.
    fn app_grants(&mut self) -> String;

    /// Takes back all access `app` was given. Names app only.
    fn revoke_app(&mut self, app: String) -> String;

    /// Called once the module is wired to the host.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

// The builder injects the generated scaffold here: the `install` entry point,
// `RustModuleContext`, `context()` and `modules()`. Unit tests build without it.
#[cfg(not(test))]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[cfg(test)]
pub struct RustModuleContext;

struct Names {
    sequencer: String,
    program: Option<AccountId>,
    /// Deriving a name's identifier and key is slow on purpose, so each name
    /// asked about is derived once.
    derived: HashMap<String, Derived>,
    /// The wallet's configuration and storage files, if one was named.
    wallet_files: Option<(String, String)>,
    /// The `names` CLI and its wallet directory, if set (see `set_cli`).
    cli: Option<String>,
    cli_wallet: Option<String>,
    /// Where CLI jobs keep their logs; the config directory's `jobs` if unset.
    jobs_dir: Option<std::path::PathBuf>,
    /// Where `cli_install` puts the CLI (and the man pages beside it, in
    /// `man`); `install_dir()` if unset.
    bin_dir: Option<std::path::PathBuf>,
    /// The names followed on the board, kept on this device only.
    follows: Vec<String>,
    /// The name this device posts, likes and pays as (see `set_main_name`).
    main_name: Option<String>,
    /// The Logos Blockchain public key and node last bridged from (`cli_bridge`).
    bridge_key: Option<String>,
    bridge_node: Option<String>,
    /// The posts this device's names have liked, `<name>/<n>`, so a like toggles.
    likes: Vec<String>,
    /// Which name each chat conversation is with, by conversation id, as this
    /// device started or labelled it. The chat module itself keeps no names.
    dm_names: std::collections::BTreeMap<String, String>,
    /// Which name each chat address belongs to, as this device learned it (by
    /// messaging or adding that name), so group messages show who wrote them.
    chat_names: std::collections::BTreeMap<String, String>,
    /// Board reads, by what was asked: they take the CLI many seconds, so they
    /// run on their own thread and every call answers at once with the last result.
    board_reads: std::sync::Arc<std::sync::Mutex<HashMap<String, BoardRead>>>,
    /// Recent answers to reads about a name (resolve, page, avatar), by kind and
    /// name, so screens that ask again don't wait on the network. A change to
    /// the name drops them.
    read_cache: HashMap<(String, String), (String, std::time::Instant)>,
    /// What other apps have asked the wallet for, and what they were given
    /// (Wallet Provider SDK), read from disk the first time it is needed.
    provider: Option<provider::Store>,
    /// Where that is kept; the config directory's `provider.json` if unset.
    provider_path: Option<std::path::PathBuf>,
    /// The token program the token methods use (see `set_token_program`).
    token_program: Option<AccountId>,
    /// Recent `cli_token_info` answers, by token program and definition.
    token_info: HashMap<(AccountId, AccountId), (Value, std::time::Instant)>,
    /// What the Program Registry said about each asking app's publisher, and when (seconds).
    publishers: HashMap<String, (Value, u64)>,
}

/// How long a cached read about a name is used before it is read again.
const READ_FRESH: std::time::Duration = std::time::Duration::from_secs(30);

/// A board read: the last result, and whether a newer one is being fetched.
#[derive(Default)]
struct BoardRead {
    value: Option<Value>,
    error: Option<String>,
    updating: bool,
    /// When the last read ended (only kept by `read_in_background`).
    fetched: Option<std::time::Instant>,
}

/// Where this module keeps what it remembers between runs.
#[cfg(not(test))]
fn config_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".config/logos-names"))
}

/// Tests keep theirs in a folder of their own, never the person's.
#[cfg(test)]
fn config_dir() -> Option<std::path::PathBuf> {
    Some(std::env::temp_dir().join(format!("logos-names-test-config-{}", std::process::id())))
}

/// The installer `cli_install` runs: the official one, built in.
#[cfg(not(test))]
fn installer_script() -> String {
    cli::INSTALLER.to_owned()
}

/// Tests never download anything: their installer writes a stand-in CLI and
/// prover, and records what it was given.
#[cfg(test)]
fn installer_script() -> String {
    r#"#!/bin/sh
set -eu
env > "$NAMES_INSTALL_DIR/../installer-env"
printf '#!/bin/sh\necho "names 9.9.9-test"\n' > "$NAMES_INSTALL_DIR/names"
printf '#!/bin/sh\necho "r0vm 3.0.5"\n' > "$NAMES_INSTALL_DIR/r0vm"
chmod 755 "$NAMES_INSTALL_DIR/names" "$NAMES_INSTALL_DIR/r0vm"
echo "Installed $NAMES_INSTALL_DIR/names"
"#
    .to_owned()
}

/// Where `cli_install` puts the names CLI and its prover: the module's own
/// folder, so nothing else on the computer changes.
fn install_dir() -> Option<std::path::PathBuf> {
    config_dir().map(|dir| dir.join("bin"))
}

impl Default for Names {
    fn default() -> Self {
        let mut names = Self {
            sequencer: DEFAULT_SEQUENCER.to_owned(),
            program: registry::parse_account(DEFAULT_REGISTRY).ok(),
            derived: HashMap::new(),
            wallet_files: None,
            cli: None,
            cli_wallet: None,
            jobs_dir: None,
            bin_dir: None,
            follows: Vec::new(),
            main_name: None,
            bridge_key: None,
            bridge_node: None,
            likes: Vec::new(),
            dm_names: Default::default(),
            chat_names: Default::default(),
            board_reads: Default::default(),
            read_cache: HashMap::new(),
            provider: None,
            provider_path: None,
            token_program: None,
            token_info: HashMap::new(),
            publishers: HashMap::new(),
        };
        // Files saved by versions before 0.10 were readable by every account on the
        // machine; this closes them up once.
        if let Some(dir) = config_dir() {
            tighten(&dir);
        }
        // A missing or unreadable file just means the defaults stand.
        let saved = config_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join("config.json")).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let Some(saved) = saved {
            names.apply_saved(&saved);
        }
        names
    }
}

/// A sequencer URL the module will talk to: `https://`, or `http://` only to this
/// machine (a local dev chain). Plain http elsewhere would let anyone on the network
/// swap name records and payment keys.
fn is_http_url(url: &str) -> bool {
    if url.starts_with("https://") {
        return url.len() > "https://".len();
    }
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = if let Some(bracketed) = host.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// The one app that may use the methods that spend, sign, change the module's
/// settings or read the person's own state: the Names app.
const NAMES_APP: &str = "names_ui";

/// This module's package version, the one in `basecamp-module/metadata.json`.
/// `get_config` reports it so the Names app can refuse a module from another
/// release (the app's own metadata asks for at least this at install, but an
/// older module can still be the one loaded).
const MODULE_VERSION: &str = "0.13.0";

/// Whether `caller` may use those methods. Basecamp gives each app its own
/// credential, named after the app, so the module knows which app is calling.
/// Any other app that lists `names` as a dependency can still read names, but not
/// pay, sign, run the CLI or repoint the module. The host itself (Basecamp, or
/// `logosctl` driving the module) is allowed; a caller that can't be named is not.
fn may_act(caller: &logos_rust_sdk::LogosCaller) -> bool {
    matches!(caller, logos_rust_sdk::LogosCaller::HostAnchor) || caller.is_module(NAMES_APP)
}

#[cfg(not(test))]
fn calling() -> logos_rust_sdk::LogosCaller {
    logos_rust_sdk::current_caller()
}

// Tests run outside any dispatch; they act as the host unless a test says otherwise.
#[cfg(test)]
thread_local! {
    static TEST_CALLER: std::cell::RefCell<logos_rust_sdk::LogosCaller> =
        const { std::cell::RefCell::new(logos_rust_sdk::LogosCaller::HostAnchor) };
}

#[cfg(test)]
fn calling() -> logos_rust_sdk::LogosCaller {
    TEST_CALLER.with(|caller| caller.borrow().clone())
}

fn only_the_names_app() -> Result<(), String> {
    let caller = calling();
    if may_act(&caller) {
        Ok(())
    } else {
        Err(format!("only the Names app can do this; refused for {}", caller.describe_for_human()))
    }
}

fn reply(outcome: Result<Value, String>) -> String {
    match outcome {
        Ok(value) => json!({ "success": true, "value": value, "error": null }),
        Err(error) => json!({ "success": false, "value": null, "error": error }),
    }
    .to_string()
}

/// Adds the fields of `extra`, when it is an object, to `value`.
fn merged(mut value: Value, extra: &Value) -> Value {
    if let (Some(value), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
        for (key, field) in extra {
            value.insert(key.clone(), field.clone());
        }
    }
    value
}

/// Text entries from a JSON object of key to text; null counts as empty.
fn parse_entries(entries_json: &str) -> Result<Vec<(String, String)>, String> {
    let parsed: Value = serde_json::from_str(entries_json.trim())
        .map_err(|error| format!("the entries are not JSON: {error}"))?;
    let object = parsed
        .as_object()
        .ok_or("the entries must be a JSON object of key to text")?;
    object
        .iter()
        .map(|(key, value)| match value {
            Value::String(text) => Ok((key.trim().to_owned(), text.clone())),
            Value::Null => Ok((key.trim().to_owned(), String::new())),
            _ => Err(format!("the value of {key} must be text")),
        })
        .collect()
}

/// The names this device remembers, per registry. The chain keeps no list, so
/// without this a person could not be shown the names they hold.
struct Memory {
    path: std::path::PathBuf,
    registries: Map<String, Value>,
}

/// A remembered name that is not registered is dropped after this long. Until
/// then it may be a registration that has not reached a block.
const FORGET_UNREGISTERED_AFTER: u64 = 24 * 60 * 60;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn load_object(path: &std::path::Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|saved| saved.as_object().cloned())
        .unwrap_or_default()
}

/// Failing to save is not an error for the caller: the memory still serves
/// this run.
/// Makes `dir`, and the files and folders directly in it and in its `jobs`, readable
/// by this user only. Symlinks are left alone (and not followed).
fn tighten(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if !dir.is_dir() {
        return;
    }
    for folder in [dir.to_path_buf(), dir.join("jobs")] {
        let Ok(meta) = std::fs::symlink_metadata(&folder) else { continue };
        if !meta.is_dir() {
            continue;
        }
        let _ = std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700));
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            if let Ok(meta) = entry.path().symlink_metadata() {
                if meta.is_file() {
                    let _ = std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600));
                }
            }
        }
    }
}

fn save_object(path: &std::path::Path, object: &Map<String, Value>) {
    let _ = cli::write_private(path, Value::Object(object.clone()).to_string().as_bytes());
}

impl Memory {
    fn load(path: std::path::PathBuf) -> Self {
        let registries = load_object(&path);
        Self { path, registries }
    }

    /// The names remembered for `program`, each with when it was added.
    fn names(&self, program: &AccountId) -> Vec<(String, u64)> {
        self.registries
            .get(&registry::account_text(program))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| Some((entry["name"].as_str()?.to_owned(), entry["added"].as_u64()?)))
            .collect()
    }

    fn set(&mut self, program: &AccountId, names: &[(String, u64)]) {
        let entries: Vec<Value> = names
            .iter()
            .map(|(name, added)| json!({ "name": name, "added": added }))
            .collect();
        self.registries.insert(registry::account_text(program), Value::from(entries));
    }

    /// Adds `name` for `program` unless it is already there.
    fn remember(&mut self, program: &AccountId, name: &str) {
        let mut names = self.names(program);
        if !names.iter().any(|(held, _)| held == name) {
            names.push((name.to_owned(), now()));
            self.set(program, &names);
        }
    }

    fn save(&self) {
        save_object(&self.path, &self.registries);
    }
}

fn memory() -> Result<Memory, String> {
    config_dir()
        .map(|dir| Memory::load(dir.join("names.json")))
        .ok_or_else(|| "this device has nowhere to remember names".to_owned())
}

/// A commitment made and not yet revealed: what the reveal needs. The secret
/// is what binds the reveal to the commitment, so it stays on this device.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Commitment {
    owner: AccountId,
    secret: [u8; 32],
    /// The chain time the commitment claimed.
    made: u64,
    /// The `private_account` choice for the reveal.
    private_account: String,
    /// The reveal's transaction, once sent.
    revealed: Option<String>,
    /// The chain time the reveal claimed. Until it can no longer land, the
    /// reveal is not sent again.
    revealed_at: Option<u64>,
}

impl Commitment {
    fn to_json(&self) -> Value {
        json!({
            "owner": registry::account_text(&self.owner),
            "secret": registry::hex(&self.secret),
            "made": self.made,
            "private_account": self.private_account,
            "revealed": self.revealed,
            "revealed_at": self.revealed_at,
        })
    }

    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            owner: registry::parse_account(value["owner"].as_str()?).ok()?,
            secret: registry::unhex(value["secret"].as_str()?).ok()?.try_into().ok()?,
            made: value["made"].as_u64()?,
            private_account: value["private_account"].as_str().unwrap_or_default().to_owned(),
            revealed: value["revealed"].as_str().map(str::to_owned),
            revealed_at: value["revealed_at"].as_u64(),
        })
    }

    /// When the reveal may be sent, in chain time.
    fn ready_at(&self) -> u64 {
        self.made.saturating_add(registry::MIN_COMMIT_AGE_MS)
    }

    /// Whether a reveal claiming `chain_now` can still land before the
    /// commitment expires.
    fn usable_at(&self, chain_now: u64) -> bool {
        chain_now.saturating_add(registry::TIME_WINDOW_MS) < self.made.saturating_add(registry::MAX_COMMIT_AGE_MS)
    }
}

/// The commitments this device has under way, per registry and name.
struct Pending {
    path: std::path::PathBuf,
    registries: Map<String, Value>,
}

impl Pending {
    fn load(path: std::path::PathBuf) -> Self {
        let registries = load_object(&path);
        Self { path, registries }
    }

    fn all(&self, program: &AccountId) -> Vec<(String, Commitment)> {
        self.registries
            .get(&registry::account_text(program))
            .and_then(Value::as_object)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|(name, entry)| Some((name.clone(), Commitment::from_json(entry)?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get(&self, program: &AccountId, name: &str) -> Option<Commitment> {
        Commitment::from_json(self.registries.get(&registry::account_text(program))?.get(name)?)
    }

    fn put(&mut self, program: &AccountId, name: &str, pending: &Commitment) {
        let names = self
            .registries
            .entry(registry::account_text(program))
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(names) = names.as_object_mut() {
            names.insert(name.to_owned(), pending.to_json());
        }
    }

    fn remove(&mut self, program: &AccountId, name: &str) {
        if let Some(names) = self.registries.get_mut(&registry::account_text(program)).and_then(Value::as_object_mut) {
            names.remove(name);
        }
    }

    fn save(&self) {
        save_object(&self.path, &self.registries);
    }
}

fn pending() -> Result<Pending, String> {
    config_dir()
        .map(|dir| Pending::load(dir.join("pending.json")))
        .ok_or_else(|| "this device has nowhere to keep a commitment".to_owned())
}

impl Names {
    /// Applies a saved configuration. A registry that this version can't read
    /// is ignored, so the default stands.
    fn apply_saved(&mut self, saved: &Value) {
        if let Some(url) = saved["sequencer"].as_str().filter(|url| is_http_url(url)) {
            self.sequencer = url.to_owned();
        }
        if let Some(program) = saved["registry"].as_str().filter(|program| !RETIRED_REGISTRIES.contains(program)) {
            self.program = registry::parse_account(program).ok().or(self.program);
        }
        // Paths are checked as set_* checks them, so one saved before those checks
        // existed (or written into the file by hand) can't point the module elsewhere.
        let own = |path: &str, dir: bool, root_ok: bool| cli::is_owned_safely(std::path::Path::new(path), dir, root_ok);
        if let (Some(config), Some(storage)) = (saved["wallet_config"].as_str(), saved["wallet_storage"].as_str()) {
            if own(config, false, false) && own(storage, false, false) {
                self.wallet_files = Some((config.to_owned(), storage.to_owned()));
            }
        }
        self.cli = saved["cli"]
            .as_str()
            .filter(|path| !path.is_empty() && cli::is_executable(std::path::Path::new(path)) && own(path, false, true))
            .map(str::to_owned);
        self.cli_wallet = saved["cli_wallet"].as_str().filter(|dir| !dir.is_empty() && own(dir, true, false)).map(str::to_owned);
        self.follows = saved["follows"]
            .as_array()
            .map(|list| list.iter().filter_map(|name| registry::parse_name(name.as_str()?).ok()).collect())
            .unwrap_or_default();
        self.main_name = saved["main_name"].as_str().and_then(|name| registry::parse_name(name).ok());
        // Checked as typed ones are, so an edited file can't point the module elsewhere.
        self.bridge_key = saved["bridge_key"].as_str().and_then(|key| cli::bridge_key(key).ok());
        self.bridge_node = saved["bridge_node"].as_str().filter(|node| !node.trim().is_empty()).and_then(|node| cli::bridge_node(node).ok());
        self.dm_names = saved["dm_names"]
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(convo, name)| Some((convo.clone(), registry::parse_name(name.as_str()?).ok()?)))
                    .collect()
            })
            .unwrap_or_default();
        self.chat_names = saved["chat_names"]
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(address, name)| Some((address.clone(), registry::parse_name(name.as_str()?).ok()?)))
                    .collect()
            })
            .unwrap_or_default();
        self.likes = saved["likes"]
            .as_array()
            .map(|list| list.iter().filter_map(|post| post.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        self.token_program = saved["token_program"]
            .as_str()
            .and_then(|program| registry::parse_account(program).ok())
            .filter(|program| *program != registry::NATIVE_TOKEN);
    }

    fn config(&self) -> Value {
        json!({
            "sequencer": self.sequencer,
            "registry": self.program.as_ref().map(registry::account_text),
            "wallet_config": self.wallet_files.as_ref().map(|(config, _)| config),
            "wallet_storage": self.wallet_files.as_ref().map(|(_, storage)| storage),
            "cli": self.cli,
            "cli_wallet": self.cli_wallet,
            "follows": self.follows,
            "main_name": self.main_name,
            "bridge_key": self.bridge_key,
            "bridge_node": self.bridge_node,
            "likes": self.likes,
            "dm_names": self.dm_names,
            "chat_names": self.chat_names,
            "token_program": self.token_program.as_ref().map(registry::account_text),
        })
    }

    /// The configuration as `get_config` shows it: the CLI and its wallet
    /// directory as they will be used, defaults filled in (`cli` is null if
    /// no CLI is found).
    fn shown_config(&self) -> Value {
        let mut config = self.config();
        config["cli"] = json!(cli::find_program(self.cli.as_deref()).map(|path| path.to_string_lossy().into_owned()));
        config["cli_wallet"] = json!(cli::find_wallet(self.cli_wallet.as_deref()).map(|dir| dir.to_string_lossy().into_owned()));
        config["module_version"] = json!(MODULE_VERSION);
        config
    }

    /// The CLI to run for this module's registry and sequencer.
    /// Answers a board read at once with what it last returned, `{posts,
    /// updating, error}`, and starts fetching a newer one on its own thread
    /// unless one is already on the way.
    fn board_read_now(&self, key: &str) -> Value {
        let reads = self.board_reads.lock().expect("the board reads lock");
        let read = reads.get(key);
        json!({
            "posts": read.and_then(|read| read.value.clone()).unwrap_or_else(|| json!([])),
            "updating": read.is_some_and(|read| read.updating),
            "error": read.and_then(|read| read.error.clone()),
        })
    }

    fn resolve_now(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let Some(resolved) = registry.resolve(name, derived)? else {
                return Ok(Value::Null);
            };
            let mut value = resolved.to_json();
            value["deposit"] = Value::from(registry.deposit(&derived.id)?.to_string());
            Ok(value)
        })
    }

    fn page_now(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            Ok(registry.page(name, derived)?.unwrap_or(Value::Null))
        })
    }

    fn page_view_now(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            Ok(registry.page(name, derived)?.map_or(Value::Null, |page| page_view::view(&page)))
        })
    }

    fn avatar_now(&mut self, name: String) -> String {
        self.about(&name, |registry, name, derived| {
            let resolved = registry.resolve(name, derived)?;
            let entry = resolved.as_ref().and_then(|resolved| {
                resolved.profile.texts.iter().find(|(key, _)| key == "avatar").map(|(_, value)| value.as_str())
            });
            Ok(entry.and_then(avatar::pixels).unwrap_or(Value::Null))
        })
    }

    /// Drops every answer read from the chain, after the chain or registry changed.
    fn forget_all_reads(&mut self) {
        self.read_cache.clear();
        self.board_reads.lock().expect("the board reads lock").clear();
    }

    /// Answers a read about `typed` from the cache while it is fresh, else asks
    /// `read` and keeps a successful answer.
    fn cached_read(&mut self, kind: &str, typed: &str, read: impl FnOnce(&mut Self) -> String) -> String {
        let name = registry::normalize_name(typed);
        let key = (kind.to_owned(), name);
        if let Some((answer, at)) = self.read_cache.get(&key) {
            if at.elapsed() < READ_FRESH {
                return answer.clone();
            }
        }
        let answer = read(self);
        if serde_json::from_str::<Value>(&answer).is_ok_and(|reply| reply["success"] == true) {
            if self.read_cache.len() >= MAX_CACHED {
                self.read_cache.retain(|_, (_, at)| at.elapsed() < READ_FRESH);
                if self.read_cache.len() >= MAX_CACHED {
                    self.read_cache.clear();
                }
            }
            self.read_cache.insert(key, (answer.clone(), std::time::Instant::now()));
        }
        answer
    }

    fn mine_known(&self, key: &str) -> bool {
        self.board_reads.lock().expect("the board reads lock").get(key).is_some_and(|read| read.value.is_some())
    }

    fn remember_mine(&self, key: &str, list: Value) {
        let mut reads = self.board_reads.lock().expect("the board reads lock");
        reads.entry(key.to_owned()).or_default().value = Some(list);
    }

    /// The CLI's names changed (a registration or transfer ended): the next
    /// `cli_mine` waits for a fresh list.
    fn forget_mine(&self) {
        self.board_reads.lock().expect("the board reads lock").retain(|key, _| !key.starts_with("cli_mine:"));
    }

    /// Drops what the cache knows about `typed`, after a change to it.
    fn forget_reads(&mut self, typed: &str) {
        let name = registry::normalize_name(typed);
        self.read_cache.retain(|(_, cached), _| *cached != name);
    }

    fn board_read(&self, key: String, fetch: impl FnOnce() -> Result<Value, String> + Send + 'static) -> Value {
        {
            let mut reads = self.board_reads.lock().expect("the board reads lock");
            let read = reads.entry(key.clone()).or_default();
            if !read.updating {
                read.updating = true;
                let reads = std::sync::Arc::clone(&self.board_reads);
                let key = key.clone();
                std::thread::spawn(move || {
                    let fetched = fetch();
                    let mut reads = reads.lock().expect("the board reads lock");
                    let read = reads.entry(key).or_default();
                    read.updating = false;
                    match fetched {
                        Ok(value) => {
                            read.value = Some(value);
                            read.error = None;
                        }
                        Err(error) => read.error = Some(error),
                    }
                });
            }
        }
        self.board_read_now(&key)
    }

    fn cli_setup(&self) -> Result<cli::Setup, String> {
        let program = cli::find_program(self.cli.as_deref()).ok_or(
            "no names CLI found at ~/.local/bin/names or on PATH; install it, or give its path in App settings",
        )?;
        let wallet = cli::find_wallet(self.cli_wallet.as_deref()).ok_or("no wallet directory for the names CLI")?;
        Ok(cli::Setup {
            program,
            wallet,
            registry: self.program.as_ref().map(registry::account_text),
            sequencer: self.sequencer.clone(),
        })
    }

    fn jobs(&self) -> Result<cli::Jobs, String> {
        self.jobs_dir
            .clone()
            .or_else(|| config_dir().map(|dir| dir.join("jobs")))
            .map(|dir| cli::Jobs { dir })
            .ok_or_else(|| "this device has nowhere to keep a job's log".to_owned())
    }

    /// Where `cli_install` puts the CLI and its man pages.
    fn install_dirs(&self) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
        let bin = self.bin_dir.clone().or_else(install_dir).ok_or("this device has nowhere to install the names program")?;
        let man = bin.parent().map_or_else(|| bin.join("man"), |parent| parent.join("man"));
        Ok((bin, man))
    }

    /// Uses the CLI `cli_install` put at `program` from now on, with the CLI's
    /// default wallet folder unless one was chosen.
    fn adopt_cli(&mut self, program: &std::path::Path) {
        let path = program.to_string_lossy().into_owned();
        let wallet = self.cli_wallet.clone().or_else(|| cli::find_wallet(None).map(|dir| dir.to_string_lossy().into_owned()));
        if self.cli.as_deref() != Some(path.as_str()) || self.cli_wallet != wallet {
            self.cli = Some(path);
            self.cli_wallet = wallet;
            self.save();
        }
        self.forget_mine();
    }

    /// The running job called `name`, as `job` shows it, or null.
    fn running_job(&self, name: &str) -> Value {
        self.jobs()
            .ok()
            .and_then(|jobs| jobs.running_for(name).and_then(|id| jobs.state(&id).ok()))
            .unwrap_or(Value::Null)
    }

    /// Starts a wallet job (`cli_shield`, `cli_pay_account`).
    fn wallet_job(&self, kind: &str, build: impl FnOnce(&cli::Setup) -> Result<Vec<String>, String>) -> Result<Value, String> {
        let setup = self.cli_setup()?;
        setup.wallet_ready()?;
        let command = build(&setup)?;
        let jobs = self.jobs()?;
        let id = jobs.prepare(cli::WALLET_JOB)?;
        jobs.start(&setup, &id, cli::WALLET_JOB, kind, command, None)
    }

    /// Refuses while any CLI job runs: each one saves the wallet and the CLI's
    /// settings when it ends, from the copy it read when it started, which
    /// would undo an account change made meanwhile.
    fn no_job_running(&self) -> Result<(), String> {
        match self.jobs()?.running().first() {
            Some(job) => Err(format!(
                "wait for \"{}\" to finish first (job {}): it saves the wallet when it ends",
                job["kind"].as_str().unwrap_or("a change"),
                job["id"].as_str().unwrap_or_default()
            )),
            None => Ok(()),
        }
    }

    /// Adds what a finished install or wallet job means to its `job` state:
    /// an install that worked is put to use (`version`, `path`, `warning`),
    /// one that didn't says why (`problem`); a move to private says whether
    /// anything `moved`; a failed payment says why.
    fn job_outcome(&mut self, state: &mut Value) {
        let running = state["state"] == "running";
        let done = state["state"] == "done";
        let log = state["log_tail"].as_str().unwrap_or_default().to_owned();
        match state["kind"].as_str() {
            Some("install") if !running => {
                let path = self.install_dirs().ok().map(|(bin, _)| bin.join("names"));
                let version = match (&path, done) {
                    (Some(path), true) => cli::version(path),
                    _ => Err(String::new()),
                };
                match (path, version) {
                    (Some(path), Ok(version)) => {
                        self.adopt_cli(&path);
                        state["path"] = json!(path);
                        state["version"] = Value::from(version);
                        state["prover"] = Value::from(cli::has_prover(&path));
                        state["warning"] = cli::install_warning(&log).map_or(Value::Null, Value::from);
                    }
                    (_, version) => {
                        let why = version.err().unwrap_or_default();
                        let problem = cli::install_problem(&log);
                        state["state"] = Value::from("failed");
                        state["problem"] = Value::from(if done && problem.starts_with("The install stopped") && !why.is_empty() {
                            format!("It installed, but it doesn't start: {why}")
                        } else {
                            problem
                        });
                    }
                }
            }
            Some("shield") if !running => {
                state["moved"] = Value::from(false);
                if let Some(fields) = cli::shield_outcome(&log, done).as_object() {
                    for (key, value) in fields {
                        state[key.as_str()] = value.clone();
                    }
                }
            }
            Some("pay_account") if !running && !done => {
                let first = log.lines().map(str::trim).find(|line| line.starts_with("Error:")).unwrap_or_else(|| cli::last_line(&log));
                state["problem"] = Value::from(format!("The payment didn't go through: {}", first.trim_start_matches("Error:").trim()));
            }
            Some("token_send" | "token_receive") if !running => {
                let what = if state["kind"] == "token_send" { "The token transfer didn't go through" } else { "Receiving didn't finish" };
                if let Some(fields) = cli::token_outcome(&log).as_object() {
                    for (key, value) in fields {
                        state[key.as_str()] = value.clone();
                    }
                }
                if !done {
                    let first = log.lines().map(str::trim).find(|line| line.starts_with("Error:")).unwrap_or_else(|| cli::last_line(&log));
                    state["problem"] = Value::from(format!("{what}: {}", first.trim_start_matches("Error:").trim()));
                }
                // What the wallet holds has changed.
                self.forget_tokens();
            }
            _ => {}
        }
    }

    /// Starts a CLI job for the name a person typed. `build` makes the
    /// command from the bare name and the job's id.
    fn cli_job(
        &self,
        typed: &str,
        kind: &str,
        build: impl FnOnce(&cli::Jobs, &str, &str) -> Result<(Vec<String>, Option<std::path::PathBuf>, Value), String>,
    ) -> String {
        reply((|| {
            let name = registry::parse_name(typed)?;
            let setup = self.cli_setup()?;
            setup.wallet_ready()?;
            let jobs = self.jobs()?;
            let id = jobs.prepare(&name)?;
            let (command, scratch, extra) = build(&jobs, &name, &id)?;
            let started = jobs.start(&setup, &id, &name, kind, command, scratch)?;
            Ok(merged(started, &extra))
        })())
    }

    /// The wallet's accounts.
    fn held(&self) -> Result<Held, String> {
        wallet::accounts(&self.cli_setup()?)
    }

    /// Writes the configuration for the next run. Failing to save is not an
    /// error for the caller: the setting still applies to this run.
    fn save(&self) {
        if let Some(dir) = config_dir() {
            let _ = cli::write_private(&dir.join("config.json"), self.config().to_string().as_bytes());
        }
    }

    fn registry(&self) -> Result<Registry, String> {
        let program = self
            .program
            .ok_or("no registry is configured; call set_registry with its program ID")?;
        Ok(Registry {
            sequencer: self.sequencer.clone(),
            program,
        })
    }

    /// The identifier and key of `name`, derived the first time it is asked for.
    fn derived(&mut self, name: &str) -> Derived {
        // Bounded: an app looking up endless names can't grow this without limit.
        if self.derived.len() >= MAX_CACHED && !self.derived.contains_key(name) {
            self.derived.clear();
        }
        self.derived
            .entry(name.to_owned())
            .or_insert_with(|| registry::derive(name))
            .clone()
    }

    /// Where a Logos Blockchain send to `to` goes: `{key, name, forwarded_from}`, a key
    /// as given, or the `blockchain` entry of a name (one `forward` hop, SPEC.md §7).
    fn chain_target(&mut self, to: &str) -> Result<Value, String> {
        if let Some(key) = chain::parse_key(to) {
            return Ok(json!({ "key": key, "name": null, "forwarded_from": null }));
        }
        let name = registry::parse_name(to).map_err(|_| "enter a Logos Blockchain key (64 hex characters) or a name".to_owned())?;
        let registry = self.registry()?;
        let derived = self.derived(&name);
        let resolved = registry.resolve(&name, &derived)?.ok_or_else(|| format!("{name} isn't registered"))?;
        let (resolved, shown, forwarded_from) = match resolved.forward_target() {
            Some(target) => {
                let derived = self.derived(&target);
                let forwarded = registry
                    .resolve(&target, &derived)?
                    .ok_or_else(|| format!("{name} points to {target}, which isn't registered"))?;
                (forwarded, target, Some(name))
            }
            None => (resolved, name, None),
        };
        let key = resolved
            .profile
            .texts
            .iter()
            .find(|(key, _)| key == chain::BLOCKCHAIN_KEY)
            .and_then(|(_, value)| chain::parse_key(value))
            .ok_or_else(|| format!("{shown} hasn't published a Logos Blockchain key"))?;
        Ok(json!({ "key": key, "name": shown, "forwarded_from": forwarded_from }))
    }

    /// Answers a question about the name a person typed.
    fn about(&mut self, typed: &str, answer: impl FnOnce(&Registry, &str, &Derived) -> Result<Value, String>) -> String {
        reply((|| {
            let name = registry::parse_name(typed)?;
            let registry = self.registry()?;
            let derived = self.derived(&name);
            answer(&registry, &name, &derived)
        })())
    }

    /// Checks that the open wallet can sign and pay for `change`, has it sign
    /// and send it, and returns `{name, signer, tx}` with the change's extras.
    fn send(&self, registry: &Registry, held: &Held, name: &str, change: &Change) -> Result<Value, String> {
        if !held.public.contains(&change.signer) {
            return Err(format!(
                "the open wallet does not hold {} as a public account, which must sign this",
                registry::account_text(&change.signer)
            ));
        }
        let balance = registry.balance(&change.signer)?;
        if balance <= change.needs {
            return Err(if change.needs == 0 {
                format!("{} has no funds to pay the fee", registry::account_text(&change.signer))
            } else {
                format!(
                    "{} holds {balance}, not enough for the {} it pays plus the fee",
                    registry::account_text(&change.signer),
                    change.needs
                )
            });
        }
        let tx = wallet::send(&self.cli_setup()?, change)?;
        Ok(merged(
            json!({
                "name": name,
                "signer": registry::account_text(&change.signer),
                "tx": tx,
            }),
            &change.extra,
        ))
    }

    /// Has the wallet sign and send the change `build` makes from the open
    /// wallet's public accounts. The signer must be one of them; it also pays.
    fn change(&self, name: &str, build: impl FnOnce(&Registry, &[AccountId]) -> Result<Change, String>) -> String {
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?;
            let change = build(&registry, &held.public)?;
            self.send(&registry, &held, name, &change)
        })())
    }

    /// The same, for a change to a name a person typed.
    fn change_name(
        &mut self,
        typed: &str,
        build: impl FnOnce(&Registry, &str, &Derived, &[AccountId]) -> Result<Change, String>,
    ) -> String {
        self.forget_reads(typed);
        match registry::parse_name(typed) {
            Ok(name) => {
                let derived = self.derived(&name);
                self.change(&name, |registry, held| build(registry, &name, &derived, held))
            }
            Err(error) => reply(Err(error)),
        }
    }

    /// One step of registering `name` for `owner`: commit, wait, reveal, or
    /// report that it is done. See [`NamesModule::register`].
    fn register_step(&mut self, typed: &str, owner: &str, private_account: &str) -> Result<Value, String> {
        let name = registry::parse_name(typed)?;
        let registry = self.registry()?;
        let held = self.held()?;
        let owner = registry::parse_account(owner)?;
        if !held.public.contains(&owner) {
            return Err(format!(
                "the open wallet does not hold {} as a public account; this module registers names only to public accounts",
                registry::account_text(&owner)
            ));
        }
        let derived = self.derived(&name);
        let mut commitments = pending()?;
        let saved = commitments.get(&registry.program, &name).filter(|saved| saved.owner == owner);
        let status = |stage: &str, tx: Option<&str>, ready_at: Option<u64>| {
            json!({
                "name": name,
                "signer": registry::account_text(&owner),
                "tx": tx,
                "stage": stage,
                "ready_at": ready_at,
            })
        };

        if let Some(record) = registry.resolve(&name, &derived)? {
            if !record.is_held_by(&derived.id, &owner) {
                return Err(format!("{name}{} is already registered", registry::NAME_SUFFIX));
            }
            // An earlier reveal has landed.
            if saved.is_some() {
                commitments.remove(&registry.program, &name);
                commitments.save();
            }
            self.remember(&registry.program, &name);
            return Ok(status("registered", None, None));
        }

        let chain_now = registry.now()?;
        if let Some(mut saved) = saved.filter(|saved| saved.usable_at(chain_now)) {
            let promised = registry::commitment(&derived.id, &registry::owner_tag(&derived.id, &owner), &saved.secret);
            let landed = registry.commitment_made(&promised)?.is_some();
            if !landed && chain_now < saved.made.saturating_add(registry::TIME_WINDOW_MS) {
                // Sent, and still able to land.
                return Ok(status("committed", None, Some(saved.ready_at())));
            }
            if landed {
                if chain_now < saved.ready_at() {
                    return Ok(status("committed", None, Some(saved.ready_at())));
                }
                if saved.revealed_at.is_some_and(|at| chain_now < at.saturating_add(registry::TIME_WINDOW_MS)) {
                    // A reveal is on its way and may still land.
                    return Ok(status("revealed", saved.revealed.as_deref(), Some(saved.ready_at())));
                }
                let choice = if private_account.trim().is_empty() { saved.private_account.clone() } else { private_account.to_owned() };
                let keys = if choice.trim().is_empty() { None } else { Some(wallet::private_keys(&self.cli_setup()?, &choice)?) };
                let change = registry.reveal(&name, &derived, &owner, &saved.secret, keys)?;
                let sent = self.send(&registry, &held, &name, &change)?;
                saved.revealed = sent["tx"].as_str().map(str::to_owned);
                saved.revealed_at = Some(chain_now);
                commitments.put(&registry.program, &name, &saved);
                commitments.save();
                self.remember(&registry.program, &name);
                return Ok(merged(sent, &json!({ "stage": "revealed", "ready_at": saved.ready_at() })));
            }
            // The commitment never landed: make a new one.
        }

        let secret: [u8; 32] = registry::fresh_bytes()?;
        let (change, made) = registry.commit(&name, &derived, &owner, &secret)?;
        let sent = self.send(&registry, &held, &name, &change)?;
        let saved = Commitment {
            owner,
            secret,
            made,
            private_account: private_account.trim().to_owned(),
            revealed: None,
            revealed_at: None,
        };
        commitments.put(&registry.program, &name, &saved);
        commitments.save();
        self.remember(&registry.program, &name);
        Ok(merged(sent, &json!({ "stage": "committed", "ready_at": saved.ready_at() })))
    }

    /// Remembers `name` for `program`, for `mine`.
    fn remember(&self, program: &AccountId, name: &str) {
        if let Ok(mut memory) = memory() {
            memory.remember(program, name);
            memory.save();
        }
    }
}

/// What registering a name of `tier` takes under `settings`, for `deposit`.
fn deposit_json(name: &str, tier: u8, settings: Option<&Settings>, now: u64) -> Value {
    let mut value = json!({
        "name": name,
        "display": format!("{name}{}", registry::NAME_SUFFIX),
        "tier": changes::tier_text(tier),
        "mode": Settings::mode_text(settings),
        "claims_until": settings.filter(|settings| settings.claims_open(now)).map(|settings| settings.claims_until),
    });
    let deposit = match settings {
        _ if tier == shortlist::TIER_LOCKED => Value::Null,
        Some(settings) if settings.deposit_mode() => {
            Value::from(settings.deposits[usize::from(tier == shortlist::TIER_THREE)].to_string())
        }
        _ => Value::from("0"),
    };
    value["deposit"] = deposit;
    if let Some(settings) = settings.filter(|settings| !settings.deposit_mode()) {
        value["fee"] = Value::from(settings.prices[usize::from(changes::REGISTER_TERM)].to_string());
    }
    value
}

fn settings_json(settings: &Settings, now: u64) -> Value {
    json!({
        "mode": Settings::mode_text(Some(settings)),
        "deposits": {
            "normal": settings.deposits[0].to_string(),
            "three": settings.deposits[1].to_string(),
        },
        "admin": registry::account_text(&settings.admin),
        "treasury": registry::account_text(&settings.treasury),
        "prices": {
            "30d": settings.prices[0].to_string(),
            "1y": settings.prices[1].to_string(),
            "5y": settings.prices[2].to_string(),
        },
        "claims_until": settings.claims_open(now).then_some(settings.claims_until),
    })
}

impl NamesModule for Names {
    fn recipient(&mut self, name: String) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            let registry = self.registry()?;
            let derived = self.derived(&name);
            let Some(resolved) = registry.resolve(&name, &derived)? else {
                return Ok(Value::Null);
            };
            // A name that forwards is paid where it points, one hop (SPEC.md §7).
            let Some(target) = resolved.forward_target() else {
                return Ok(resolved.recipient_json());
            };
            let derived = self.derived(&target);
            let forwarded = registry
                .resolve(&target, &derived)?
                .ok_or_else(|| format!("{name} points to {target}, which isn't registered"))?;
            let mut value = forwarded.recipient_json();
            value["forwarded_from"] = Value::from(name);
            Ok(value)
        })())
    }

    fn resolve(&mut self, name: String) -> String {
        self.cached_read("resolve", &name.clone(), |this| this.resolve_now(name))
    }

    fn page(&mut self, name: String) -> String {
        self.cached_read("page", &name.clone(), |this| this.page_now(name))
    }

    fn page_view(&mut self, name: String) -> String {
        self.cached_read("page_view", &name.clone(), |this| this.page_view_now(name))
    }

    fn avatar(&mut self, name: String) -> String {
        self.cached_read("avatar", &name.clone(), |this| this.avatar_now(name))
    }

    fn page_preview(&mut self, body: String) -> String {
        let content_type = if body.trim_start().starts_with('<') { "text/html; charset=utf-8" } else { "text/plain; charset=utf-8" };
        reply(Ok(page_view::view(&json!({ "content_type": content_type, "bytes": body.len(), "text": body }))))
    }

    fn reverse(&mut self, account: String) -> String {
        reply((|| {
            let account = registry::parse_account(&account)?;
            let name = self.registry()?.reverse(&account)?;
            Ok(name.map_or(Value::Null, |name| {
                json!({ "name": name, "display": format!("{name}{}", registry::NAME_SUFFIX) })
            }))
        })())
    }

    fn normalize(&mut self, input: String) -> String {
        registry::parse_name(&input).unwrap_or_default()
    }

    fn wallet(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?;
            // Each summary is a read over the network; they are read together.
            let mut accounts = std::thread::scope(|scope| {
                let readers: Vec<_> = held
                    .public
                    .iter()
                    .take(MAX_WALLET_ACCOUNTS)
                    .map(|account| scope.spawn(|| registry.account_summary(account)))
                    .collect();
                readers
                    .into_iter()
                    .map(|reader| reader.join().unwrap_or_else(|_| Err("a read failed".to_owned())))
                    .collect::<Result<Vec<Value>, String>>()
            })?;
            // A stable sort: funded accounts first, each group in wallet order.
            accounts.sort_by_key(|account| account["funded"] != true);
            // A private account's balance is known only to the wallet.
            let mut private = Vec::new();
            for (account, balance) in held.private.iter().take(MAX_WALLET_ACCOUNTS) {
                private.push(json!({
                    "account": registry::account_text(account),
                    "balance": balance.to_string(),
                    "funded": *balance > 0,
                }));
            }
            private.sort_by_key(|account| account["funded"] != true);
            Ok(json!({ "accounts": accounts, "private_accounts": private }))
        })())
    }

    fn mine(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let registry = self.registry()?;
            let held = self.held()?.public;
            let mut memory = memory()?;
            let remembered = memory.names(&registry.program);
            // Deriving is done one name at a time (it is CPU work and cached);
            // the reads that follow go out together.
            let derived: Vec<Derived> = remembered.iter().map(|(name, _)| self.derived(name)).collect();
            let reader = &registry;
            let resolved = std::thread::scope(|scope| {
                let readers: Vec<_> = remembered
                    .iter()
                    .zip(&derived)
                    .map(|((name, _), derived)| scope.spawn(move || reader.resolve(name, derived)))
                    .collect();
                readers
                    .into_iter()
                    .map(|reader| reader.join().unwrap_or_else(|_| Err("a read failed".to_owned())))
                    .collect::<Result<Vec<_>, String>>()
            })?;
            let mut commitments = pending()?;
            let mut names = Vec::new();
            let mut kept = Vec::new();
            let mut landed = Vec::new();
            for (((name, added), found), derived) in remembered.iter().cloned().zip(resolved).zip(&derived) {
                match found {
                    Some(resolved) => {
                        let holder = held.iter().find(|account| resolved.is_held_by(&derived.id, account));
                        if let Some(holder) = holder {
                            names.push(json!({
                                "name": name,
                                "display": format!("{name}{}", registry::NAME_SUFFIX),
                                "owner": registry::account_text(holder),
                                "expires": resolved.expires_json(),
                                "never": resolved.never_expires(),
                                "lapsed": resolved.lapsed(registry::local_now()),
                            }));
                            landed.push(name.clone());
                        }
                        // A name another wallet holds is kept: it may be this
                        // person's other wallet.
                        kept.push((name, added));
                    }
                    None if now().saturating_sub(added) < FORGET_UNREGISTERED_AFTER => kept.push((name, added)),
                    // A registration still under way is kept too.
                    None if commitments.get(&registry.program, &name).is_some() => kept.push((name, added)),
                    None => {}
                }
            }
            if kept.len() != remembered.len() {
                memory.set(&registry.program, &kept);
                memory.save();
            }
            // Registrations under way: a landed one is done, and an expired
            // commitment is dropped.
            let local = registry::local_now();
            let mut under_way = Vec::new();
            let mut changed = false;
            for (name, saved) in commitments.all(&registry.program) {
                if landed.contains(&name) || !saved.usable_at(local) {
                    commitments.remove(&registry.program, &name);
                    changed = true;
                    continue;
                }
                under_way.push(json!({
                    "name": name,
                    "display": format!("{name}{}", registry::NAME_SUFFIX),
                    "owner": registry::account_text(&saved.owner),
                    "stage": if saved.revealed.is_some() { "revealed" } else { "committed" },
                    "ready_at": saved.ready_at(),
                    "tx": saved.revealed,
                }));
            }
            if changed {
                commitments.save();
            }
            Ok(json!({ "names": names, "pending": under_way }))
        })())
    }

    fn keep(&mut self, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let name = registry::parse_name(&name)?;
            let registry = self.registry()?;
            // Only names the open wallet holds are remembered, so the device
            // keeps no list of names merely looked up.
            let derived = self.derived(&name);
            let held = self.held()?.public;
            let found = registry.resolve(&name, &derived)?;
            if !found.is_some_and(|resolved| held.iter().any(|account| resolved.is_held_by(&derived.id, account))) {
                return Err(format!("{name}{} is not held by the open wallet", registry::NAME_SUFFIX));
            }
            let mut memory = memory()?;
            memory.remember(&registry.program, &name);
            memory.save();
            Ok(Value::from(name))
        })())
    }

    fn deposit(&mut self, name: String) -> String {
        // Showing what a name takes needs no proof, so no download of the locked list.
        self.about(&name, |registry, name, _derived| {
            let tier = shortlist::tier_of_name(name);
            let settings = registry.settings()?;
            let now = registry.now()?;
            Ok(deposit_json(name, tier, settings.as_ref(), now))
        })
    }

    fn settings(&mut self) -> String {
        reply((|| {
            let registry = self.registry()?;
            let settings = registry.settings()?;
            let now = registry.now()?;
            Ok(settings.as_ref().map_or(Value::Null, |settings| settings_json(settings, now)))
        })())
    }

    fn register_name(&mut self, name: String, owner: String, private_account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        // The proof a registration carries comes from the names CLI, which holds the list.
        shortlist::use_cli(self.cli_setup().ok());
        reply(self.register_step(&name, &owner, &private_account))
    }

    fn receive_privately(&mut self, name: String, private_account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let setup = self.cli_setup();
        self.change_name(&name, |registry, name, derived, held| {
            let (npk, vpk) = wallet::private_keys(&setup?, &private_account)?;
            registry.set_private_keys(name, derived, held, &npk, &vpk, true)
        })
    }

    fn set_account(&mut self, name: String, account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_account(name, derived, held, &registry::parse_account(&account)?)
        })
    }

    fn set_private_keys(&mut self, name: String, nullifier_public_key: String, viewing_public_key: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| {
            let npk = registry::unhex(&nullifier_public_key)?;
            let vpk = registry::unhex(&viewing_public_key)?;
            registry.set_private_keys(name, derived, held, &npk, &vpk, false)
        })
    }

    fn clear_addresses(&mut self, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| registry.clear_addresses(name, derived, held))
    }

    fn set_text(&mut self, name: String, key: String, value: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_text(name, derived, held, key.trim(), &value)
        })
    }

    fn set_profile(&mut self, name: String, entries_json: String, replace: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        // The entries are checked before anything is read from the chain.
        let entries = match parse_entries(&entries_json) {
            Ok(entries) => entries,
            Err(error) => return reply(Err(error)),
        };
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_profile(name, derived, held, entries, replace)
        })
    }

    fn set_page(&mut self, name: String, content_type: String, body: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| {
            registry.set_page(name, derived, held, content_type.trim(), body.as_bytes())
        })
    }

    fn transfer(&mut self, name: String, new_owner: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| {
            registry.transfer(name, derived, held, &registry::parse_account(&new_owner)?)
        })
    }

    fn release(&mut self, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.change_name(&name, |registry, name, derived, held| registry.release(name, derived, held))
    }

    fn set_primary(&mut self, account: String, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let name = if name.trim().is_empty() {
            String::new()
        } else {
            match registry::parse_name(&name) {
                Ok(name) => name,
                Err(error) => return reply(Err(error)),
            }
        };
        let derived = (!name.is_empty()).then(|| self.derived(&name));
        self.change(&name, |registry, _| {
            registry.set_primary(&registry::parse_account(&account)?, &name, derived.as_ref())
        })
    }

    fn get_config(&mut self) -> String {
        self.shown_config().to_string()
    }

    fn cli_claim(&mut self, name: String, snapshot: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let snapshot = snapshot.trim().to_owned();
        self.cli_job(&name, "claim", |jobs, name, id| {
            if std::path::Path::new(&snapshot).is_absolute() {
                return Ok((cli::claim_args(name, std::path::Path::new(&snapshot))?, None, Value::Null));
            }
            let url = if snapshot.is_empty() { CLAIMS_SNAPSHOT } else { snapshot.as_str() };
            if !is_http_url(url) {
                return Err("give the snapshot as a full path or a URL".to_owned());
            }
            let file = jobs.scratch(id, "snapshot");
            cli::write_private(&file, &cli::fetch_snapshot(url)?).map_err(|error| format!("cannot keep the list of names: {error}"))?;
            Ok((cli::claim_args(name, &file)?, Some(file), Value::Null))
        })
    }

    fn cli_restore_wallet(&mut self, phrase: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            let restored = cli::run_restore_wallet(&setup, &phrase)?;
            self.forget_mine();
            Ok(restored)
        })())
    }

    fn cli_mine(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            // The CLI reads every name it holds from the chain, which takes seconds:
            // answer with the last list and refresh it on its own thread.
            // Kept per wallet: another wallet holds other names.
            let key = format!("cli_mine:{}", setup.wallet.display());
            if self.mine_known(&key) {
                let cached = self.board_read(key, move || cli::run_mine(&setup).map(Value::from));
                return Ok(cached["posts"].clone());
            }
            let fresh = Value::from(cli::run_mine(&setup)?);
            self.remember_mine(&key, fresh.clone());
            Ok(fresh)
        })())
    }

    fn cli_profile(&mut self, name: String, entries_json: String, replace: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let entries = match parse_entries(&entries_json) {
            Ok(entries) => entries,
            Err(error) => return reply(Err(error)),
        };
        // Checked before anything runs, so a bad entry fails at once.
        let made = match cli::profile_args(&name, &entries, replace) {
            Ok(made) => made,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "profile", |_, _, _| Ok((made.args, None, json!({ "skipped": made.skipped }))))
    }

    fn cli_set_page(&mut self, name: String, content_type: String, body: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        if let Err(error) = cli::set_page_args(&name, &content_type, Some(std::path::Path::new("check"))) {
            return reply(Err(error));
        }
        self.cli_job(&name, "set_page", |jobs, name, id| {
            if body.is_empty() {
                return Ok((cli::set_page_args(name, &content_type, None)?, None, Value::Null));
            }
            let file = jobs.scratch(id, "page");
            cli::write_private(&file, body.as_bytes()).map_err(|error| format!("cannot write the page for the CLI: {error}"))?;
            match cli::set_page_args(name, &content_type, Some(&file)) {
                Ok(command) => Ok((command, Some(file), Value::Null)),
                Err(error) => {
                    let _ = std::fs::remove_file(&file);
                    Err(error)
                }
            }
        })
    }

    fn cli_set_text(&mut self, name: String, key: String, value: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let command = match cli::set_text_args(&name, &key, &value) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "set_text", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn cli_balance(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| cli::run_balance(&self.cli_setup()?))())
    }

    fn cli_bridge(&mut self, amount: String, from_key: String, node: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            // Everything is checked before anything is remembered or run.
            cli::bridge_amount(&amount)?;
            let key = cli::bridge_key(&from_key)?;
            let node = cli::bridge_node(&node)?;
            let setup = self.cli_setup()?;
            if self.bridge_key.as_deref() != Some(key.as_str()) || self.bridge_node.as_deref() != Some(node.as_str()) {
                self.bridge_key = Some(key.clone());
                self.bridge_node = Some(node.clone());
                self.save();
            }
            cli::run_bridge(&setup, &amount, &key, &node, send)
        })())
    }

    fn cli_faucet(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let wallet = cli::find_wallet(self.cli_wallet.as_deref())
                .ok_or_else(|| "create your wallet first (Welcome, Create my wallet)".to_owned())?;
            let account = cli::saved_account(&wallet, "public")
                .ok_or_else(|| "your wallet has no public account yet; create your wallet first (Welcome, Create my wallet)".to_owned())?;
            let url = std::env::var("LOGOS_NAMES_FAUCET").ok().filter(|url| !url.trim().is_empty());
            cli::request_faucet(url.as_deref().unwrap_or(cli::FAUCET_URL), &account)
        })())
    }

    fn cli_status(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let program = cli::find_program(self.cli.as_deref()).filter(|path| cli::is_executable(path));
            let wallet = cli::find_wallet(self.cli_wallet.as_deref());
            let has_wallet = wallet.as_ref().is_some_and(|dir| dir.join(cli::STORAGE_FILE).is_file());
            let account = |which: &str| wallet.as_ref().filter(|_| has_wallet).and_then(|dir| cli::saved_account(dir, which));
            let setup = self.cli_setup().ok().filter(|_| has_wallet);
            Ok(json!({
                "cli": program.as_ref().map(|path| path.to_string_lossy().into_owned()),
                "prover": program.as_ref().is_some_and(|path| cli::has_prover(path)),
                "outdated": program.as_ref().is_some_and(|path| !cli::speaks_module(path)),
                "old_wallet": setup.as_ref().is_some_and(cli::wallet_is_old),
                "installing": self.running_job(cli::INSTALL_JOB),
                "wallet_dir": wallet.as_ref().map(|dir| dir.to_string_lossy().into_owned()),
                "wallet": has_wallet,
                "public": account("public"),
                "private": account("private"),
                "busy": self.running_job(cli::WALLET_JOB),
            }))
        })())
    }

    fn cli_install(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let jobs = self.jobs()?;
            let (bin, man) = self.install_dirs()?;
            let ours = bin.join("names");
            if let Some(id) = jobs.running_for(cli::INSTALL_JOB) {
                return Ok(json!({ "installed": false, "path": ours, "version": null, "prover": false, "job": jobs.state(&id)? }));
            }
            // A CLI that already works is left as it is, unless it is the one
            // installed here and its prover is missing: then install again.
            if let Some(program) = cli::find_program(self.cli.as_deref()) {
                if let Ok(version) = cli::version(&program) {
                    let prover = cli::has_prover(&program);
                    let is_ours = program == ours;
                    if (prover && cli::speaks_module(&program)) || !is_ours {
                        if is_ours {
                            self.adopt_cli(&ours);
                        }
                        return Ok(json!({ "installed": true, "path": program, "version": version, "prover": prover, "job": null }));
                    }
                }
            }
            let job = jobs.start_install(&installer_script(), &bin, &man)?;
            Ok(json!({ "installed": false, "path": ours, "version": null, "prover": false, "job": job }))
        })())
    }

    fn cli_create_wallet(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            if let Some(id) = self.jobs().ok().and_then(|jobs| jobs.running_for(cli::WALLET_JOB)) {
                return Err(format!("your wallet is busy with something else (job {id}); wait for it to finish"));
            }
            let made = cli::run_create_wallet(&setup)?;
            // The folder it was made in is the one used from now on.
            if self.cli_wallet.is_none() {
                self.cli_wallet = Some(setup.wallet.to_string_lossy().into_owned());
                self.save();
            }
            self.forget_mine();
            Ok(made)
        })())
    }

    fn cli_shield(&mut self, amount: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let command = cli::setup_args(&amount)?;
            if command[1] == "--shield=0" {
                return Err("enter an amount above zero".to_owned());
            }
            self.wallet_job("shield", |setup| {
                cli::default_public(setup)?;
                Ok(command)
            })
        })())
    }

    fn cli_pay_account(&mut self, account: String, amount: String, from_private: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            // Checked before the CLI is looked for, so bad input fails the same way every time.
            if !matches!(cli::pay_to(&account)?, cli::PayTo::Account(_)) {
                return Err("that is a name; pay it as a name".to_owned());
            }
            if !cli::is_amount(&amount.trim().replace(',', "")) {
                return Err("enter a whole amount, such as 1000".to_owned());
            }
            self.wallet_job("pay_account", |setup| cli::pay_account_args(setup, &account, &amount, from_private))
        })())
    }

    fn cli_register(&mut self, name: String, as_public: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let command = match cli::register_args(&name, as_public) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "register", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_feed(&mut self, following: bool) -> String {
        if following {
            if let Err(refused) = only_the_names_app() {
                return reply(Err(refused));
            }
        }
        reply((|| {
            let setup = self.cli_setup()?;
            if following && self.follows.is_empty() {
                return Ok(json!({ "posts": [], "updating": false, "error": null }));
            }
            let follows = if following { self.follows.clone() } else { Vec::new() };
            let key = format!("feed:{}", follows.join(","));
            Ok(self.board_read(key, move || cli::run_feed(&setup, &follows)))
        })())
    }

    fn board_posts_of(&mut self, name: String, before: i64) -> String {
        reply((|| {
            let name = registry::parse_name(&name)?;
            let setup = self.cli_setup()?;
            let key = format!("author:{name}:{before}");
            Ok(self.board_read(key, move || cli::run_posts_of(&setup, &name, before)))
        })())
    }

    fn board_thread(&mut self, post: String) -> String {
        reply((|| {
            let setup = self.cli_setup()?;
            Ok(self.board_read(format!("thread:{post}"), move || cli::run_thread(&setup, &post)))
        })())
    }

    fn board_post(&mut self, name: String, text: String, cids_json: String, reply_to: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let cids: Vec<String> = match serde_json::from_str::<Vec<String>>(if cids_json.trim().is_empty() { "[]" } else { &cids_json }) {
            Ok(cids) => cids,
            Err(_) => return reply(Err("cids must be a JSON list of CIDs".to_owned())),
        };
        let command = match cli::post_args(&name, &text, &cids, &reply_to) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "post", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_like(&mut self, name: String, post: String, undo: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let command = match cli::like_args(&name, &post, undo) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "like", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_hide(&mut self, post: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let (name, command) = match cli::hide_args(&post) {
            Ok(made) => made,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "hide", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn main_name(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(Ok(json!(self.main_name)))
    }

    fn set_main_name(&mut self, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let name = registry::parse_name(&name)?;
            self.main_name = Some(name.clone());
            self.save();
            Ok(json!(name))
        })())
    }

    fn anon_name(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            let saved: Value = std::fs::read_to_string(setup.wallet.join("names.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or(Value::Null);
            Ok(saved["anon"].as_str().and_then(|name| registry::parse_name(name).ok()).map_or(Value::Null, Value::from))
        })())
    }

    fn running_jobs(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| Ok(Value::from(self.jobs()?.running())))())
    }

    fn make_anon(&mut self, as_public: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let mut command = vec!["anon".to_owned()];
        if as_public {
            command.push("--public".to_owned());
        }
        self.cli_job("anon", "anon", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn spell_check(&mut self, text: String) -> String {
        reply(Ok(spell::check(&text)))
    }

    fn make_public(&mut self, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let name = match registry::parse_name(&name) {
            Ok(name) => name,
            Err(error) => return reply(Err(error)),
        };
        let command = vec!["make-public".to_owned(), "--".to_owned(), name.clone()];
        self.cli_job(&name, "make_public", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn dm_names(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(Ok(json!(self.dm_names)))
    }

    fn set_dm_name(&mut self, convo_id: String, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let convo = convo_id.trim();
            if convo.is_empty() || convo.len() > 200 || !convo.bytes().all(|b| b.is_ascii_graphic()) {
                return Err("not a conversation id".to_owned());
            }
            if name.trim().is_empty() {
                self.dm_names.remove(convo);
            } else {
                self.dm_names.insert(convo.to_owned(), registry::parse_name(&name)?);
            }
            self.save();
            Ok(json!(self.dm_names))
        })())
    }

    fn chat_names(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(Ok(json!(self.chat_names)))
    }

    fn set_chat_name(&mut self, address: String, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let address = address.trim();
            if address.is_empty() || address.len() > 200 || !address.bytes().all(|b| b.is_ascii_graphic()) {
                return Err("not a chat address".to_owned());
            }
            self.chat_names.insert(address.to_owned(), registry::parse_name(&name)?);
            self.save();
            Ok(json!(self.chat_names))
        })())
    }

    fn board_likes(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(Ok(json!(self.likes)))
    }

    fn board_toggle_like(&mut self, name: String, post: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let post = match cli::post_ref(&post) {
            Ok(post) => post,
            Err(error) => return reply(Err(error)),
        };
        let undo = self.likes.contains(&post);
        let command = match cli::like_args(&name, &post, undo) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        let started = self.cli_job(&name, "like", |_, _, _| Ok((command, None, Value::Null)));
        if serde_json::from_str::<Value>(&started).is_ok_and(|reply| reply["success"] == true) {
            self.likes.retain(|liked| *liked != post);
            if !undo {
                self.likes.push(post);
            }
            self.save();
        }
        started
    }

    fn cli_pay(&mut self, name: String, amount: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let command = match cli::pay_args(&name, &amount) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        self.cli_job(&name, "pay", |_, _, _| Ok((command, None, Value::Null)))
    }

    fn board_follows(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(Ok(json!(self.follows)))
    }

    fn board_follow(&mut self, name: String, undo: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let name = registry::parse_name(&name)?;
            self.follows.retain(|followed| *followed != name);
            if !undo {
                self.follows.push(name);
            }
            self.save();
            Ok(json!(self.follows))
        })())
    }

    fn job(&mut self, id: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let mut state = (|| self.jobs()?.state(id.trim()))();
        if let Ok(state) = &mut state {
            self.job_outcome(state);
            state["fee"] = self.jobs().map_or(Value::Null, |jobs| cli::parse_fee(&jobs.whole_log(id.trim())));
        }
        // A change that ended may have changed its name: read it fresh next time.
        if let Ok(state) = &state {
            if state["state"] != "running" {
                if let Some(name) = state["name"].as_str() {
                    let name = name.to_owned();
                    self.forget_reads(&name);
                    self.forget_mine();
                }
            }
        }
        reply(state)
    }

    fn set_cli(&mut self, path: String) -> bool {
        if only_the_names_app().is_err() {
            return false;
        }
        let path = path.trim();
        if path.is_empty() {
            self.cli = None;
        } else if cli::is_executable(std::path::Path::new(path)) && cli::is_owned_safely(std::path::Path::new(path), false, true) {
            self.cli = Some(path.to_owned());
        } else {
            return false;
        }
        self.save();
        true
    }

    fn set_cli_wallet(&mut self, dir: String) -> bool {
        if only_the_names_app().is_err() {
            return false;
        }
        let dir = dir.trim();
        if dir.is_empty() {
            self.cli_wallet = None;
        } else if cli::is_owned_safely(std::path::Path::new(dir), true, false) {
            self.cli_wallet = Some(dir.to_owned());
        } else {
            return false;
        }
        self.save();
        true
    }

    fn set_registry(&mut self, program_id: String) -> bool {
        if only_the_names_app().is_err() {
            return false;
        }
        match registry::parse_account(&program_id) {
            Ok(program) => {
                self.program = Some(program);
                self.forget_all_reads();
                self.save();
                true
            }
            Err(_) => false,
        }
    }

    fn set_wallet_files(&mut self, config_path: String, storage_path: String) -> bool {
        if only_the_names_app().is_err() {
            return false;
        }
        let (config, storage) = (config_path.trim(), storage_path.trim());
        if config.is_empty() && storage.is_empty() {
            self.wallet_files = None;
        } else if cli::is_owned_safely(std::path::Path::new(config), false, false) && cli::is_owned_safely(std::path::Path::new(storage), false, false) {
            self.wallet_files = Some((config.to_owned(), storage.to_owned()));
        } else {
            return false;
        }
        self.save();
        true
    }

    fn set_sequencer(&mut self, url: String) -> bool {
        if only_the_names_app().is_err() {
            return false;
        }
        let url = url.trim();
        if is_http_url(url) {
            self.sequencer = url.to_owned();
            self.forget_all_reads();
            self.token_info.clear();
            self.save();
            true
        } else {
            false
        }
    }

    // ── Tokens ──────────────────────────────────────────────────────────

    fn set_token_program(&mut self, program_id: String) -> bool {
        if only_the_names_app().is_err() {
            return false;
        }
        let program_id = program_id.trim();
        if program_id.is_empty() {
            self.token_program = None;
        } else {
            match registry::parse_account(program_id) {
                Ok(program) if program != registry::NATIVE_TOKEN => self.token_program = Some(program),
                _ => return false,
            }
        }
        self.forget_tokens();
        self.token_info.clear();
        self.save();
        true
    }

    fn cli_token_holdings(&mut self, account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let only = match account.trim() {
                "" => None,
                account => {
                    let (private, id) = provider::account_ref(account)?;
                    Some(provider::account_written(private, &id))
                }
            };
            let (program, read) = self.token_read()?;
            let holdings = read["value"]["holdings"].as_array().map(|rows| {
                rows.iter().filter(|row| only.as_ref().is_none_or(|only| row["account"] == only.as_str())).cloned().collect::<Vec<_>>()
            });
            Ok(json!({
                "token_program": registry::account_text(&program),
                "holdings": holdings,
                "updating": read["updating"],
                "error": read["error"],
            }))
        })())
    }

    fn cli_token_pending(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let (program, read) = self.token_read()?;
            Ok(json!({
                "token_program": registry::account_text(&program),
                "pending": read["value"]["pending"],
                "updating": read["updating"],
                "error": read["error"],
            }))
        })())
    }

    fn cli_token_info(&mut self, definition: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let definition = cli::token_definition(&definition)?;
            let program = self.token_program_set()?;
            self.token_info_of(&program, &definition)
        })())
    }

    fn cli_token_send(&mut self, definition: String, amount: String, to: String, from_private: bool, to_public: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            // Checked before the CLI is looked for, so bad input fails the same way every time.
            let program = self.token_program_set()?;
            let command = cli::token_send_args(&program, &definition, &amount, &to, from_private, to_public)?;
            self.wallet_job("token_send", |setup| {
                // The account it comes from must exist before anything runs.
                if from_private {
                    cli::default_private(setup)?;
                } else {
                    cli::default_public(setup)?;
                }
                Ok(command)
            })
        })())
    }

    fn cli_token_receive(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let program = self.token_program_set()?;
            self.wallet_job("token_receive", |setup| {
                cli::default_private(setup)?;
                Ok(cli::token_receive_args(&program))
            })
        })())
    }

    fn cli_accounts(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| cli::run_account_list(&self.cli_setup()?))())
    }

    fn cli_account_new(&mut self, kind: String, label: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            self.no_job_running()?;
            cli::run_account_new(&setup, kind.trim(), &label)
        })())
    }

    fn cli_account_label(&mut self, account: String, label: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            self.no_job_running()?;
            cli::run_account_label(&setup, &account, &label)
        })())
    }

    fn cli_account_use(&mut self, account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let setup = self.cli_setup()?;
            self.no_job_running()?;
            let chosen = cli::run_account_use(&setup, &account)?;
            // Balances and holdings were read for the old default.
            self.forget_mine();
            Ok(chosen)
        })())
    }

    fn cli_fee(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| cli::run_fee(&self.cli_setup()?))())
    }

    // ── The Logos Blockchain ────────────────────────────────────────────

    fn chain_status(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let info = node::call("get_cryptarchia_info", json!([]), node::QUICK)?;
            let time = node::call("get_time_info", json!([]), node::QUICK)?;
            Ok(chain::status(&info, &time))
        })())
    }

    fn chain_accounts(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let keys = node::call("wallet_get_known_addresses", json!([]), node::QUICK)?;
            let keys: Vec<String> = keys.as_array().map(|k| k.iter().filter_map(|k| k.as_str().and_then(chain::parse_key)).collect()).unwrap_or_default();
            // The node lists some keys its wallet can't spend from; those are left out.
            Ok(Value::from(
                keys.into_iter()
                    .take(MAX_WALLET_ACCOUNTS)
                    .filter_map(|key| match node::call("wallet_get_balance", json!([key]), node::QUICK) {
                        Ok(balance) => Some(json!({ "key": key, "balance": chain::balance_of(&balance).to_string() })),
                        Err(error) if chain::not_held(&error) => None,
                        Err(_) => Some(json!({ "key": key, "balance": null })),
                    })
                    .collect::<Vec<_>>(),
            ))
        })())
    }

    fn chain_stake(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let aged = node::call("wallet_get_leader_aged_notes", json!([""]), node::QUICK)?;
            let vouchers = node::call("wallet_get_claimable_vouchers", json!([]), node::QUICK)?;
            let time = node::call("get_time_info", json!([]), node::QUICK)?;
            Ok(chain::stake(&aged, &vouchers, &time))
        })())
    }

    fn chain_recipient(&mut self, to: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(self.chain_target(&to))
    }

    fn chain_send(&mut self, from_key: String, to: String, amount: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let target = self.chain_target(&to)?;
            let (from, to_key, amount) = chain::send_input(&from_key, target["key"].as_str().unwrap_or_default(), &amount)?;
            let balance = chain::balance_of(&node::call("wallet_get_balance", json!([from]), node::QUICK)?);
            if balance < u128::from(amount) {
                return Err(format!("that key holds {balance}, less than {amount}"));
            }
            let mut plan = json!({
                "from": from,
                "to": to_key,
                "name": target["name"],
                "forwarded_from": target["forwarded_from"],
                "amount": amount.to_string(),
                "balance": balance.to_string(),
            });
            if send {
                let before = chain::notes_of(&node::call("wallet_get_notes", json!([from, ""]), node::QUICK)?);
                let sent = node::call("wallet_transfer_funds", json!([from, [from], to_key, amount.to_string(), ""]), node::PROVING)?;
                let hash = chain::hash_of(&sent);
                // Sent: from here nothing fails, so the hash always comes back.
                let stored = hash.as_ref().and_then(|hash| node::call("get_transaction", json!([hash]), node::QUICK).ok());
                let fee = stored.as_ref().and_then(|tx| chain::transfer_fee(tx, &before));
                if let (Some(hash), Some(tx)) = (&hash, &stored) {
                    let inputs = chain::transfer_parts(tx).0;
                    chain_sent_add(hash, json!({ "from": from, "inputs": inputs, "fee": fee.map(|f| f.to_string()), "at": now() }));
                }
                plan["hash"] = hash.map_or(Value::Null, Value::from);
                plan["fee"] = fee.map_or(Value::Null, |f| Value::from(f.to_string()));
            }
            Ok(plan)
        })())
    }

    fn chain_tx(&mut self, hash: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let hash = chain::parse_key(&hash).ok_or("a transaction hash is 64 hex characters")?;
            // A send made here is in a block once its input notes leave the sender's wallet
            // (the node stores a transaction as soon as it takes it, so finding it says nothing).
            let Some(known) = chain_sent().remove(&hash) else {
                // Not sent from here: the node is asked, so a node that is down or has no
                // record of it says so. Even when it has it, which key it spends from (and
                // so whether it is in a block) isn't known here.
                node::call("get_transaction", json!([hash]), node::QUICK)?;
                return Err("That transaction wasn't sent from the Names app, so the app can't tell whether it is in a block.".to_owned());
            };
            let inputs: Vec<String> = known["inputs"].as_array().into_iter().flatten().filter_map(|i| i.as_str().map(str::to_owned)).collect();
            let notes = chain::notes_of(&node::call("wallet_get_notes", json!([known["from"], ""]), node::QUICK)?);
            Ok(json!({
                "hash": hash,
                "state": if chain::spent(&inputs, &notes) { "in_block" } else { "pending" },
                "fee": known["fee"],
            }))
        })())
    }

    fn chain_claim(&mut self, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let vouchers = node::call("wallet_get_claimable_vouchers", json!([]), node::QUICK)?;
            let count = vouchers["vouchers"].as_array().map_or(0, Vec::len);
            if !send {
                return Ok(json!({
                    "claimable": chain::amount(&vouchers["total_claimable"]).to_string(),
                    "vouchers": count,
                    "reward_per_voucher": chain::amount(&vouchers["reward_amount"]).to_string(),
                }));
            }
            if count == 0 {
                return Err("There is nothing to claim: your node has no leader rewards waiting.".to_owned());
            }
            // The fee comes from the node's leader funding key, known only once the claim
            // exists, and the solo node can put it in a block within a second: so every
            // key's notes are read first.
            let keys = node::call("wallet_get_known_addresses", json!([]), node::QUICK)?;
            let mut before = serde_json::Map::new();
            for key in keys.as_array().into_iter().flatten().filter_map(Value::as_str).take(MAX_WALLET_ACCOUNTS) {
                if let Ok(notes) = node::call("wallet_get_notes", json!([key, ""]), node::QUICK) {
                    before.extend(chain::notes_of(&notes));
                }
            }
            let hash = chain::hash_of(&node::call("leader_claim", json!([]), node::PROVING)?);
            // Claimed: from here nothing fails, so the hash always comes back.
            let stored = hash.as_ref().and_then(|hash| node::call("get_transaction", json!([hash]), node::QUICK).ok());
            let fee = stored.as_ref().and_then(|tx| chain::transfer_fee(tx, &before));
            Ok(chain::claim_result(hash, &vouchers, fee))
        })())
    }

    // ── Wallet Provider SDK ─────────────────────────────────────────────

    fn request_access(&mut self, reason: String) -> String {
        reply((|| {
            let app = asking_app()?;
            let now = provider::now_ms();
            let store = self.store();
            let granted = store.granted(&app);
            if !granted.is_empty() {
                return Ok(json!({ "request": null, "kind": provider::CONNECT, "state": "approved", "accounts": granted }));
            }
            if let Some(waiting) = store.requests.iter().find(|r| r["app"] == app.as_str() && r["kind"] == provider::CONNECT && r["state"] == "pending") {
                return Ok(provider::app_view(waiting));
            }
            let request = provider::connect_request(&app, &reason, now)?;
            let view = provider::app_view(&request);
            store.add(request, now)?;
            store.save();
            Ok(view)
        })())
    }

    fn granted_accounts(&mut self) -> String {
        reply((|| {
            let app = asking_app()?;
            let granted = self.store().granted(&app);
            let accounts: Vec<Value> = granted.iter().map(|account| json!({ "account": account, "account_kind": kind_of(account) })).collect();
            Ok(json!({ "connected": !accounts.is_empty(), "accounts": accounts }))
        })())
    }

    fn account_balances(&mut self) -> String {
        reply((|| {
            let app = asking_app()?;
            let granted = self.store().granted(&app);
            Ok(Value::from(granted.iter().map(|account| self.granted_balance(account)).collect::<Vec<_>>()))
        })())
    }

    fn propose_transfer(&mut self, asset: String, amount: String, destination: String) -> String {
        reply((|| {
            let app = asking_app()?;
            let now = provider::now_ms();
            let mut request = provider::transfer_request(&app, &asset, &amount, &destination, now)?;
            if let Some(definition) = provider::token_of(&request) {
                // Reading the token costs a run of the CLI, so only an app that
                // may ask now gets one.
                let store = self.store();
                if store.granted(&app).is_empty() {
                    return Err(NOT_CONNECTED.to_owned());
                }
                store.room_for(&app, now)?;
                let program = self.token_program.ok_or("this wallet can't send tokens yet: it has no token program set")?;
                // The app learns only that the token isn't there, not why the wallet couldn't read it.
                let info = self.token_info_of(&program, &definition).map_err(|_| "this wallet's token program has no such token".to_owned())?;
                let label = program_registry::label(&registry::account_text(&program));
                provider::add_token(&mut request, info["name"].as_str(), &program, label.as_ref());
            }
            self.queue(&app, request, now)
        })())
    }

    fn propose_call(&mut self, program_id: String, accounts_json: String, instruction_hex: String, summary: String) -> String {
        reply((|| {
            let app = asking_app()?;
            let now = provider::now_ms();
            if self.store().granted(&app).is_empty() {
                return Err(NOT_CONNECTED.to_owned());
            }
            // Checked before the registry is asked, so a bad call costs nothing.
            provider::call_request(&app, &program_id, &accounts_json, &instruction_hex, &summary, None, None, now)?;
            let program = registry::parse_account(program_id.trim().strip_prefix("Public/").unwrap_or(program_id.trim()))?;
            let label = program_registry::label(&registry::account_text(&program));
            let request = provider::call_request(&app, &program_id, &accounts_json, &instruction_hex, &summary, label.as_ref(), self.program.as_ref(), now)?;
            self.queue(&app, request, now)
        })())
    }

    fn request_status(&mut self, request_id: String) -> String {
        reply((|| {
            let app = asking_app()?;
            self.provider_refresh();
            let store = self.store();
            match store.get(request_id.trim()) {
                // The Names app also sees the payment's job and, if it failed, the CLI's words.
            Some(request) if may_act(&calling()) => {
                Ok(merged(provider::app_view(request), &json!({ "app": request["app"], "job": request["job"], "problem": request["problem"] })))
            }
            Some(request) if request["app"] == app.as_str() => Ok(provider::app_view(request)),
                // Another app's request is answered as if it didn't exist.
                _ => Err("there is no such request".to_owned()),
            }
        })())
    }

    fn cancel_request(&mut self, request_id: String) -> String {
        reply((|| {
            let app = asking_app()?;
            let now = provider::now_ms();
            let store = self.store();
            let request = store.get_mut(request_id.trim()).filter(|request| request["app"] == app.as_str()).ok_or("there is no such request")?;
            if request["state"] != "pending" {
                return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
            }
            request["state"] = json!("cancelled");
            request["decided"] = json!(now);
            let view = provider::app_view(request);
            store.save();
            Ok(view)
        })())
    }

    fn drop_access(&mut self) -> String {
        reply((|| {
            let app = asking_app()?;
            let store = self.store();
            let revoked = store.revoke(&app);
            store.save();
            Ok(json!({ "revoked": revoked }))
        })())
    }

    fn pending_requests(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.provider_refresh();
        let pending: Vec<Value> = self.store().requests.iter().filter(|request| request["state"] == "pending").cloned().collect();
        // The wallet's other public accounts take the CLI a moment to list, so
        // they come from a read in the background and join the choices once known.
        let others = if pending.iter().any(|request| request["kind"] != provider::TRANSFER) { self.publics_read_lately() } else { Vec::new() };
        let choices = self.wallet_choices(&others);
        let mut publishers = HashMap::new();
        for request in &pending {
            let app = request["app"].as_str().unwrap_or_default().to_owned();
            if !publishers.contains_key(&app) {
                let publisher = self.publisher_of(&app);
                publishers.insert(app, publisher);
            }
        }
        let store = self.store();
        let views: Vec<Value> = pending
            .into_iter()
            .map(|mut request| {
                let app = request["app"].as_str().unwrap_or_default().to_owned();
                let granted = store.granted(&app);
                let kind = request["kind"].as_str().unwrap_or_default().to_owned();
                request["choices"] = Value::from(
                    choices
                        .iter()
                        .filter(|choice| offered_for(&kind, choice))
                        .map(|choice| merged(choice.clone(), &json!({ "granted": granted.iter().any(|account| choice["account"] == account.as_str()) })))
                        .collect::<Vec<_>>(),
                );
                request["granted"] = json!(granted);
                request["publisher"] = publishers.get(&app).cloned().unwrap_or(Value::Null);
                request
            })
            .collect();
        reply(Ok(Value::from(views)))
    }

    fn approve_request(&mut self, request_id: String, account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(self.approve(request_id.trim(), &account))
    }

    fn reject_request(&mut self, request_id: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let now = provider::now_ms();
            let store = self.store();
            let request = store.get_mut(request_id.trim()).ok_or("there is no such request")?;
            if request["state"] != "pending" {
                return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
            }
            request["state"] = json!("rejected");
            request["decided"] = json!(now);
            let view = provider::app_view(request);
            store.save();
            Ok(view)
        })())
    }

    fn app_grants(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let store = self.store();
        let grants: Vec<Value> = store
            .grants
            .iter()
            .map(|(app, grant)| json!({ "app": app, "accounts": grant["accounts"], "since": grant["since"] }))
            .collect();
        reply(Ok(Value::from(grants)))
    }

    fn revoke_app(&mut self, app: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let store = self.store();
        let revoked = store.revoke(app.trim());
        store.save();
        reply(Ok(json!({ "revoked": revoked })))
    }
}

// ── Wallet Provider SDK: the parts behind the methods ───────────────────

/// What an app that hasn't been given access is told when it proposes a transaction.
const NOT_CONNECTED: &str = "ask for access first (request_access), then propose it";

/// How often an app's balance reads may go to the chain (or the CLI).
const BALANCE_EVERY: std::time::Duration = std::time::Duration::from_secs(15);

/// The app asking, as Basecamp names its credential: a module's name, or
/// `host` for the host itself (Basecamp, or `logosctl` driving the module).
/// A caller that can't be named, or a module that calls itself `host`, can't
/// ask the wallet for anything.
fn asking_app() -> Result<String, String> {
    match calling() {
        logos_rust_sdk::LogosCaller::Module { name, .. } if name != "host" => provider::app_name(&name),
        logos_rust_sdk::LogosCaller::HostAnchor => Ok("host".to_owned()),
        other => Err(format!("the wallet can't tell which app is asking ({}), so it refuses", other.describe_for_human())),
    }
}

/// Whether `choice` (from `wallet_choices`) may be used for a request of `kind`:
/// a payment comes from an account the CLI pays from, a call is signed by a
/// public account, and access may be given to any.
fn offered_for(kind: &str, choice: &Value) -> bool {
    match kind {
        provider::TRANSFER => choice["pays"] == true,
        provider::CALL => choice["account_kind"] == "public",
        _ => true,
    }
}

/// `public` or `private`, for an account written `Public/...` or `Private/...`.
fn kind_of(account: &str) -> &'static str {
    if account.starts_with("Private/") {
        "private"
    } else {
        "public"
    }
}

impl Names {
    /// The provider's store, read from disk the first time, with requests
    /// nobody answered in time expired.
    fn store(&mut self) -> &mut provider::Store {
        if self.provider.is_none() {
            let path = self.provider_path.clone().or_else(|| config_dir().map(|dir| dir.join("provider.json")));
            self.provider = Some(provider::Store::load(path));
        }
        let store = self.provider.as_mut().expect("the store was just loaded");
        if store.sweep(provider::now_ms()) {
            store.save();
        }
        store
    }

    /// Adds a transaction request from `app`, which must have been given access.
    /// Whether the asking app is really published by the LNS name its package names, as the
    /// Program Registry checks it (`verify_app`, docs/APPS.md in lez-program-registry):
    /// `{status, publisher, module, version, record}`, `{status: "unavailable"}` when the
    /// registry module isn't installed or couldn't say, or null for Basecamp itself. Kept for
    /// a minute per app.
    fn publisher_of(&mut self, app: &str) -> Value {
        if app == "host" {
            return Value::Null;
        }
        let now = now();
        if let Some((answer, at)) = self.publishers.get(app) {
            if now.saturating_sub(*at) < 60 {
                return answer.clone();
            }
        }
        let answer = program_registry::publisher(app).unwrap_or_else(|| json!({ "status": "unavailable" }));
        self.publishers.insert(app.to_owned(), (answer.clone(), now));
        answer
    }

    fn queue(&mut self, app: &str, request: Value, now: u64) -> Result<Value, String> {
        let store = self.store();
        if store.granted(app).is_empty() {
            return Err(NOT_CONNECTED.to_owned());
        }
        let view = provider::app_view(&request);
        store.add(request, now)?;
        store.save();
        Ok(view)
    }

    /// The accounts a request may use, all in the names CLI's wallet: its main
    /// public and private accounts, which pay (`names setup` saved them), and
    /// its other public accounts `others`, which can sign program calls.
    fn wallet_choices(&self, others: &[AccountId]) -> Vec<Value> {
        let mut choices = Vec::new();
        let wallet = cli::find_wallet(self.cli_wallet.as_deref()).filter(|dir| dir.join(cli::STORAGE_FILE).is_file());
        let Some(wallet) = wallet else { return choices };
        for (which, private, label) in [("public", false, "Names wallet, main public account"), ("private", true, "Names wallet, private account")] {
            if let Some(id) = cli::saved_account(&wallet, which).and_then(|id| registry::parse_account(&id).ok()) {
                choices.push(json!({ "account": provider::account_written(private, &id), "account_kind": which, "pays": true, "label": label }));
            }
        }
        for id in others.iter().take(MAX_WALLET_ACCOUNTS) {
            let account = provider::account_written(false, id);
            if !choices.iter().any(|choice| choice["account"] == account.as_str()) {
                choices.push(json!({ "account": account, "account_kind": "public", "pays": false, "label": "Names wallet, public account" }));
            }
        }
        choices
    }

    /// The wallet's public accounts as last read in the background (see
    /// `pending_requests`); empty until the first read ends.
    fn publics_read_lately(&self) -> Vec<AccountId> {
        let Ok(setup) = self.cli_setup() else { return Vec::new() };
        let read = self.read_in_background("provider:publics".to_owned(), std::time::Duration::from_secs(30), move || {
            wallet::accounts(&setup).map(|held| json!(held.public.iter().map(registry::account_text).collect::<Vec<_>>()))
        });
        read["value"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|id| registry::parse_account(id.as_str()?).ok())
            .collect()
    }

    /// Approves a pending request with the account(s) the person chose.
    /// Nothing is signed or sent unless the request is pending, the account
    /// is one of its choices, and the existing payment or signing path
    /// accepts it.
    fn approve(&mut self, id: &str, account: &str) -> Result<Value, String> {
        let now = provider::now_ms();
        let request = self.store().get(id).cloned().ok_or("there is no such request")?;
        if request["state"] != "pending" {
            return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
        }
        let kind = request["kind"].as_str().unwrap_or_default().to_owned();
        let app = request["app"].as_str().unwrap_or_default().to_owned();
        let chosen: Vec<String> = account
            .split(',')
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .map(|account| provider::account_ref(account).map(|(private, id)| provider::account_written(private, &id)))
            .collect::<Result<_, _>>()?;
        if chosen.is_empty() {
            return Err("choose an account".to_owned());
        }
        if kind != provider::CONNECT && chosen.len() != 1 {
            return Err("choose one account".to_owned());
        }
        // Checked against the wallet itself, not a list read earlier: a call
        // must be signed by an account the CLI's wallet holds now.
        let others = if kind == provider::TRANSFER { Vec::new() } else { self.held().map(|held| held.public).unwrap_or_default() };
        let choices = self.wallet_choices(&others);
        let offered: Vec<&Value> = choices.iter().filter(|choice| offered_for(&kind, choice)).collect();
        if kind == provider::CALL && !others.iter().any(|id| provider::account_written(false, id) == chosen[0]) {
            return Err(format!("{} isn't an account the names wallet can sign with now", chosen[0]));
        }
        if let Some(stray) = chosen.iter().find(|wanted| !offered.iter().any(|choice| choice["account"] == wanted.as_str())) {
            return Err(format!("{stray} isn't one of the accounts this request can use"));
        }
        let mut outcome = json!({ "decided": now, "account": if kind == provider::CONNECT { Value::Null } else { json!(chosen[0]) } });
        let failure = match kind.as_str() {
            provider::CONNECT => {
                let granted = self.store().grant(&app, &chosen, now)?;
                outcome["state"] = json!("approved");
                outcome["granted"] = json!(granted);
                None
            }
            provider::TRANSFER => {
                // A payment that didn't start sent nothing: the request stays pending.
                let job = self.provider_transfer(&request, &chosen[0])?;
                outcome["state"] = json!("running");
                outcome["job"] = job["id"].clone();
                None
            }
            provider::CALL => match self.provider_call(&request, &chosen[0]) {
                Ok(tx) => {
                    outcome["state"] = json!("done");
                    outcome["tx"] = json!(tx);
                    None
                }
                // A send that failed may still have gone through, so it is
                // never offered again: the request ends failed.
                Err(error) => {
                    outcome["state"] = json!("failed");
                    outcome["error"] = json!("the wallet couldn't send it");
                    outcome["problem"] = json!(error);
                    Some(error)
                }
            },
            _ => return Err("this request can't be approved".to_owned()),
        };
        let store = self.store();
        let view = match store.get_mut(id) {
            Some(stored) => {
                for (key, value) in outcome.as_object().into_iter().flatten() {
                    stored[key.as_str()] = value.clone();
                }
                if kind == provider::CALL {
                    // The instruction was only kept to be sent.
                    if let Some(fields) = stored.as_object_mut() {
                        fields.remove("instruction");
                    }
                }
                merged(provider::app_view(stored), &json!({ "job": stored["job"] }))
            }
            None => outcome,
        };
        store.save();
        match failure {
            Some(error) => Err(error),
            None => Ok(view),
        }
    }

    /// Starts an approved transfer through the names CLI, as `cli_pay` and
    /// `cli_pay_account` would: a name is paid with `names pay` (from the
    /// public account with `--from`), an account with `auth-transfer send`.
    fn provider_transfer(&self, request: &Value, account: &str) -> Result<Value, String> {
        let (private, _) = provider::account_ref(account)?;
        let amount = request["amount"].as_str().unwrap_or_default().to_owned();
        let destination = request["destination"].as_str().unwrap_or_default().to_owned();
        if let Some(definition) = provider::token_of(request) {
            // The prompt named one token program; nothing is sent with another.
            let program = self.token_program_set()?;
            if request["token_program"] != registry::account_text(&program).as_str() {
                return Err("the token program has changed since this was asked; reject it, and the app can ask again".to_owned());
            }
            let command = cli::token_send_args(&program, &registry::account_text(&definition), &amount, &destination, private, false)?;
            return self.wallet_job("token_send", |_| Ok(command));
        }
        match cli::pay_to(&destination)? {
            cli::PayTo::Name(name) => {
                let command = if private {
                    cli::pay_args(&name, &amount)?
                } else {
                    cli::pay_from_public_args(&self.cli_setup()?, &name, &amount)?
                };
                let started = parsed_reply(&self.cli_job(&name, "pay", |_, _, _| Ok((command, None, Value::Null))))?;
                Ok(started)
            }
            cli::PayTo::Account(_) => self.wallet_job("pay_account", |setup| cli::pay_account_args(setup, &destination, &amount, private)),
        }
    }

    /// Signs and sends an approved program call through the names CLI, the
    /// way the module sends its own registry changes (`names module send`),
    /// with the call's program in place of the registry: the CLI tries it
    /// against the chain first, sizes its gas, signs, sends and waits for its
    /// block. The signer's own actor of the program comes first, then the
    /// accounts the request reaches. Returns the transaction hash.
    fn provider_call(&self, request: &Value, account: &str) -> Result<String, String> {
        let (_, signer) = provider::account_ref(account)?;
        let program = registry::parse_account(request["program"].as_str().unwrap_or_default())?;
        let message = registry::unhex(request["instruction"].as_str().unwrap_or_default())?;
        if message.is_empty() {
            return Err("this request has nothing to send".to_owned());
        }
        let mut mentions = vec![Mention { account: signer, program, seed: None, signs: true }];
        for reached in provider::stored_reached(request)? {
            let account = if reached.account == provider::SIGNER { signer } else { registry::parse_account(&reached.account)? };
            mentions.push(Mention {
                account,
                program: if reached.balance { registry::NATIVE_TOKEN } else { program },
                seed: reached.seed,
                signs: account == signer && reached.seed.is_none(),
            });
        }
        let change = Change { message, mentions, signer, needs: 0, extra: Value::Null };
        let mut setup = self.cli_setup()?;
        setup.wallet_ready()?;
        setup.registry = Some(registry::account_text(&program));
        wallet::send(&setup, &change)
    }

    /// Ends the requests whose payment job has ended. The app that asked is
    /// told only whether it went through; the CLI's own words, which may name
    /// the person's accounts and balances, go only to the Names app.
    fn provider_refresh(&mut self) {
        let jobs = self.jobs().ok();
        let now = provider::now_ms();
        let store = self.store();
        let mut changed = false;
        let mut tokens_moved = false;
        for request in store.requests.iter_mut().filter(|request| request["state"] == "running") {
            let state = request["job"].as_str().and_then(|job| jobs.as_ref()?.state(job).ok());
            let ended = match &state {
                None => Some((false, "the payment's record is gone; check the balance".to_owned())),
                Some(state) if state["state"] == "done" => Some((true, String::new())),
                Some(state) if state["state"] == "failed" => {
                    let log = state["log_tail"].as_str().unwrap_or_default();
                    let first = log.lines().map(str::trim).find(|line| line.starts_with("Error:")).unwrap_or_else(|| cli::last_line(log));
                    Some((false, provider::clean_text(first.trim_start_matches("Error:").trim(), 300)))
                }
                _ => None,
            };
            if let Some((done, problem)) = ended {
                request["state"] = json!(if done { "done" } else { "failed" });
                request["finished"] = json!(now);
                if !done {
                    request["error"] = json!("the payment didn't go through");
                    request["problem"] = json!(problem);
                }
                tokens_moved |= request["asset_kind"] == provider::TOKEN;
                changed = true;
            }
        }
        if changed {
            store.save();
        }
        if tokens_moved {
            self.forget_tokens();
        }
    }

    /// One account's balance for the app it was given to, read in the background.
    fn granted_balance(&self, account: &str) -> Value {
        let unreadable = |why: &str| json!({ "account": account, "account_kind": kind_of(account), "balance": null, "updating": false, "error": why });
        let Ok((private, id)) = provider::account_ref(account) else {
            return unreadable("not an account");
        };
        let read = if private {
            // Only the names CLI's own private account can be read here.
            let setup = match self.cli_setup() {
                Ok(setup) if cli::default_private(&setup).ok().as_deref() == Some(registry::account_text(&id).as_str()) => setup,
                _ => return unreadable("this private account's balance can't be read on this computer"),
            };
            let read = self.read_in_background("provider:cli_balance".to_owned(), BALANCE_EVERY, move || cli::run_balance(&setup));
            let balance = read["value"]["balance"].clone();
            merged(read, &json!({ "value": balance }))
        } else {
            let registry = Registry { sequencer: self.sequencer.clone(), program: self.program.unwrap_or([0; 32]) };
            self.read_in_background(format!("provider:balance:{}", registry::account_text(&id)), BALANCE_EVERY, move || {
                registry.balance(&id).map(|balance| json!(balance.to_string()))
            })
        };
        json!({
            "account": account,
            "account_kind": kind_of(account),
            "balance": read["value"],
            "updating": read["updating"],
            "error": read["error"],
        })
    }

    /// Answers at once with the last value `fetch` gave for `key`, `{value,
    /// updating, error}`, and starts a newer read on its own thread unless
    /// one is on the way or the last ended less than `every` ago.
    fn read_in_background(&self, key: String, every: std::time::Duration, fetch: impl FnOnce() -> Result<Value, String> + Send + 'static) -> Value {
        let mut reads = self.board_reads.lock().expect("the board reads lock");
        let read = reads.entry(key.clone()).or_default();
        if !read.updating && read.fetched.is_none_or(|at| at.elapsed() >= every) {
            read.updating = true;
            let reads = std::sync::Arc::clone(&self.board_reads);
            std::thread::spawn(move || {
                let fetched = fetch();
                let mut reads = reads.lock().expect("the board reads lock");
                let read = reads.entry(key).or_default();
                read.updating = false;
                read.fetched = Some(std::time::Instant::now());
                match fetched {
                    Ok(value) => {
                        read.value = Some(value);
                        read.error = None;
                    }
                    Err(error) => read.error = Some(error),
                }
            });
        }
        json!({ "value": read.value, "updating": read.updating, "error": read.error })
    }

    // ── Tokens ──────────────────────────────────────────────────────────

    /// The token program the token methods use, or why there is none.
    fn token_program_set(&self) -> Result<AccountId, String> {
        self.token_program.ok_or_else(|| {
            "no token program is set: enter its program ID under Tokens (no token program runs on the public testnet yet)".to_owned()
        })
    }

    /// The wallet's tokens and pending credits as last read, `{value:
    /// {holdings, pending}, updating, error}`, with a newer read started in
    /// the background when the last is older than `TOKENS_EVERY`. Kept per
    /// wallet and token program.
    fn token_read(&self) -> Result<(AccountId, Value), String> {
        let program = self.token_program_set()?;
        let setup = self.cli_setup()?;
        setup.wallet_ready()?;
        let key = format!("{TOKENS_KEY}{}:{}", registry::account_text(&program), setup.wallet.display());
        let read = self.read_in_background(key, TOKENS_EVERY, move || cli::run_token_list(&setup, &program));
        Ok((program, read))
    }

    /// Drops the token reads, after a token job ended or the program changed.
    fn forget_tokens(&self) {
        self.board_reads.lock().expect("the board reads lock").retain(|key, _| !key.starts_with(TOKENS_KEY));
    }

    /// What the token `definition` of `program` is, read through the CLI and
    /// kept for `READ_FRESH`.
    fn token_info_of(&mut self, program: &AccountId, definition: &AccountId) -> Result<Value, String> {
        let key = (*program, *definition);
        if let Some((info, at)) = self.token_info.get(&key) {
            if at.elapsed() < READ_FRESH {
                return Ok(info.clone());
            }
        }
        let info = cli::run_token_info(&self.cli_setup()?, program, definition)?;
        if self.token_info.len() >= MAX_CACHED {
            self.token_info.retain(|_, (_, at)| at.elapsed() < READ_FRESH);
            if self.token_info.len() >= MAX_CACHED {
                self.token_info.clear();
            }
        }
        self.token_info.insert(key, (info.clone(), std::time::Instant::now()));
        Ok(info)
    }
}

/// The prefix of the token reads' keys among the background reads.
const TOKENS_KEY: &str = "tokens:";
/// How often the token reads may run the CLI; each catches the wallet up.
const TOKENS_EVERY: std::time::Duration = std::time::Duration::from_secs(20);

/// The value of a reply this module made, or its error.
fn parsed_reply(text: &str) -> Result<Value, String> {
    let reply: Value = serde_json::from_str(text).map_err(|_| "an unreadable answer".to_owned())?;
    if reply["success"] == true {
        Ok(reply["value"].clone())
    } else {
        Err(reply["error"].as_str().unwrap_or("it failed").to_owned())
    }
}

/// The most names, and the most answers, the module keeps in memory at once.
const MAX_CACHED: usize = 512;

/// A wallet can hold many accounts and each costs a read, so `wallet` stops here.
const MAX_WALLET_ACCOUNTS: usize = 64;

/// The Program Registry module (`program_registry`, LP-0023), asked what a
/// program is and whether its source is verified, for a program call's
/// prompt. It is optional: without it the prompt says so.
#[cfg(not(test))]
mod program_registry {
    use super::{json, Value};
    use logos_rust_sdk::LogosModuleSDK;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    /// Asking is also how a missing module is noticed, so it gives up quickly.
    const LOOK: Duration = Duration::from_secs(3);

    /// `program_label`'s value for `program` (base58), or None when the
    /// module isn't there or couldn't say. After one goes unanswered it isn't
    /// asked again for a minute, so a missing registry costs one wait, not one
    /// per request.
    pub fn label(program: &str) -> Option<Value> {
        static ABSENT_UNTIL: AtomicU64 = AtomicU64::new(0);
        let now = super::now();
        if now < ABSENT_UNTIL.load(Ordering::Relaxed) {
            return None;
        }
        match LogosModuleSDK::new().plugin("program_registry").call_json_with_timeout("program_label", &json!([program]), LOOK) {
            Ok(answer) => {
                // It answers `{success, value, error}`, perhaps as JSON text.
                let answer = match answer.as_str().map(serde_json::from_str::<Value>) {
                    Some(Ok(parsed)) => parsed,
                    _ => answer,
                };
                (answer["success"] == true).then(|| answer["value"].clone()).filter(Value::is_object)
            }
            Err(_) => {
                ABSENT_UNTIL.store(now + 60, Ordering::Relaxed);
                None
            }
        }
    }

    /// `verify_app`'s value for the Basecamp module `app`: whether its package is really
    /// published by the LNS name it names. The first check of a name derives it (Argon2, about
    /// a second) and reads its record, so it may take a few seconds.
    pub fn publisher(app: &str) -> Option<Value> {
        let answer = LogosModuleSDK::new()
            .plugin("program_registry")
            .call_json_with_timeout("verify_app", &json!([app]), Duration::from_secs(15))
            .ok()?;
        let answer = match answer.as_str().map(serde_json::from_str::<Value>) {
            Some(Ok(parsed)) => parsed,
            _ => answer,
        };
        (answer["success"] == true).then(|| answer["value"].clone()).filter(Value::is_object)
    }
}

/// Sends made from this device, by hash: `{from, inputs, fee, at}`, for `chain_tx`.
/// Kept on disk, so a send is still followed after the module restarts.
fn chain_sent() -> Map<String, Value> {
    config_dir().map(|dir| load_object(&dir.join("chain-sent.json"))).unwrap_or_default()
}

/// Remembers a send; past `MAX_CACHED` sends the oldest is forgotten.
fn chain_sent_add(hash: &str, sent: Value) {
    let Some(dir) = config_dir() else { return };
    let mut all = chain_sent();
    while all.len() >= MAX_CACHED {
        let Some(oldest) = all.iter().min_by_key(|(_, s)| s["at"].as_u64().unwrap_or(0)).map(|(h, _)| h.clone()) else { break };
        all.remove(&oldest);
    }
    all.insert(hash.to_owned(), sent);
    save_object(&dir.join("chain-sent.json"), &all);
}

/// Basecamp's Logos Blockchain node (`blockchain_module`), for the chain methods.
#[cfg(not(test))]
mod node {
    use super::{chain, Value};
    use logos_rust_sdk::LogosModuleSDK;
    use std::time::Duration;

    /// A read: the node answers from its own state.
    pub const QUICK: Duration = Duration::from_secs(15);
    /// A transfer or claim, which the node proves before it answers.
    pub const PROVING: Duration = Duration::from_secs(180);

    pub fn call(method: &str, args: Value, wait: Duration) -> Result<Value, String> {
        let answer = LogosModuleSDK::new()
            .plugin(chain::MODULE)
            .call_json_with_timeout(method, &args, wait)
            .map_err(|_| "Basecamp's Logos Blockchain node didn't answer. Install the Blockchain app and start its node.".to_owned())?;
        chain::unwrap(method, answer)
    }
}

/// Stands in for the node in unit tests: a test sets each method's answer, and
/// what was asked is kept.
#[cfg(test)]
mod node {
    use super::{chain, Value};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::Duration;

    pub const QUICK: Duration = Duration::from_secs(15);
    pub const PROVING: Duration = Duration::from_secs(180);

    thread_local! {
        pub static ANSWERS: RefCell<HashMap<String, Value>> = RefCell::new(HashMap::new());
        pub static ASKED: RefCell<Vec<(String, Value)>> = const { RefCell::new(Vec::new()) };
    }

    pub fn call(method: &str, args: Value, _wait: Duration) -> Result<Value, String> {
        ASKED.with(|asked| asked.borrow_mut().push((method.to_owned(), args)));
        let answer = ANSWERS.with(|answers| answers.borrow().get(method).cloned());
        chain::unwrap(method, answer.ok_or_else(|| "Basecamp's Logos Blockchain node didn't answer.".to_owned())?)
    }
}

/// Stands in for the Program Registry in unit tests: a test sets the label.
#[cfg(test)]
mod program_registry {
    use super::Value;

    thread_local! {
        pub static LABEL: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
    }

    pub fn label(_program: &str) -> Option<Value> {
        LABEL.with(|label| label.borrow().clone())
    }

    thread_local! {
        pub static PUBLISHER: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
    }

    pub fn publisher(_app: &str) -> Option<Value> {
        PUBLISHER.with(|publisher| publisher.borrow().clone())
    }
}

#[cfg(not(test))]
#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<Names>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).expect("the module answers in JSON")
    }

    /// A module with the built-in defaults, ignoring any saved configuration.
    fn fresh() -> Names {
        Names {
            sequencer: DEFAULT_SEQUENCER.to_owned(),
            program: None,
            derived: HashMap::new(),
            wallet_files: None,
            cli: None,
            cli_wallet: None,
            jobs_dir: None,
            bin_dir: None,
            follows: Vec::new(),
            main_name: None,
            bridge_key: None,
            bridge_node: None,
            likes: Vec::new(),
            dm_names: Default::default(),
            chat_names: Default::default(),
            board_reads: Default::default(),
            read_cache: HashMap::new(),
            provider: None,
            provider_path: None,
            token_program: None,
            token_info: HashMap::new(),
            publishers: HashMap::new(),
        }
    }

    #[test]
    fn questions_fail_clearly_until_a_registry_is_configured() {
        let mut names = fresh();
        for answer in [names.recipient("jef.lez".into()), names.deposit("jef".into()), names.settings()] {
            let answer = parsed(&answer);
            assert_eq!(answer["success"], false);
            assert!(answer["error"].as_str().expect("an error").contains("set_registry"));
        }
    }

    #[test]
    fn the_default_registry_is_v5_and_a_retired_one_is_not_kept() {
        let v5 = registry::parse_account(DEFAULT_REGISTRY).expect("an account");
        let mut names = fresh();
        names.program = Some(v5);
        names.apply_saved(&json!({ "registry": RETIRED_REGISTRIES[0], "sequencer": "http://127.0.0.1:3040" }));
        assert_eq!(names.program, Some(v5));
        assert_eq!(names.sequencer, "http://127.0.0.1:3040");
        let other = registry::account_text(&[7; 32]);
        names.apply_saved(&json!({ "registry": other }));
        assert_eq!(names.program, Some([7; 32]));
    }

    #[test]
    fn a_bad_name_is_refused_before_any_network_call() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        let answer = parsed(&names.resolve("jef.eth".into()));
        assert_eq!(answer["success"], false);
        assert_eq!(names.normalize("Jef.LEZ".into()), "jef");
        assert_eq!(names.normalize("jef.eth".into()), "");
    }

    #[test]
    fn a_change_to_a_bad_name_is_refused_before_any_network_call() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        for answer in [
            names.register_name("jef.eth".into(), "x".into(), String::new()),
            names.release("-jef".into()),
            names.set_primary("x".into(), "not a name".into()),
            names.receive_privately("j f".into(), "auto".into()),
            names.set_profile("j f".into(), "{}".into(), true),
        ] {
            let answer = parsed(&answer);
            assert_eq!(answer["success"], false);
            assert!(answer["error"].as_str().expect("an error").contains("1-32 characters"));
        }
    }

    #[test]
    fn profile_entries_are_read_from_a_json_object() {
        assert_eq!(
            parse_entries(r#"{"display": "Jef", "bio": "", "cid": null}"#),
            Ok(vec![
                ("bio".to_owned(), String::new()),
                ("cid".to_owned(), String::new()),
                ("display".to_owned(), "Jef".to_owned()),
            ])
        );
        assert!(parse_entries("[1]").is_err());
        assert!(parse_entries(r#"{"bio": 3}"#).is_err());
        assert!(parse_entries("not json").is_err());
        let mut names = fresh();
        names.program = Some([7; 32]);
        let answer = parsed(&names.set_profile("jef".into(), "[]".into(), false));
        assert!(answer["error"].as_str().expect("an error").contains("JSON object"));
    }

    #[test]
    fn deposits_are_reported_by_tier_and_mode() {
        let deposits = Settings {
            admin: [1; 32],
            treasury: [2; 32],
            prices: [10, 20, 30],
            mode: registry::MODE_DEPOSIT,
            deposits: [100, 2_000],
            claims_root: [7; 32],
            claims_until: 50,
            claims_program: [8; 32],
        };
        let normal = deposit_json("logos", shortlist::TIER_NORMAL, Some(&deposits), 49);
        assert_eq!(normal["claims_until"], 50, "while claims are open");
        assert_eq!(normal["tier"], "normal");
        assert_eq!(normal["deposit"], "100");
        assert_eq!(normal["mode"], "deposit");
        assert_eq!(normal["display"], "logos");
        assert_eq!(deposit_json("jef", shortlist::TIER_THREE, Some(&deposits), 49)["deposit"], "2000");
        let locked = deposit_json("ab", shortlist::TIER_LOCKED, Some(&deposits), 49);
        assert_eq!((locked["tier"].clone(), locked["deposit"].clone()), (json!("locked"), Value::Null));
        let fees = Settings { mode: registry::MODE_FEE, ..deposits.clone() };
        let fee = deposit_json("logos", shortlist::TIER_NORMAL, Some(&fees), 50);
        assert_eq!(fee["claims_until"], Value::Null, "and not after");
        assert_eq!((fee["mode"].clone(), fee["deposit"].clone(), fee["fee"].clone()), (json!("fee"), json!("0"), json!("20")));
        assert_eq!(deposit_json("logos", shortlist::TIER_NORMAL, None, 49)["mode"], "unconfigured");

        let settings = settings_json(&deposits, 49);
        assert_eq!(settings["mode"], "deposit");
        assert_eq!(settings["deposits"]["normal"], "100");
        assert_eq!(settings["deposits"]["three"], "2000");
        assert_eq!(settings["admin"], registry::account_text(&[1; 32]));
    }

    #[test]
    fn a_commitment_is_kept_and_its_timing_follows_the_program() {
        let saved = Commitment {
            owner: [1; 32],
            secret: [5; 32],
            made: 1_000_000,
            private_account: "auto".into(),
            revealed: Some("ab".into()),
            revealed_at: Some(2_000_000),
        };
        assert_eq!(Commitment::from_json(&saved.to_json()), Some(saved.clone()));
        assert_eq!(saved.ready_at(), 1_000_000 + 15 * 60 * 1000);
        assert!(saved.usable_at(1_000_000 + registry::MAX_COMMIT_AGE_MS - registry::TIME_WINDOW_MS - 1));
        assert!(!saved.usable_at(1_000_000 + registry::MAX_COMMIT_AGE_MS - registry::TIME_WINDOW_MS));

        let path = std::env::temp_dir().join(format!("logos-names-pending-{}-{}.json", std::process::id(), now()));
        let mut pending = Pending::load(path.clone());
        pending.put(&[7; 32], "jef", &saved);
        pending.save();
        let reloaded = Pending::load(path.clone());
        assert_eq!(reloaded.get(&[7; 32], "jef"), Some(saved));
        assert_eq!(reloaded.get(&[8; 32], "jef"), None);
        assert_eq!(reloaded.all(&[7; 32]).len(), 1);
        let mut reloaded = reloaded;
        reloaded.remove(&[7; 32], "jef");
        assert!(reloaded.all(&[7; 32]).is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extras_are_merged_into_the_answer() {
        let value = merged(json!({ "name": "jef", "tx": "ab" }), &json!({ "refund": "100" }));
        assert_eq!(value, json!({ "name": "jef", "tx": "ab", "refund": "100" }));
        assert_eq!(merged(json!({ "name": "jef" }), &Value::Null), json!({ "name": "jef" }));
    }

    #[test]
    fn a_name_is_derived_once_and_then_remembered() {
        let mut names = fresh();
        let first = names.derived("jef");
        assert_eq!(first, registry::derive("jef"));
        assert_eq!(names.derived.len(), 1);
        assert_eq!(names.derived("jef"), first);
        assert_eq!(names.derived.len(), 1);
    }

    #[test]
    fn the_device_remembers_names_per_registry() {
        let path = std::env::temp_dir().join(format!("logos-names-test-{}-{}.json", std::process::id(), now()));
        let mut memory = Memory::load(path.clone());
        assert!(memory.names(&[7; 32]).is_empty());
        memory.remember(&[7; 32], "jef");
        memory.remember(&[7; 32], "ada");
        memory.remember(&[7; 32], "jef");
        memory.remember(&[8; 32], "linus");
        memory.save();

        let reloaded = Memory::load(path.clone());
        let names: Vec<String> = reloaded.names(&[7; 32]).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["jef", "ada"]);
        assert_eq!(reloaded.names(&[8; 32]).len(), 1);
        assert!(reloaded.names(&[9; 32]).is_empty());
        let _ = std::fs::remove_file(path);
    }

    /// A module whose CLI is a script that prints its arguments, one `[arg]`
    /// per line, then runs `tail`; with a wallet and jobs directory of its own.
    fn with_fake_cli(label: &str, tail: &str) -> (Names, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("logos-names-module-{label}-{}-{}", std::process::id(), now()));
        let wallet = dir.join("wallet");
        std::fs::create_dir_all(&wallet).expect("a temp dir");
        std::fs::write(wallet.join(cli::STORAGE_FILE), "{}").expect("a wallet");
        let script = dir.join("names");
        std::fs::write(&script, format!("#!/bin/sh\nfor arg in \"$@\"; do printf '[%s]\\n' \"$arg\"; done\n{tail}\n")).expect("a script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("executable");
        let mut names = fresh();
        names.program = Some([7; 32]);
        names.cli = Some(script.to_string_lossy().into_owned());
        names.cli_wallet = Some(wallet.to_string_lossy().into_owned());
        names.jobs_dir = Some(dir.join("jobs"));
        (names, dir)
    }

    fn job_end(names: &mut Names, id: &str) -> Value {
        for _ in 0..200 {
            let state = parsed(&names.job(id.to_owned()));
            assert_eq!(state["success"], true, "{state}");
            if state["value"]["state"] != "running" {
                return state["value"].clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("job {id} did not finish");
    }

    #[test]
    fn cli_changes_run_as_jobs_with_the_module_registry() {
        let (mut names, dir) = with_fake_cli("jobs", "exit 0");
        let started = parsed(&names.cli_set_text("Jef.lez".into(), "chat".into(), "say \"hi\"\nand $(id)".into()));
        assert_eq!(started["success"], true, "{started}");
        assert_eq!(started["value"]["state"], "running");
        let id = started["value"]["id"].as_str().expect("an id").to_owned();
        assert!(id.starts_with("jef-"));
        let done = job_end(&mut names, &id);
        assert_eq!(done["state"], "done");
        let program = registry::account_text(&[7; 32]);
        assert_eq!(
            done["log_tail"],
            format!("[--program={program}]\n[--rpc={DEFAULT_SEQUENCER}]\n[set-text]\n[--]\n[jef]\n[chat]\n[say \"hi\"\nand $(id)]")
        );

        let page = parsed(&names.cli_set_page("jef".into(), "text/plain; charset=utf-8".into(), "line 1\n-- line 2".into()));
        let done = job_end(&mut names, page["value"]["id"].as_str().expect("an id"));
        let tail = done["log_tail"].as_str().expect("a log");
        assert!(tail.contains("[set-page]\n[--content-type=text/plain; charset=utf-8]\n[--]\n[jef]\n["), "{tail}");
        let cleared = parsed(&names.cli_set_page("jef".into(), "text/html".into(), String::new()));
        let done = job_end(&mut names, cleared["value"]["id"].as_str().expect("an id"));
        assert!(done["log_tail"].as_str().expect("a log").ends_with("[clear-page]\n[--]\n[jef]"));

        let profile = parsed(&names.cli_profile("jef".into(), r#"{"display": "Jef", "bio": "", "avatar": "data:image/png;base64,AA", "chat": "x=y"}"#.into(), false));
        assert_eq!(profile["value"]["skipped"], json!(["avatar"]));
        let done = job_end(&mut names, profile["value"]["id"].as_str().expect("an id"));
        assert!(done["log_tail"].as_str().expect("a log").ends_with("[profile]\n[--clear=bio]\n[--set=chat=x=y]\n[--display=Jef]\n[--]\n[jef]"), "{}", done["log_tail"]);

        let config = parsed(&names.get_config());
        assert_eq!(config["cli"], names.cli.clone().expect("set"));
        assert_eq!(config["cli_wallet"], names.cli_wallet.clone().expect("set"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cli_refusal_ends_the_job_failed_with_its_reason() {
        let (mut names, dir) = with_fake_cli("refusal", "echo 'Error: this wallet does not hold jef.lez' >&2; exit 1");
        let started = parsed(&names.cli_set_text("jef".into(), "chat".into(), "x".into()));
        let id = started["value"]["id"].as_str().expect("an id").to_owned();
        let failed = job_end(&mut names, &id);
        assert_eq!(failed["state"], "failed");
        assert_eq!(cli::last_line(failed["log_tail"].as_str().expect("a log")), "Error: this wallet does not hold jef.lez");
        assert!(parsed(&names.cli_mine())["error"].as_str().expect("an error").contains("does not hold"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cli_changes_check_their_input_before_running_anything() {
        let (mut names, dir) = with_fake_cli("input", "exit 0");
        for answer in [
            names.cli_set_text("jef; rm -rf ~".into(), "chat".into(), "x".into()),
            names.cli_set_text("$(id)".into(), "chat".into(), "x".into()),
            names.cli_set_page("a|b".into(), "text/plain".into(), "x".into()),
            names.cli_profile("jef`id`".into(), r#"{"bio": "x"}"#.into(), false),
            names.cli_register("--owner=x".into(), false),
        ] {
            assert!(parsed(&answer)["error"].as_str().expect("an error").contains("1-32 characters"));
        }
        assert_eq!(parsed(&names.cli_set_text("jef".into(), "Bad=Key".into(), "x".into()))["success"], false);
        assert_eq!(parsed(&names.cli_profile("jef".into(), r#"{"theme": "neon"}"#.into(), false))["success"], false);
        assert_eq!(parsed(&names.cli_set_page("jef".into(), "text/html\n--x".into(), "x".into()))["success"], false);
        assert!(parsed(&names.job("../../etc/passwd".into()))["error"].as_str().expect("an error").contains("not a job id"));
        // Nothing was started.
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_cli_wallet_holds_nothing_and_takes_no_changes() {
        let (mut names, dir) = with_fake_cli("empty", "echo 'jef.lez  held by a deposit (no expiry)'");
        assert_eq!(parsed(&names.cli_mine())["value"], json!([{ "name": "jef", "display": "jef", "signer": "cli", "state": "held by a deposit (no expiry)", "lapsed": false, "registering": false, "failed": false, "private": null }]));
        let empty = dir.join("empty-wallet");
        std::fs::create_dir_all(&empty).expect("a dir");
        names.cli_wallet = Some(empty.to_string_lossy().into_owned());
        assert_eq!(parsed(&names.cli_mine())["value"], json!([]));
        let refused = parsed(&names.cli_set_text("jef".into(), "chat".into(), "x".into()));
        assert!(refused["error"].as_str().expect("an error").contains("create your wallet"));
        // The CLI was not run, so it made no wallet there.
        assert_eq!(std::fs::read_dir(&empty).expect("the dir").count(), 0);
        assert!(!names.set_cli("relative/names".into()));
        assert!(!names.set_cli(dir.join("wallet").join(cli::STORAGE_FILE).to_string_lossy().into_owned()));
        assert!(!names.set_cli_wallet("/no/such/dir".into()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_bridge_key_and_node_are_kept_only_when_valid() {
        let key = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
        let mut names = fresh();
        names.apply_saved(&json!({ "bridge_key": key.to_uppercase(), "bridge_node": "http://localhost:18080/" }));
        assert_eq!(names.bridge_key.as_deref(), Some(key));
        assert_eq!(names.bridge_node.as_deref(), Some("http://localhost:18080"));
        let config = parsed(&names.get_config());
        assert_eq!((config["bridge_key"].clone(), config["bridge_node"].clone()), (json!(key), json!("http://localhost:18080")));
        // An edited file can't smuggle in another host or a malformed key.
        names.apply_saved(&json!({ "bridge_key": "../../etc", "bridge_node": "http://evil.example:8080" }));
        assert_eq!((names.bridge_key.clone(), names.bridge_node.clone()), (None, None));
        // A bad argument is refused before the CLI is looked for or anything is
        // saved. Only the plan is asked for, and of a CLI that isn't there, so
        // this test can never run a real deposit.
        names.cli = Some("/no/such/names".into());
        names.cli_wallet = Some(std::env::temp_dir().join("logos-names-no-wallet").to_string_lossy().into_owned());
        for (amount, from_key, node) in [("0", key, ""), ("5", "nope", ""), ("5", key, "http://10.0.0.2:8080"), ("1e3", key, "")] {
            let answer = parsed(&names.cli_bridge(amount.into(), from_key.into(), node.into(), false));
            assert_eq!(answer["success"], false, "{amount} {from_key} {node}");
            let error = answer["error"].as_str().unwrap_or_default().to_owned();
            assert!(!error.contains("names CLI") && !error.contains("wallet"), "{error}");
        }
        assert_eq!((names.bridge_key.clone(), names.bridge_node.clone()), (None, None));
    }

    #[test]
    fn configuration_rejects_malformed_values() {
        let mut names = fresh();
        assert!(!names.set_registry("not-an-account".into()));
        assert!(!names.set_sequencer("ftp://example.test".into()));
        assert_eq!(parsed(&names.get_config())["sequencer"], DEFAULT_SEQUENCER);
        assert_eq!(parsed(&names.get_config())["registry"], Value::Null);
        assert!(is_http_url("http://127.0.0.1:3040"));
        assert!(!is_http_url("ftp://example.test"));
    }

    // ── Who may change things (open-review FR-1, FR-3, FR-6) ────────────

    #[test]
    fn plain_http_is_only_for_this_machine() {
        for local in ["http://localhost:3040", "http://127.0.0.1", "http://[::1]:3040/rpc"] {
            assert!(is_http_url(local), "{local}");
        }
        for remote in ["http://testnet.lez.logos.co", "http://evil.test/", "http://localhost.evil.test", "http://127.0.0.1@evil.test", "https://"] {
            assert!(!is_http_url(remote), "{remote}");
        }
        // userinfo before the host is skipped, so it can't smuggle a local-looking prefix
        assert!(!is_http_url("http://localhost@evil.test:3040"));
        assert!(is_http_url("https://testnet.lez.logos.co"));
        let mut names = fresh();
        assert!(!names.set_sequencer("http://evil.test".into()));
        assert_eq!(parsed(&names.get_config())["sequencer"], DEFAULT_SEQUENCER);
    }

    fn as_caller(caller: logos_rust_sdk::LogosCaller) {
        TEST_CALLER.with(|current| *current.borrow_mut() = caller);
    }

    // ── The Logos Blockchain, against a stand-in node ──────────────────

    const NODE_KEY: &str = "66ecf4bccaa13e4e03c3d5c0e3ebecb945c51bbf815e92727a9a4efa6b39a81b";

    /// Answers `method` as the node module does: `{success, value, error}`, the value as JSON text.
    fn node_answers(method: &str, value: Value) {
        let value = if value.is_string() { value } else { Value::from(value.to_string()) };
        node::ANSWERS.with(|answers| answers.borrow_mut().insert(method.to_owned(), json!({ "success": true, "value": value, "error": null })));
    }

    fn node_asked() -> Vec<String> {
        node::ASKED.with(|asked| asked.borrow().iter().map(|(method, _)| method.clone()).collect())
    }

    #[test]
    fn chain_accounts_lists_the_nodes_keys_with_balances() {
        let mut names = fresh();
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("wallet_get_known_addresses".into(), json!({ "success": true, "value": [NODE_KEY, "not-a-key"] })));
        node_answers("wallet_get_balance", json!("1191220464"));
        let shown = parsed(&names.chain_accounts());
        assert_eq!(shown["value"], json!([{ "key": NODE_KEY, "balance": "1191220464" }]), "{shown}");
        // A key the node lists but its wallet doesn't hold, as the M2 node does with three of six.
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("wallet_get_balance".into(), json!({ "success": false, "error": "Unknown wallet address." })));
        assert_eq!(parsed(&names.chain_accounts())["value"], json!([]));
    }

    #[test]
    fn chain_send_plans_then_sends_and_refuses_overdrafts() {
        let mut names = fresh();
        let to = "ab".repeat(32);
        node_answers("wallet_get_balance", json!("5000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "b271", "value": "5000" }] }));
        node_answers("wallet_transfer_funds", json!("cd".repeat(32)));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["b271"], "outputs": [{ "value": 1000 }, { "value": 3946 }] } }] } }));
        let plan = parsed(&names.chain_send(NODE_KEY.into(), to.clone(), "1,000".into(), false));
        assert_eq!(plan["value"]["amount"], "1000", "{plan}");
        assert_eq!(plan["value"]["hash"], Value::Null, "a plan sends nothing");
        assert!(!node_asked().contains(&"wallet_transfer_funds".to_owned()));
        let sent = parsed(&names.chain_send(NODE_KEY.into(), to.clone(), "1000".into(), true));
        assert_eq!(sent["value"]["hash"], "cd".repeat(32), "{sent}");
        assert_eq!(sent["value"]["fee"], "54", "5000 in, 1000 + 3946 out");
        // Pending while its input is still in the wallet, in a block once it's gone.
        assert_eq!(parsed(&names.chain_tx("cd".repeat(32)))["value"]["state"], "pending");
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "c0de", "value": "3946" }] }));
        let landed = parsed(&names.chain_tx("cd".repeat(32)));
        assert_eq!(landed["value"], json!({ "hash": "cd".repeat(32), "state": "in_block", "fee": "54" }));
        // The module reloaded (a verifier's find: the send was held in memory only).
        let mut reloaded = fresh();
        assert_eq!(parsed(&reloaded.chain_tx("cd".repeat(32)))["value"], landed["value"], "a send is followed after a restart");
        let asked = node::ASKED.with(|asked| asked.borrow().iter().find(|(m, _)| m == "wallet_transfer_funds").map(|(_, a)| a.clone()));
        assert_eq!(asked, Some(json!([NODE_KEY, [NODE_KEY], to, "1000", ""])), "change goes back to the sender");
        let over = parsed(&names.chain_send(NODE_KEY.into(), "ab".repeat(32), "5001".into(), true));
        assert_eq!(over["success"], false);
        assert!(over["error"].as_str().unwrap().contains("less than 5001"), "{over}");
    }

    #[test]
    fn chain_claim_with_nothing_to_claim_says_so_and_sends_nothing() {
        let mut names = fresh();
        node_answers("wallet_get_claimable_vouchers", json!({ "reward_amount": "13140", "total_claimable": "0", "vouchers": [] }));
        let plan = parsed(&names.chain_claim(false));
        assert_eq!(plan["value"], json!({ "claimable": "0", "vouchers": 0, "reward_per_voucher": "13140" }), "{plan}");
        let claimed = parsed(&names.chain_claim(true));
        assert_eq!(claimed["success"], false, "{claimed}");
        assert_eq!(claimed["error"], "There is nothing to claim: your node has no leader rewards waiting.");
        assert!(!node_asked().contains(&"leader_claim".to_owned()));
    }

    /// As a solo 0.3.4 node claimed over logosctl: one voucher of 1722, the fee paid
    /// from the leader funding key (5000 in, 2972 back).
    #[test]
    fn chain_claim_claims_one_voucher_and_reads_its_fee() {
        let mut names = fresh();
        let funding = "e01a".repeat(16);
        node_answers("wallet_get_claimable_vouchers", json!({ "reward_amount": "38", "total_claimable": "76", "vouchers": [{ "commitment": "c1", "nullifier": "n1" }, { "commitment": "c2", "nullifier": "n2" }] }));
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("wallet_get_known_addresses".into(), json!({ "success": true, "value": [NODE_KEY, funding] })));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "af49", "value": "5000" }] }));
        node_answers("leader_claim", json!("a4".repeat(32)));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [
            { "opcode": 48, "payload": { "pk": funding, "voucher_nullifier": "n1" } },
            { "opcode": 0, "payload": { "inputs": ["af49"], "outputs": [{ "pk": funding, "value": 2972 }] } },
        ] } }));
        let claimed = parsed(&names.chain_claim(true));
        assert_eq!(claimed["value"]["hash"], "a4".repeat(32), "{claimed}");
        assert_eq!(claimed["value"]["claimed"], "38");
        assert_eq!(claimed["value"]["fee"], "2028");
        assert_eq!(claimed["value"]["vouchers_left"], 1);
        assert_eq!(claimed["value"]["claimable_left"], "38");
        assert!(claimed["value"]["warning"].as_str().unwrap().contains("fee of 2028 for a reward of 38"), "{claimed}");
        let asked = node_asked();
        let notes_at = asked.iter().position(|m| m == "wallet_get_notes").unwrap();
        assert!(notes_at < asked.iter().position(|m| m == "leader_claim").unwrap(), "the notes are read before the claim: {asked:?}");
    }

    /// A hash this device didn't send fails: with the node's own reason, or saying the
    /// app can't follow it (a verifier's find: it answered success, state unknown).
    #[test]
    fn chain_tx_of_a_send_made_elsewhere_fails_plainly() {
        let mut names = fresh();
        let other = "ef".repeat(32);
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("get_transaction".into(), json!({ "success": false, "error": "No transaction found for hash TxHash([239, 239])" })));
        let missing = parsed(&names.chain_tx(other.clone()));
        assert_eq!(missing["success"], false, "{missing}");
        assert_eq!(missing["error"], "Your node has no record of that transaction, and it wasn't sent from the Names app: check the hash.");
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [] } }));
        let elsewhere = parsed(&names.chain_tx(other.clone()));
        assert_eq!(elsewhere["error"], "That transaction wasn't sent from the Names app, so the app can't tell whether it is in a block.", "{elsewhere}");
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("get_transaction".into(), json!({ "success": false, "error": "The node is not running." })));
        assert!(parsed(&names.chain_tx(other))["error"].as_str().unwrap().contains("isn't running"));
    }

    #[test]
    fn chain_methods_are_the_names_apps_alone_and_say_when_the_node_is_down() {
        let mut names = fresh();
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("get_cryptarchia_info".into(), json!({ "success": false, "error": "The node is not running." })));
        let down = parsed(&names.chain_status());
        assert!(down["error"].as_str().unwrap().contains("isn't running"), "{down}");
        as_caller(logos_rust_sdk::LogosCaller::Module { name: "evil_ui".into(), instance: None });
        for refused in [names.chain_accounts(), names.chain_send(NODE_KEY.into(), "ab".repeat(32), "1".into(), true), names.chain_claim(true)] {
            assert!(parsed(&refused)["error"].as_str().unwrap().contains("only the Names app"), "{refused}");
        }
        assert!(node_asked().iter().all(|m| m == "get_cryptarchia_info"), "nothing was asked for the other app");
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
    }

    #[test]
    fn only_the_names_app_and_the_host_may_act() {
        use logos_rust_sdk::LogosCaller;
        assert!(may_act(&LogosCaller::HostAnchor));
        assert!(may_act(&LogosCaller::Module { name: "names_ui".into(), instance: None }));
        assert!(may_act(&LogosCaller::Module { name: "names_ui".into(), instance: Some("a41f".into()) }));
        assert!(!may_act(&LogosCaller::Module { name: "evil_ui".into(), instance: None }));
        assert!(!may_act(&LogosCaller::Module { name: "whisperbox".into(), instance: None }));
        assert!(!may_act(&LogosCaller::Unknown));
        assert!(!may_act(&LogosCaller::Derived { parent: "names_ui".into(), leaf: "x".into() }));
    }

    #[test]
    fn another_app_can_read_names_but_not_pay_or_repoint_the_module() {
        let (mut names, dir) = with_fake_cli("other-app", "echo should-not-run > \"$(dirname \"$0\")/ran\"");
        as_caller(logos_rust_sdk::LogosCaller::Module { name: "evil_ui".into(), instance: None });
        let paid = parsed(&names.cli_pay("victim".into(), "1000000".into()));
        assert_eq!(paid["success"], false, "{paid}");
        assert!(paid["error"].as_str().unwrap().contains("only the Names app"), "{paid}");
        for refused in [names.cli_create_wallet(), names.set_main_name("attacker".into()), names.set_chat_name("peer".into(), "attacker".into()),
                        names.cli_status(), names.wallet(), names.job("victim-1".into()), names.board_feed(true)] {
            assert_eq!(parsed(&refused)["success"], false, "{refused}");
        }
        assert!(!names.set_cli("/bin/sh".into()));
        assert!(!names.set_sequencer("https://evil.test".into()));
        assert!(!names.set_cli_wallet(dir.to_string_lossy().into_owned()));
        assert!(!names.set_wallet_files(String::new(), String::new()));
        assert!(!names.set_registry(registry::account_text(&[9; 32])));
        assert_eq!(parsed(&names.get_config())["sequencer"], DEFAULT_SEQUENCER);
        assert!(!dir.join("ran").exists(), "the CLI must not have run");
        // Reading stays open to every app.
        assert_eq!(names.normalize("@Jef".into()), "jef");
        assert_eq!(parsed(&names.page_preview("<p>hi</p>".into()))["success"], true);
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
    }

    #[test]
    fn the_cli_and_wallet_must_be_this_users_own() {
        use std::os::unix::fs::PermissionsExt;
        let (mut names, dir) = with_fake_cli("owned", "exit 0");
        let script = dir.join("names");
        assert!(names.set_cli(script.to_string_lossy().into_owned()));
        // writable by others: refused
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(!names.set_cli(script.to_string_lossy().into_owned()));
        // a relative path: refused
        assert!(!names.set_cli("names".into()));
        // a system file is not a wallet
        assert!(!names.set_wallet_files("/etc/hosts".into(), "/etc/hosts".into()));
        // a saved path that fails the checks is dropped on load
        names.apply_saved(&json!({ "cli": script.to_string_lossy(), "cli_wallet": "/etc" }));
        assert_eq!(names.cli, None);
        assert_eq!(names.cli_wallet, None);
    }

    #[test]
    fn private_files_are_this_users_only_and_replace_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("logos-names-private-{}-{}", std::process::id(), now()));
        let file = dir.join("config.json");
        cli::write_private(&file, b"{}").unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        // a symlink planted where the file goes is replaced, not written through
        let target = dir.join("elsewhere");
        std::fs::write(&target, "untouched").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&target, &file).unwrap();
        cli::write_private(&file, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched");
        assert!(!std::fs::symlink_metadata(&file).unwrap().file_type().is_symlink());
        assert!(cli::create_private(&target).is_err(), "never opens an existing file");
        // old 0644 files are closed up
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        tighten(&dir);
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn oversized_records_and_pages_are_not_believed() {
        let derived = registry::derive("jef");
        let huge = vec![0_u8; registry::MAX_SEALED_RECORD_LEN + 1];
        assert_eq!(registry::Profile::open(&derived.key, &huge, "jef").texts.len(), 0);
        assert!(registry::open_page(&derived.key, &vec![0_u8; registry::MAX_SEALED_PAGE_LEN + 1]).is_none());
    }

    #[test]
    fn caches_stay_bounded() {
        let mut names = fresh();
        let one = registry::derive("jef");
        for i in 0..MAX_CACHED {
            names.derived.insert(format!("n{i}"), one.clone());
        }
        names.derived("another");
        assert!(names.derived.len() <= MAX_CACHED);
    }

    // ── Getting started without a terminal ──────────────────────────────

    #[test]
    fn install_puts_the_cli_in_the_modules_folder_and_uses_it() {
        let (mut names, dir) = with_fake_cli("install", "exit 0");
        names.bin_dir = Some(dir.join("data/bin"));
        // A configured CLI that runs is left alone: nothing is installed.
        std::fs::write(dir.join("names"), "#!/bin/sh\necho 'names 0.5.0'\n").unwrap();
        let kept = parsed(&names.cli_install());
        assert_eq!(kept["value"]["installed"], true, "{kept}");
        assert_eq!(kept["value"]["version"], "names 0.5.0");
        assert_eq!(kept["value"]["job"], Value::Null);
        assert!(!dir.join("data/bin").exists());
        // One that doesn't run is replaced by an install here.
        names.cli = Some("/no/such/names".into());
        let wallet = names.cli_wallet.clone();
        let started = parsed(&names.cli_install());
        assert_eq!(started["value"]["installed"], false, "{started}");
        let id = started["value"]["job"]["id"].as_str().expect("a job").to_owned();
        assert!(id.starts_with("-install-"));
        let ended = job_end(&mut names, &id);
        assert_eq!(ended["state"], "done", "{ended}");
        assert_eq!(ended["version"], "names 9.9.9-test");
        assert_eq!(ended["prover"], true);
        assert_eq!(ended["warning"], Value::Null);
        let ours = dir.join("data/bin/names").to_string_lossy().into_owned();
        assert_eq!(names.cli.as_deref(), Some(ours.as_str()));
        // The wallet folder already chosen stays.
        assert_eq!(names.cli_wallet, wallet);
        // The man pages go beside bin, and nothing outside its folder was given.
        assert!(dir.join("data/man").is_dir());
        let env = std::fs::read_to_string(dir.join("data/installer-env")).unwrap();
        assert!(!env.contains("NAMES_DOWNLOAD_BASE="));
        // Asked again, it's installed.
        let again = parsed(&names.cli_install());
        assert_eq!(again["value"]["installed"], true, "{again}");
        assert_eq!(again["value"]["path"], json!(ours));
        let status = parsed(&names.cli_status())["value"].clone();
        assert_eq!((status["cli"].clone(), status["prover"].clone(), status["installing"].clone()), (json!(ours), json!(true), Value::Null));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_install_says_why_and_changes_nothing() {
        let (mut names, dir) = with_fake_cli("install-failed", "exit 0");
        names.bin_dir = Some(dir.join("data/bin"));
        names.cli = Some("/no/such/names".into());
        let jobs = names.jobs().unwrap();
        let script = "#!/bin/sh\necho 'Downloading names for macos-arm64...'\necho 'curl: (6) Could not resolve host: github.com' >&2\nexit 6\n";
        let (bin, man) = names.install_dirs().unwrap();
        let id = jobs.start_install(script, &bin, &man).unwrap()["id"].as_str().unwrap().to_owned();
        let ended = job_end(&mut names, &id);
        assert_eq!(ended["state"], "failed", "{ended}");
        assert_eq!(ended["problem"], "Couldn't reach the internet. Check your connection, then try again.", "{ended}");
        assert_eq!(names.cli.as_deref(), Some("/no/such/names"));
        // A finished install whose CLI doesn't start isn't used either.
        let script = "#!/bin/sh\nprintf '#!/bin/sh\\nexit 1\\n' > \"$NAMES_INSTALL_DIR/names\"\nchmod 755 \"$NAMES_INSTALL_DIR/names\"\necho \"Installed $NAMES_INSTALL_DIR/names, but it does not start: \" >&2\n";
        let id = jobs.start_install(script, &bin, &man).unwrap()["id"].as_str().unwrap().to_owned();
        let ended = job_end(&mut names, &id);
        assert_eq!(ended["state"], "failed", "{ended}");
        assert!(ended["problem"].as_str().unwrap().starts_with("It installed, but it doesn't start"), "{ended}");
        assert_eq!(names.cli.as_deref(), Some("/no/such/names"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn creating_the_wallet_returns_the_phrase_once_and_logs_nothing() {
        let phrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
        let public = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
        let private = "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
        let (mut names, dir) = with_fake_cli(
            "create",
            &format!("echo 'Created a new wallet. Write down its recovery phrase and keep it safe;'; echo 'it is the only way to get the wallet back:'; echo; echo '  {phrase}'; echo; echo 'public account   Public/{public}'; echo 'private account  Private/{private}'"),
        );
        let made = parsed(&names.cli_create_wallet());
        assert_eq!(made["value"], json!({ "public": public, "private": private, "phrase": phrase }), "{made}");
        // Nothing about it went to the jobs folder.
        assert!(!dir.join("jobs").exists() || std::fs::read_dir(dir.join("jobs")).unwrap().count() == 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn moving_to_private_and_paying_an_account_are_wallet_jobs() {
        let public = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
        let other = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let (mut names, dir) = with_fake_cli("shield", "echo 'public account   Public/x'; echo 'Public/x holds 1200; send it at least 50000000 testnet tokens'");
        let wallet = std::path::PathBuf::from(names.cli_wallet.clone().unwrap());
        // No accounts yet: refused before anything runs.
        assert!(parsed(&names.cli_shield(String::new()))["error"].as_str().unwrap().contains("create your wallet"));
        std::fs::write(wallet.join("names.json"), format!("{{\"public\":\"{public}\",\"private\":\"yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z\"}}")).unwrap();
        assert!(parsed(&names.cli_shield("0".into()))["error"].as_str().unwrap().contains("above zero"));
        let started = parsed(&names.cli_shield(String::new()));
        assert_eq!(started["value"]["kind"], "shield", "{started}");
        let id = started["value"]["id"].as_str().unwrap().to_owned();
        let ended = job_end(&mut names, &id);
        assert_eq!(ended["state"], "done");
        assert_eq!(ended["moved"], false);
        assert!(ended["log_tail"].as_str().unwrap().contains("[setup]\n[--shield=50000000]"), "{ended}");
        assert!(ended["problem"].as_str().unwrap().contains("needs at least 50,000,000"), "{ended}");
        // Paying an account: checked first, then the wallet's own transfer.
        for (to, amount) in [("jef", "5"), (other, "5"), ("Public/nope", "5"), (&format!("Public/{other}") as &str, "0")] {
            assert_eq!(parsed(&names.cli_pay_account(to.into(), amount.into(), false))["success"], false, "{to} {amount}");
        }
        let paid = parsed(&names.cli_pay_account(format!("Public/{other}"), "1000".into(), false));
        let id = paid["value"]["id"].as_str().expect("a job").to_owned();
        let ended = job_end(&mut names, &id);
        let log = ended["log_tail"].as_str().unwrap();
        assert!(log.contains(&format!("[wallet]\n[auth-transfer]\n[send]\n[--from=Public/{public}]\n[--to=Public/{other}]\n[--amount=1000]")), "{log}");
        let status = parsed(&names.cli_status())["value"].clone();
        assert_eq!((status["wallet"].clone(), status["public"].clone(), status["busy"].clone()), (json!(true), json!(public), Value::Null));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_reported_version_is_the_package_version() {
        let metadata: Value = serde_json::from_str(include_str!("../../metadata.json")).expect("metadata.json");
        assert_eq!(metadata["version"], MODULE_VERSION);
        let app: Value = serde_json::from_str(include_str!("../../../basecamp/metadata.json")).expect("the app's metadata.json");
        assert_eq!(app["dependencies"][0]["version"], format!(">={MODULE_VERSION}"));
        assert_eq!(fresh().shown_config()["module_version"], MODULE_VERSION);
    }

    // ── Wallet Provider SDK ─────────────────────────────────────────────

    const PUBLIC: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
    const PRIVATE: &str = "yGrPEbP3Go89i1Cy5EDMqtXxUFjKbe6FuA5VNiRfe4Z";
    const OTHER: &str = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";

    fn app(name: &str) -> logos_rust_sdk::LogosCaller {
        logos_rust_sdk::LogosCaller::Module { name: name.into(), instance: None }
    }

    /// A module with a fake CLI whose wallet has a public and a private
    /// account, a provider store of its own, and a sequencer nobody answers.
    fn with_wallet(label: &str, tail: &str) -> (Names, std::path::PathBuf) {
        let (mut names, dir) = with_fake_cli(label, tail);
        let wallet = std::path::PathBuf::from(names.cli_wallet.clone().unwrap());
        std::fs::write(wallet.join("names.json"), format!("{{\"public\":\"{PUBLIC}\",\"private\":\"{PRIVATE}\"}}")).unwrap();
        names.provider_path = Some(dir.join("provider.json"));
        names.sequencer = "http://127.0.0.1:9".into();
        (names, dir)
    }

    /// Has the Names app (here, the host) approve `id` with `account`.
    fn approve(names: &mut Names, id: &str, account: &str) -> Value {
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        parsed(&names.approve_request(id.into(), account.into()))
    }

    /// `app` asks for access and the Names app gives it `account`.
    fn connect(names: &mut Names, who: &str, account: &str) {
        as_caller(app(who));
        let asked = parsed(&names.request_access("to show your balance".into()));
        let id = asked["value"]["request"].as_str().expect("a request").to_owned();
        assert_eq!(approve(names, &id, account)["value"]["state"], "approved");
    }

    #[test]
    fn another_app_must_ask_and_can_never_decide() {
        let (mut names, dir) = with_wallet("provider-guard", "exit 0");
        as_caller(app("evil_ui"));
        // Nothing can be proposed before access is given.
        let early = parsed(&names.propose_transfer("LEZ".into(), "5".into(), "jef".into()));
        assert!(early["error"].as_str().unwrap().contains("request_access"), "{early}");
        let asked = parsed(&names.request_access("Tips\u{202E} for posts".into()));
        assert_eq!(asked["value"]["state"], "pending", "{asked}");
        let id = asked["value"]["request"].as_str().unwrap().to_owned();
        // Asking again returns the same request rather than a second prompt.
        assert_eq!(parsed(&names.request_access(String::new()))["value"]["request"], json!(id));
        // The asking app can't see, approve or reject anything, nor grant itself access.
        for refused in [
            names.pending_requests(),
            names.approve_request(id.clone(), format!("Public/{PUBLIC}")),
            names.reject_request(id.clone()),
            names.app_grants(),
            names.revoke_app("names_ui".into()),
        ] {
            assert!(parsed(&refused)["error"].as_str().unwrap().contains("only the Names app"), "{refused}");
        }
        assert_eq!(parsed(&names.granted_accounts())["value"], json!({ "connected": false, "accounts": [] }));
        assert_eq!(parsed(&names.account_balances())["value"], json!([]));
        // A caller that can't be named can't even ask.
        as_caller(logos_rust_sdk::LogosCaller::Unknown);
        assert_eq!(parsed(&names.request_access(String::new()))["success"], false);
        as_caller(app("host"));
        assert_eq!(parsed(&names.request_access(String::new()))["success"], false);

        // The Names app sees who asks, why, and which accounts it may give.
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let pending = parsed(&names.pending_requests())["value"].clone();
        assert_eq!(pending.as_array().unwrap().len(), 1, "{pending}");
        assert_eq!(pending[0]["app"], "evil_ui");
        assert_eq!(pending[0]["reason"], "Tips for posts");
        let choices: Vec<&str> = pending[0]["choices"].as_array().unwrap().iter().map(|c| c["account"].as_str().unwrap()).collect();
        assert_eq!(choices, vec![format!("Public/{PUBLIC}"), format!("Private/{PRIVATE}")]);
        // Only an offered account can be given.
        assert!(approve(&mut names, &id, &format!("Public/{OTHER}"))["error"].as_str().unwrap().contains("isn't one of"));
        assert!(approve(&mut names, &id, PUBLIC)["error"].as_str().unwrap().contains("Public/<id>"));
        let given = approve(&mut names, &id, &format!("Public/{PUBLIC}"));
        assert_eq!(given["value"]["state"], "approved", "{given}");
        assert_eq!(given["value"]["accounts"], json!([format!("Public/{PUBLIC}")]));
        assert!(approve(&mut names, &id, &format!("Public/{PUBLIC}"))["error"].as_str().unwrap().contains("approved already"));

        // The app sees only what it was given: the public account, not the private one.
        as_caller(app("evil_ui"));
        assert_eq!(parsed(&names.request_status(id.clone()))["value"]["state"], "approved");
        let granted = parsed(&names.granted_accounts())["value"].clone();
        assert_eq!(granted, json!({ "connected": true, "accounts": [{ "account": format!("Public/{PUBLIC}"), "account_kind": "public" }] }));
        let balances = parsed(&names.account_balances())["value"].clone();
        assert_eq!(balances.as_array().unwrap().len(), 1, "{balances}");
        assert_eq!(balances[0]["account"], format!("Public/{PUBLIC}"));
        // Already connected: no new prompt.
        assert_eq!(parsed(&names.request_access(String::new()))["value"]["request"], Value::Null);
        // Another app can't read this one's request.
        as_caller(app("other_ui"));
        assert!(parsed(&names.request_status(id.clone()))["error"].as_str().unwrap().contains("no such request"));
        assert!(parsed(&names.cancel_request(id.clone()))["error"].as_str().unwrap().contains("no such request"));
        // Giving access survives a restart.
        let mut again = fresh();
        again.provider_path = names.provider_path.clone();
        as_caller(app("evil_ui"));
        assert_eq!(parsed(&again.granted_accounts())["value"]["connected"], true);
        // Access can be taken back by the Names app, or given up by the app.
        assert_eq!(parsed(&names.drop_access())["value"]["revoked"], true);
        assert_eq!(parsed(&names.granted_accounts())["value"]["connected"], false);
        // Nothing ran the CLI.
        assert!(!dir.join("jobs").exists() || std::fs::read_dir(dir.join("jobs")).unwrap().count() == 0);
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_transfer_runs_only_after_approval_from_the_chosen_account() {
        let (mut names, dir) = with_wallet("provider-transfer", "exit 0");
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        // Checked as it is asked for.
        for (asset, amount, to) in [("USDC", "5", "jef"), ("LEZ", "0", "jef"), ("LEZ", "5", OTHER), ("LEZ", "5", "$(id)")] {
            assert_eq!(parsed(&names.propose_transfer(asset.into(), amount.into(), to.into()))["success"], false, "{asset} {amount} {to}");
        }
        let to_name = parsed(&names.propose_transfer("LEZ".into(), "1,000".into(), "Jef".into()))["value"].clone();
        let to_account = parsed(&names.propose_transfer(String::new(), "250".into(), format!("Public/{OTHER}")))["value"].clone();
        assert_eq!((to_name["state"].as_str(), to_name["amount"].as_str(), to_name["destination"].as_str()), (Some("pending"), Some("1000"), Some("jef")));
        // Nothing has run yet.
        assert!(!dir.join("jobs").exists());
        // The prompt shows the asset, amount and destination, and offers the CLI's accounts.
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let pending = parsed(&names.pending_requests())["value"].clone();
        let shown = pending.as_array().unwrap().iter().find(|r| r["id"] == to_name["request"]).unwrap().clone();
        assert_eq!((shown["app"].as_str(), shown["asset"].as_str(), shown["amount"].as_str(), shown["destination"].as_str()), (Some("tips_ui"), Some("LEZ"), Some("1000"), Some("jef")));
        assert_eq!(shown["choices"].as_array().unwrap().len(), 2);
        assert_eq!(shown["choices"][0]["granted"], true);
        assert_eq!(shown["choices"][1]["granted"], false);

        // To a name from the public account: `names pay --from`.
        let started = approve(&mut names, to_name["request"].as_str().unwrap(), &format!("Public/{PUBLIC}"));
        assert_eq!(started["value"]["state"], "running", "{started}");
        let job = started["value"]["job"].as_str().unwrap().to_owned();
        let ended = job_end(&mut names, &job);
        assert!(ended["log_tail"].as_str().unwrap().contains(&format!("[pay]\n[--from=Public/{PUBLIC}]\n[--]\n[jef]\n[1000]")), "{ended}");
        // To an account from the private account: the wallet's own transfer.
        let started = approve(&mut names, to_account["request"].as_str().unwrap(), &format!("Private/{PRIVATE}"));
        let job = started["value"]["job"].as_str().unwrap().to_owned();
        let ended = job_end(&mut names, &job);
        assert!(ended["log_tail"].as_str().unwrap().contains(&format!("[--from=Private/{PRIVATE}]\n[--to=Public/{OTHER}]\n[--amount=250]")), "{ended}");

        // The app learns it went through, never the CLI's log or job.
        as_caller(app("tips_ui"));
        let status = parsed(&names.request_status(to_account["request"].as_str().unwrap().into()))["value"].clone();
        assert_eq!(status["state"], "done", "{status}");
        assert_eq!(status["account"], format!("Private/{PRIVATE}"));
        assert!(status.get("job").is_none() && status.get("log_tail").is_none(), "{status}");
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_payment_tells_the_app_nothing_about_the_wallet() {
        let (mut names, dir) = with_wallet("provider-failed", "echo 'Error: Public/CCVd holds 1200; send it at least 5000' >&2; exit 1");
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        let asked = parsed(&names.propose_transfer("LEZ".into(), "5000".into(), format!("Public/{OTHER}")))["value"].clone();
        let id = asked["request"].as_str().unwrap().to_owned();
        let started = approve(&mut names, &id, &format!("Public/{PUBLIC}"));
        job_end(&mut names, started["value"]["job"].as_str().unwrap());
        as_caller(app("tips_ui"));
        let status = parsed(&names.request_status(id.clone()))["value"].clone();
        assert_eq!(status["state"], "failed", "{status}");
        assert_eq!(status["error"], "the payment didn't go through");
        assert!(!status.to_string().contains("1200"), "{status}");
        // The Names app gets the CLI's reason.
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let full = parsed(&names.request_status(id))["value"].clone();
        assert!(full["problem"].as_str().unwrap().contains("holds 1200"), "{full}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_prompt_says_who_publishes_the_asking_app() {
        let (mut names, dir) = with_wallet("provider-publisher", "exit 0");
        // Without the Program Registry the prompt says it couldn't check.
        program_registry::PUBLISHER.with(|p| *p.borrow_mut() = None);
        as_caller(app("forumer_ui"));
        parsed(&names.request_access("to post as you".into()));
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let shown = parsed(&names.pending_requests())["value"][0].clone();
        assert_eq!(shown["publisher"], json!({ "status": "unavailable" }));
        // What the registry says is shown, and kept for a minute.
        names.publishers.clear();
        let verified = json!({ "status": "verified", "publisher": "forumer", "module": "forumer_ui", "version": "1.2.0", "root": "ab".repeat(32), "record": null });
        program_registry::PUBLISHER.with(|p| *p.borrow_mut() = Some(verified.clone()));
        let shown = parsed(&names.pending_requests())["value"][0].clone();
        assert_eq!(shown["publisher"], verified);
        program_registry::PUBLISHER.with(|p| *p.borrow_mut() = Some(json!({ "status": "mismatch" })));
        assert_eq!(parsed(&names.pending_requests())["value"][0]["publisher"], verified, "kept for a minute");
        // The asking app never sees it: only the Names app reads pending requests.
        as_caller(app("forumer_ui"));
        assert_eq!(parsed(&names.pending_requests())["success"], false);
        program_registry::PUBLISHER.with(|p| *p.borrow_mut() = None);
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_busy_wallet_leaves_the_request_pending() {
        let (mut names, dir) = with_wallet("provider-busy", "sleep 2");
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        let first = parsed(&names.propose_transfer("LEZ".into(), "1".into(), format!("Public/{OTHER}")))["value"]["request"].as_str().unwrap().to_owned();
        let second = parsed(&names.propose_transfer("LEZ".into(), "2".into(), format!("Public/{OTHER}")))["value"]["request"].as_str().unwrap().to_owned();
        assert_eq!(approve(&mut names, &first, &format!("Public/{PUBLIC}"))["value"]["state"], "running");
        let busy = approve(&mut names, &second, &format!("Public/{PUBLIC}"));
        assert!(busy["error"].as_str().unwrap().contains("busy"), "{busy}");
        as_caller(app("tips_ui"));
        assert_eq!(parsed(&names.request_status(second.clone()))["value"]["state"], "pending");
        // The app may withdraw it; afterwards it can't be approved.
        assert_eq!(parsed(&names.cancel_request(second.clone()))["value"]["state"], "cancelled");
        assert!(approve(&mut names, &second, &format!("Public/{PUBLIC}"))["error"].as_str().unwrap().contains("cancelled"));
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_call_is_signed_by_the_names_wallet_only_after_approval() {
        // The fake CLI lists two public accounts and, asked to send, keeps
        // what it was given and says it submitted it.
        let tail = format!(
            "case \"$*\" in\n\
             *\"module accounts\"*) echo '{{\"public\":[\"{PUBLIC}\",\"{OTHER}\"],\"private\":[]}}' ;;\n\
             *\"module send\"*) printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"; cat > \"$(dirname \"$0\")/sent.json\"; echo 'submitted feedbeef'; echo 'applied in block 4' ;;\n\
             esac"
        );
        let (mut names, dir) = with_wallet("provider-call", &tail);
        let program = registry::account_text(&[5; 32]);
        let data = registry::account_text(&[6; 32]);
        names.program = Some([5; 32]);
        program_registry::LABEL.with(|label| *label.borrow_mut() = Some(json!({ "text": "Tips 1.0", "kind": "deployer", "verified": false, "stale": false, "upgradeable": true })));
        connect(&mut names, "tips_ui", &format!("Public/{OTHER}"));
        as_caller(app("tips_ui"));
        let accounts = format!("[\"{data}\", {{\"account\": \"signer\", \"balance\": true}}]");
        assert!(parsed(&names.propose_call(program.clone(), "[\"signer\"]".into(), "0a0b".into(), String::new()))["error"].as_str().unwrap().contains("only for its LEZ balance"));
        let asked = parsed(&names.propose_call(program.clone(), accounts.clone(), "000a0b".into(), "Tip jef 5 LEZ".into()))["value"].clone();
        assert_eq!(asked["state"], "pending", "{asked}");
        assert!(!dir.join("sent.json").exists());

        // The prompt: the program's label and status, what it can do, the app's own words.
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let mut shown = Value::Null;
        for _ in 0..100 {
            shown = parsed(&names.pending_requests())["value"][0].clone();
            if shown["choices"].as_array().is_some_and(|choices| choices.len() == 2) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(shown["program_label"]["text"], "Tips 1.0");
        assert_eq!(shown["program_label"]["verified"], false);
        assert_eq!(shown["app_summary"], "Tip jef 5 LEZ");
        let warnings = shown["warnings"].to_string();
        assert!(warnings.contains("Names registry") && warnings.contains("hasn't been verified") && warnings.contains("upgraded"), "{warnings}");
        assert!(shown["effects"][1].as_str().unwrap().contains("Tips 1.0 may move LEZ out of the account you choose"), "{shown}");
        // Only the wallet's public accounts can sign a call, the main one first.
        let choices: Vec<&str> = shown["choices"].as_array().unwrap().iter().map(|c| c["account"].as_str().unwrap()).collect();
        assert_eq!(choices, vec![format!("Public/{PUBLIC}"), format!("Public/{OTHER}")], "{shown}");
        assert_eq!(shown["choices"][1]["granted"], true);
        let id = asked["request"].as_str().unwrap().to_owned();
        assert!(approve(&mut names, &id, &format!("Private/{PRIVATE}"))["error"].as_str().unwrap().contains("sign with"));
        let sent = approve(&mut names, &id, &format!("Public/{OTHER}"));
        assert_eq!(sent["value"]["state"], "done", "{sent}");
        assert_eq!(sent["value"]["tx"], "feedbeef");
        // The CLI got the call's program in place of the registry, and the change.
        let argv = std::fs::read_to_string(dir.join("argv")).unwrap();
        assert!(argv.contains(&format!("--program={program}\n")) && argv.ends_with("module\nsend\n"), "{argv}");
        let change: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("sent.json")).unwrap()).unwrap();
        let native = registry::account_text(&registry::NATIVE_TOKEN);
        assert_eq!(change, json!({
            "message": "000a0b",
            "spent": "0",
            "mentions": [
                { "account": OTHER, "program": program, "seed": null, "signs": true },
                { "account": data, "program": program, "seed": null, "signs": false },
                { "account": OTHER, "program": native, "seed": null, "signs": true },
            ],
        }));
        // It's sent once.
        std::fs::remove_file(dir.join("sent.json")).unwrap();
        assert!(approve(&mut names, &id, &format!("Public/{OTHER}"))["error"].as_str().unwrap().contains("done already"));
        assert!(!dir.join("sent.json").exists());
        as_caller(app("tips_ui"));
        assert_eq!(parsed(&names.request_status(id))["value"]["tx"], "feedbeef");
        // A rejected call is never sent.
        let again = parsed(&names.propose_call(program, accounts, "0c".into(), String::new()))["value"]["request"].as_str().unwrap().to_owned();
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        assert_eq!(parsed(&names.reject_request(again.clone()))["value"]["state"], "rejected");
        assert!(approve(&mut names, &again, &format!("Public/{OTHER}"))["error"].as_str().unwrap().contains("rejected already"));
        assert!(!dir.join("sent.json").exists());
        // The Names app sees and can take back what each app was given.
        let grants = parsed(&names.app_grants())["value"].clone();
        assert_eq!(grants[0]["app"], "tips_ui");
        assert_eq!(parsed(&names.revoke_app("tips_ui".into()))["value"]["revoked"], true);
        as_caller(app("tips_ui"));
        assert!(parsed(&names.propose_call(registry::account_text(&[5; 32]), "[]".into(), "01".into(), String::new()))["error"].as_str().unwrap().contains("request_access"));
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_call_is_never_sent_again() {
        let tail = format!(
            "case \"$*\" in\n\
             *\"module accounts\"*) echo '{{\"public\":[\"{PUBLIC}\"],\"private\":[]}}' ;;\n\
             *\"module send\"*) touch \"$(dirname \"$0\")/tried\"; echo 'Error: refused: insufficient balance' >&2; exit 1 ;;\n\
             esac"
        );
        let (mut names, dir) = with_wallet("provider-call-failed", &tail);
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        let id = parsed(&names.propose_call(registry::account_text(&[5; 32]), "[]".into(), "01".into(), String::new()))["value"]["request"].as_str().unwrap().to_owned();
        let failed = approve(&mut names, &id, &format!("Public/{PUBLIC}"));
        assert!(failed["error"].as_str().unwrap().contains("insufficient balance"), "{failed}");
        assert!(dir.join("tried").exists());
        as_caller(app("tips_ui"));
        let status = parsed(&names.request_status(id.clone()))["value"].clone();
        assert_eq!((status["state"].as_str(), status["error"].as_str()), (Some("failed"), Some("the wallet couldn't send it")), "{status}");
        assert!(approve(&mut names, &id, &format!("Public/{PUBLIC}"))["error"].as_str().unwrap().contains("failed already"));
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Tokens ──────────────────────────────────────────────────────────

    /// The token program and a token of it, in these tests.
    const PROGRAM: [u8; 32] = [3; 32];
    const DEMO: [u8; 32] = [4; 32];

    /// Asks `ask` until its read is no longer updating and has a value at `key`.
    fn settled(names: &mut Names, key: &str, ask: impl Fn(&mut Names) -> String) -> Value {
        for _ in 0..200 {
            let answer = parsed(&ask(names));
            assert_eq!(answer["success"], true, "{answer}");
            if answer["value"]["updating"] == false && !answer["value"][key].is_null() {
                return answer["value"].clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("the token read did not end");
    }

    #[test]
    fn the_token_program_is_chosen_by_the_names_app_and_kept() {
        let mut names = fresh();
        // None is built in, and every token method says so.
        for answer in [names.cli_token_pending(), names.cli_token_holdings(String::new()), names.cli_token_receive(), names.cli_token_info(registry::account_text(&DEMO))] {
            assert!(parsed(&answer)["error"].as_str().unwrap().contains("no token program"), "{answer}");
        }
        assert!(!names.set_token_program("nope".into()));
        assert!(!names.set_token_program(registry::account_text(&registry::NATIVE_TOKEN)));
        // Another app can neither choose it nor use any token method.
        as_caller(app("evil_ui"));
        assert!(!names.set_token_program(registry::account_text(&PROGRAM)));
        for refused in [
            names.cli_token_holdings(String::new()),
            names.cli_token_pending(),
            names.cli_token_info(registry::account_text(&DEMO)),
            names.cli_token_send(registry::account_text(&DEMO), "1".into(), "jef".into(), false, false),
            names.cli_token_receive(),
        ] {
            assert!(parsed(&refused)["error"].as_str().unwrap().contains("only the Names app"), "{refused}");
        }
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        assert_eq!(names.token_program, None);
        assert!(names.set_token_program(format!(" {} ", registry::account_text(&PROGRAM))));
        assert_eq!(parsed(&names.get_config())["token_program"], registry::account_text(&PROGRAM));
        // It survives a restart; an edited file can't name something else.
        let mut again = fresh();
        again.apply_saved(&names.config());
        assert_eq!(again.token_program, Some(PROGRAM));
        again.apply_saved(&json!({ "token_program": "../../etc" }));
        assert_eq!(again.token_program, None);
        again.apply_saved(&json!({ "token_program": registry::account_text(&registry::NATIVE_TOKEN) }));
        assert_eq!(again.token_program, None);
        assert!(names.set_token_program(String::new()));
        assert_eq!(names.token_program, None);
    }

    #[test]
    fn holdings_and_pending_credits_are_read_in_the_background() {
        let demo = registry::account_text(&DEMO);
        let list = json!({
            "holdings": [
                { "account": format!("Public/{PUBLIC}"), "private": false, "definition": demo, "name": "Demo", "balance": "1000250" },
                { "account": format!("Private/{PRIVATE}"), "private": true, "definition": demo, "name": "Demo", "balance": "200" },
            ],
            "pending": [{ "position": 21, "definition": demo, "name": "Demo", "amount": "50", "to": PRIVATE }],
        });
        let tail = format!(
            "case \"$*\" in\n\
             *\"token list --json\"*) printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"; echo 'Latest block is 9'; echo '{list}' ;;\n\
             esac"
        );
        let (mut names, dir) = with_wallet("token-list", &tail);
        names.token_program = Some(PROGRAM);
        let all = settled(&mut names, "holdings", |names| names.cli_token_holdings(String::new()));
        assert_eq!(all["token_program"], registry::account_text(&PROGRAM));
        assert_eq!(all["holdings"].as_array().unwrap().len(), 2, "{all}");
        assert_eq!(all["holdings"][1], json!({ "account": format!("Private/{PRIVATE}"), "account_kind": "private", "definition": demo, "name": "Demo", "balance": "200" }));
        // One account's only.
        let private = parsed(&names.cli_token_holdings(format!("Private/{PRIVATE}")))["value"]["holdings"].clone();
        assert_eq!(private.as_array().unwrap().len(), 1);
        assert_eq!(private[0]["balance"], "200");
        assert_eq!(parsed(&names.cli_token_holdings(PUBLIC.into()))["success"], false);
        // The pending credits come from the same read, written as accounts.
        let pending = settled(&mut names, "pending", |names| names.cli_token_pending());
        assert_eq!(pending["pending"], json!([{ "position": 21, "definition": demo, "name": "Demo", "amount": "50", "to": format!("Private/{PRIVATE}") }]));
        // The CLI was given the module's token program.
        let argv = std::fs::read_to_string(dir.join("argv")).unwrap();
        assert!(argv.contains(&format!("--token-program={}\ntoken\nlist\n--json\n", registry::account_text(&PROGRAM))), "{argv}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn token_sends_and_receipts_are_wallet_jobs_checked_first() {
        let tail = "case \"$*\" in\n\
                    *\"token send\"*) echo 'sending 1000 Demo'; echo 'submitted ab12'; echo 'applied in block 7' ;;\n\
                    *\"token receive\"*) echo 'nothing to receive' ;;\n\
                    esac";
        let (mut names, dir) = with_wallet("token-send", tail);
        let demo = registry::account_text(&DEMO);
        let program = registry::account_text(&PROGRAM);
        // Without a token program, nothing runs.
        assert!(parsed(&names.cli_token_send(demo.clone(), "5".into(), "jef".into(), true, false))["error"].as_str().unwrap().contains("no token program"));
        names.token_program = Some(PROGRAM);
        for (definition, amount, to) in [
            ("nope".to_owned(), "5", "jef".to_owned()),
            (registry::account_text(&registry::NATIVE_TOKEN), "5", "jef".to_owned()),
            (demo.clone(), "0", "jef".to_owned()),
            (demo.clone(), "1.5", "jef".to_owned()),
            (demo.clone(), "340282366920938463463374607431768211456", "jef".to_owned()),
            (demo.clone(), "5", OTHER.to_owned()),
            (demo.clone(), "5", "$(id)".to_owned()),
            (demo.clone(), "5", "--from=public".to_owned()),
        ] {
            assert_eq!(parsed(&names.cli_token_send(definition.clone(), amount.into(), to.clone(), true, false))["success"], false, "{definition} {amount} {to}");
        }
        assert!(!dir.join("jobs").exists(), "nothing ran");

        // From the private account to a name.
        let started = parsed(&names.cli_token_send(demo.clone(), "1,000".into(), "Jef".into(), true, false));
        assert_eq!(started["value"]["kind"], "token_send", "{started}");
        let ended = job_end(&mut names, started["value"]["id"].as_str().unwrap());
        assert_eq!((ended["state"].as_str(), ended["tx"].as_str(), ended["block"].as_u64()), (Some("done"), Some("ab12"), Some(7)), "{ended}");
        let log = ended["log_tail"].as_str().unwrap();
        assert!(log.contains(&format!("[--token-program={program}]\n[token]\n[send]\n[--to=jef]\n[--from=private]\n[--]\n[{demo}]\n[1000]")), "{log}");
        // From the public account to a name's public account.
        let started = parsed(&names.cli_token_send(demo.clone(), "5".into(), "jef".into(), false, true));
        let ended = job_end(&mut names, started["value"]["id"].as_str().unwrap());
        assert!(ended["log_tail"].as_str().unwrap().contains("[--to=jef]\n[--from=public]\n[--public]\n[--]"), "{ended}");
        // Receiving: there was nothing.
        let started = parsed(&names.cli_token_receive());
        assert_eq!(started["value"]["kind"], "token_receive", "{started}");
        let ended = job_end(&mut names, started["value"]["id"].as_str().unwrap());
        assert_eq!((ended["nothing"].clone(), ended["applied"].clone()), (json!(true), json!(0)), "{ended}");
        assert!(ended["log_tail"].as_str().unwrap().contains(&format!("[--token-program={program}]\n[token]\n[receive]")));
        let _ = std::fs::remove_dir_all(dir);

        // A refusal says why.
        let (mut names, dir) = with_wallet("token-send-refused", "echo 'Error: Public/CCVd holds 0 Demo, less than 5' >&2; exit 1");
        names.token_program = Some(PROGRAM);
        let started = parsed(&names.cli_token_send(demo, "5".into(), format!("Public/{OTHER}"), false, false));
        let ended = job_end(&mut names, started["value"]["id"].as_str().unwrap());
        assert_eq!(ended["state"], "failed");
        assert_eq!(ended["problem"], "The token transfer didn't go through: Public/CCVd holds 0 Demo, less than 5");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_app_can_propose_a_token_transfer_named_by_its_token() {
        let demo = registry::account_text(&DEMO);
        let missing = registry::account_text(&[8; 32]);
        let tail = format!(
            "case \"$*\" in\n\
             *\"module token info -- {missing}\"*) touch \"$(dirname \"$0\")/asked\"; echo 'Error: not a token' >&2; exit 1 ;;\n\
             *\"module token info\"*) touch \"$(dirname \"$0\")/asked\"; echo '{{\"definition\":\"{demo}\",\"name\":\"Demo\",\"supply\":\"1000\",\"authority\":null}}' ;;\n\
             *\"token send\"*) echo 'submitted feed'; echo 'applied in block 9' ;;\n\
             esac"
        );
        let (mut names, dir) = with_wallet("token-provider", &tail);
        // An app that wasn't given access costs nothing, not even a read.
        as_caller(app("tips_ui"));
        assert!(parsed(&names.propose_transfer(demo.clone(), "5".into(), "jef".into()))["error"].as_str().unwrap().contains("request_access"));
        assert!(!dir.join("asked").exists());
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        // The wallet needs a token program, and the token must be one of it.
        assert!(parsed(&names.propose_transfer(demo.clone(), "5".into(), "jef".into()))["error"].as_str().unwrap().contains("no token program"));
        names.token_program = Some(PROGRAM);
        let unknown = parsed(&names.propose_transfer(missing, "5".into(), "jef".into()));
        assert_eq!(unknown["error"], "this wallet's token program has no such token", "{unknown}");
        program_registry::LABEL.with(|label| *label.borrow_mut() = Some(json!({ "text": "Tokens 0.1", "kind": "deployer", "verified": true, "stale": false, "upgradeable": false })));
        let asked = parsed(&names.propose_transfer(demo.clone(), "5".into(), "Jef".into()))["value"].clone();
        assert_eq!((asked["state"].as_str(), asked["asset"].as_str(), asked["asset_kind"].as_str(), asked["token_name"].as_str()), (Some("pending"), Some(demo.as_str()), Some("token"), Some("Demo")), "{asked}");

        // The prompt names the token, its program and the program's label.
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let shown = parsed(&names.pending_requests())["value"][0].clone();
        assert_eq!((shown["token_name"].as_str(), shown["token_program"].as_str(), shown["amount"].as_str(), shown["destination"].as_str()), (Some("Demo"), Some(registry::account_text(&PROGRAM).as_str()), Some("5"), Some("jef")), "{shown}");
        assert_eq!(shown["program_label"]["text"], "Tokens 0.1");
        assert_eq!(shown["warnings"], json!([]));
        assert_eq!(shown["choices"].as_array().unwrap().len(), 2);
        // Approved from the private account: a token send, from there, to the name.
        let started = approve(&mut names, asked["request"].as_str().unwrap(), &format!("Private/{PRIVATE}"));
        assert_eq!(started["value"]["state"], "running", "{started}");
        let ended = job_end(&mut names, started["value"]["job"].as_str().unwrap());
        let log = ended["log_tail"].as_str().unwrap();
        assert!(log.contains(&format!("[--token-program={}]\n[token]\n[send]\n[--to=jef]\n[--from=private]\n[--]\n[{demo}]\n[5]", registry::account_text(&PROGRAM))), "{log}");
        as_caller(app("tips_ui"));
        let status = parsed(&names.request_status(asked["request"].as_str().unwrap().into()))["value"].clone();
        assert_eq!((status["state"].as_str(), status["token_name"].as_str()), (Some("done"), Some("Demo")), "{status}");

        // Nothing is sent with a token program other than the one the prompt named.
        let again = parsed(&names.propose_transfer(demo, "6".into(), format!("Public/{OTHER}")))["value"]["request"].as_str().unwrap().to_owned();
        names.token_program = Some([5; 32]);
        let refused = approve(&mut names, &again, &format!("Public/{PUBLIC}"));
        assert!(refused["error"].as_str().unwrap().contains("token program has changed"), "{refused}");
        as_caller(app("tips_ui"));
        assert_eq!(parsed(&names.request_status(again))["value"]["state"], "pending");
        program_registry::LABEL.with(|label| *label.borrow_mut() = None);
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Accounts and fees ──────────────────────────────────────────────

    #[test]
    fn the_account_list_is_checked_before_the_app_sees_it() {
        let key = "ab".repeat(32);
        let list = format!(
            r#"[{{"account":"Public/{PUBLIC}","kind":"public","label":"Main","balance":"12","default":true,"public_key":"{key}"}},{{"account":"Private/{PRIVATE}","kind":"private","label":"bad\u0007","balance":"x","default":false,"public_key":"{key}"}}]"#
        );
        let (mut names, dir) = with_wallet("accounts-list", &format!("echo '{list}'"));
        let got = parsed(&names.cli_accounts());
        assert_eq!(got["success"], true, "{got}");
        let rows = got["value"].as_array().unwrap();
        assert_eq!(rows[0], json!({ "account": format!("Public/{PUBLIC}"), "kind": "public", "label": "Main", "balance": "12", "default": true, "public_key": key }));
        assert_eq!(rows[1], json!({ "account": format!("Private/{PRIVATE}"), "kind": "private", "label": null, "balance": null, "default": false, "public_key": null }));
        // Another app can't read the wallet's accounts this way.
        as_caller(app("snoop_ui"));
        assert_eq!(parsed(&names.cli_accounts())["success"], false);
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn account_changes_check_their_input_and_wait_for_running_jobs() {
        let entry = format!(r#"{{"account":"Private/{PRIVATE}","kind":"private","label":"Savings","balance":"0","default":true,"public_key":null}}"#);
        let (mut names, dir) = with_wallet("accounts-change", &format!("sleep 1; echo '{entry}'"));
        for (kind, label) in [("both", ""), ("public", &"x".repeat(41)[..]), ("public", "two\nlines")] {
            let refused = parsed(&names.cli_account_new(kind.into(), label.into()));
            assert_eq!(refused["success"], false, "{kind} {label:?}");
        }
        assert_eq!(parsed(&names.cli_account_use(PRIVATE.into()))["success"], false, "a bare ID is refused");
        // While a job runs, nothing changes the wallet's accounts.
        let job = parsed(&names.cli_shield(String::new()))["value"]["id"].as_str().unwrap().to_owned();
        let busy = parsed(&names.cli_account_use(format!("Private/{PRIVATE}")));
        assert!(busy["error"].as_str().unwrap().contains("wait for"), "{busy}");
        assert_eq!(parsed(&names.cli_account_new("private".into(), "Savings".into()))["success"], false);
        job_end(&mut names, &job);
        let chosen = parsed(&names.cli_account_use(format!("Private/{PRIVATE}")));
        assert_eq!(chosen["value"]["label"], "Savings", "{chosen}");
        let made = parsed(&names.cli_account_new("private".into(), "-Savings".into()));
        assert_eq!(made["value"]["account"], format!("Private/{PRIVATE}"), "{made}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_job_reports_the_fee_it_paid() {
        let (mut names, dir) = with_wallet("job-fee", "echo 'fee: about 10, at most 20 (gas: 1 cycles, limit 2)'; echo 'applied in block 4'; echo 'fee paid: 9'");
        let job = parsed(&names.cli_shield(String::new()))["value"]["id"].as_str().unwrap().to_owned();
        job_end(&mut names, &job);
        let state = parsed(&names.job(job))["value"].clone();
        assert_eq!(state["fee"], json!({ "about": "10", "at_most": "20", "paid": "9" }), "{state}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_fee_quote_is_checked() {
        let quote = r#"{"height":7,"base_fee_exec":8,"base_fee_stor":8,"public_transfer":{"about":"8000","at_most":"18000"},"private_transfer":{"about":"900","at_most":"1200"}}"#;
        let (mut names, dir) = with_wallet("fee-quote", &format!("echo '{quote}'"));
        let got = parsed(&names.cli_fee());
        assert_eq!(got["value"]["public_transfer"], json!({ "about": "8000", "at_most": "18000" }), "{got}");
        assert_eq!(got["value"]["height"], 7);
        let (mut bad, other) = with_wallet("fee-quote-bad", r#"echo '{"public_transfer":{"about":"-1","at_most":"2"}}'"#);
        assert_eq!(parsed(&bad.cli_fee())["success"], false);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(other);
    }
}
