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

mod activity;
mod avatar;
mod chain;
mod chain_keys;
mod chain_fee;
mod chain_inscribe;
mod changes;
mod cli;
mod page_view;
mod provider;
mod provider_inscribe;
mod spell;
mod registry;
mod shortlist;
mod wallet;
mod zones;

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

    /// Has the zone in use read from the sequencer at this URL, and remembers
    /// it across restarts (`zone_sequencer` for that zone, without
    /// `other_chain_ok`). Returns false and changes nothing unless it is an
    /// https URL (http on this computer) of a sequencer that serves the same
    /// chain; one that serves another chain is another zone (`zone_add`).
    fn set_sequencer(&mut self, url: String) -> bool;

    // ── Zones (LP-0022) ─────────────────────────────────────────────────
    //
    // A zone is one LEZ chain, known by its channel on the Logos Blockchain,
    // not by its sequencer's address. The first is home, where LNS names are
    // kept; others are added by a sequencer's address and keep a wallet
    // folder of their own. One zone is in use at a time: balances, payments,
    // tokens, bridges and other apps' requests are that zone's, and names stay
    // readable everywhere. Names app only.

    /// The zones set up here: `{zones: [{id, name, channel, sequencer, home,
    /// current, wallet_dir, has_wallet, token_program}], current, max}`. A
    /// zone's `id` is `home`, or its channel.
    fn zone_list(&mut self) -> String;

    /// Adds the zone the sequencer at `sequencer` serves, called `name` (empty:
    /// "Zone" and the start of its channel). Refused when the sequencer doesn't
    /// say its channel, or serves a zone already here. Replies as `zone_list`,
    /// with `added`, the new zone's id; it isn't in use until `zone_use`.
    fn zone_add(&mut self, name: String, sequencer: String) -> String;

    /// Uses zone `zone` (an id from `zone_list`) from now on, and remembers it
    /// across restarts. Refused while a wallet job runs. Replies as `zone_list`.
    fn zone_use(&mut self, zone: String) -> String;

    /// Forgets zone `zone`: not home, and not the one in use. Its wallet folder
    /// stays on disk. Replies as `zone_list`, with `removed`: `{name, channel,
    /// wallet_dir}`.
    fn zone_remove(&mut self, zone: String) -> String;

    /// Calls zone `zone` `name` from now on. Replies as `zone_list`.
    fn zone_rename(&mut self, zone: String, name: String) -> String;

    /// Has zone `zone` use the sequencer at `url`. The zone's wallet, accounts
    /// and history stay as they are, so the sequencer must name the zone's own
    /// channel; one that names another chain, or doesn't say, is refused and
    /// nothing changes. Replies as `zone_list`, with `changed: true` and
    /// `channel`. For home and another chain the reply is `{changed: false,
    /// other_chain: true, channel, why}`, and asking again with
    /// `other_chain_ok` makes that chain home.
    fn zone_sequencer(&mut self, zone: String, url: String, other_chain_ok: bool) -> String;

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

    /// Hands a name the names CLI holds to `new_owner` (`Public/…` or `Private/…` and its
    /// ID), as a job: `names transfer`. Its receiving keys, text entries and page
    /// are cleared; the new holder runs `receive-privately`. It can't be undone.
    fn cli_transfer(&mut self, name: String, new_owner: String) -> String;

    /// Sets up payments to a name the wallet holds (one handed to it), as a job:
    /// `names receive-privately`.
    fn cli_receive_privately(&mut self, name: String) -> String;

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

    /// `cli_bridge`, credited to the LEZ public account `to` (`Public/<id>`, one the
    /// person picked; empty is the wallet's default public account).
    fn cli_bridge_to(&mut self, amount: String, from_key: String, to: String, node: String, send: bool) -> String;

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

    /// Claims the payments waiting for the wallet's private accounts (what `cli_balance`'s
    /// `waiting` counts), as a wallet job: `names receive`. Each claim is a private
    /// transaction, several minutes of proving, its fee paid by the wallet's private
    /// account holding the most.
    fn cli_receive(&mut self) -> String;

    /// `cli_receive` that claims every credit, also those worth less than claiming them
    /// costs (`cli_balance`'s `small`: a private fee's refund, most often), as a wallet
    /// job: `names receive --all`. Each claim's fee is more than it brings in.
    fn cli_receive_all(&mut self) -> String;

    /// The Wallet's history, newest first: what this app sent on either chain (followed
    /// until it settles: `sending`, `in_block`, `arrived` for a bridge credited on LEZ,
    /// `done`/`failed` for a LEZ job) and money seen arriving (`arrived`: a balance that
    /// rose between two reads). With `account` (a LEZ account, a node key or a name),
    /// only what involves it. Kept on this computer; neither chain is asked for history.
    fn activity(&mut self, account: String) -> String;

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

    /// The tokens added to the wallet by address, for the token program in
    /// use: `[{definition, name}]`, in the order added; `name` is the
    /// definition's own (as `cli_token_info` read it when added) or null.
    fn watched_tokens(&mut self) -> String;

    /// Adds (`watch`) or removes the token `definition` (its definition's
    /// account ID) from the wallet's list, so it shows before any of it is
    /// held. Adding reads the definition through the CLI first, so an ID that
    /// isn't a token of the program in use is refused. Gives the list as
    /// `watched_tokens` does. Kept on this device, per token program.
    fn watch_token(&mut self, definition: String, watch: bool) -> String;

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
    /// balance, default, public_key, npk}]`. `account` is written `Public/<id>` or
    /// `Private/<id>`; `label` the nickname given here, or null; `balance`
    /// decimal text (a public account's read from the chain, a private one's as
    /// the wallet last saw it), or null if it couldn't be read; `default` marks
    /// the account of its kind the wallet pays and registers from. `public_key`
    /// (hex, public accounts only) is what someone needs to pay a public
    /// account the chain hasn't seen yet; `npk` (hex, private accounts only)
    /// is the key a name's profile publishes to be paid there, so it tells
    /// which name an account receives for. The wallet catches up with the chain
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

    /// The keys the node's wallet holds: `[{key, balance, label?, title?,
    /// ready?}]`, `balance` decimal text or null if it couldn't be read.
    /// `label` is the one given with `chain_account_label`/`_new`; an account
    /// made with `chain_account_new` also has its keystore `title` and `ready`
    /// (false, balance null, until the node uses it from its next start). A
    /// hidden account (`chain_account_remove`) is left out while the node shows
    /// it holding nothing.
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

    /// Splits `amount` off the node key `from_key` into a note of its own, so a
    /// bridge can deposit exactly that much: a send from the key to itself (its
    /// change comes back as another note). Without `send` the plan `{from, amount,
    /// balance, exact}`, `exact` true when a note of exactly `amount` is already
    /// there (no split needed); with `send` the transfer is made and `hash` and
    /// `fee` are added, followed by `chain_tx` like a send. The notes it spends
    /// leave the stake, and the new ones join it within two epochs.
    fn chain_split(&mut self, from_key: String, amount: String, send: bool) -> String;

    /// Bridges exactly `amount` from the node key `from_key` into the LEZ public
    /// account `to` (`Public/<id>`; empty is the names wallet's default public
    /// one): one note of exactly that value is deposited on LEZ's channel through
    /// Basecamp's node (`channel_deposit_with_notes`), its fee paid from the key's
    /// other notes, at most `fee`. Without `send` the plan `{sent: false, hash:
    /// null, to, amount, total, notes, fee, note, needs_split, channel, lines}`;
    /// `needs_split` is true (and `note` null) when no note holds exactly
    /// `amount`: `chain_split` first. With `send` the deposit is made (it cannot be
    /// undone) and `sent`, `hash` are set; `chain_tx` follows it. LEZ credits it
    /// once the block holding it is final.
    fn chain_bridge(&mut self, from_key: String, to: String, amount: String, send: bool) -> String;

    /// Makes a new account on the node: a key in its keystore, labelled
    /// `label` (at most 40 characters; empty for none): `{key, title, label,
    /// ready, note}`. The node uses it from its next start (`ready` false
    /// until then; `note` says so). Its secret stays in the node's keystore.
    fn chain_account_new(&mut self, label: String) -> String;

    /// Labels the node key `key` (empty takes the label off): `{key, label}`.
    /// Labels are kept by this module, and shown by `chain_accounts`.
    fn chain_account_label(&mut self, key: String, label: String) -> String;

    /// Hides an account made with `chain_account_new` once the node uses it
    /// and it holds nothing: `{key, title, hidden, note}`. Its key stays in the
    /// node's keystore (the node can't say what is on its way to a key, so no
    /// secret is deleted), and it is listed again once the node shows it
    /// holding anything (what reached it before the node used it isn't seen).
    /// Refused for any other key, and for one holding anything.
    fn chain_account_remove(&mut self, key: String) -> String;

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

    /// Inscribes a testimonial (`{"submission": "lp-0022:names-wallet", "text"}`, text
    /// 1-1024 bytes) on the Logos Blockchain from the node key `from_key`, which pays the
    /// fee. Each key has its own Ed25519 inscription key, made here once and kept 0600,
    /// and its own channel (`sha256("lp-0022:testimonial:" || signer)`), which the first
    /// inscription creates and later ones extend. Without `send` the plan `{from, balance,
    /// signer, channel, parent, first, submission, text, bytes, msg_id, fee_estimate}`
    /// (the fee the node asks now, or absent when it couldn't say). A plan moves and
    /// holds nothing: the node prices it on stand-ins for the key's notes (`chain_fee`),
    /// so any number of plans, sends and splits can follow at once. With `send` the
    /// inscription is funded, signed and sent (it cannot be undone) and `hash` and `fee`
    /// (what it pays) are set; `chain_tx` follows it. A send the node refused after
    /// funding it leaves that key's notes held for about 10 final blocks: the same
    /// inscription sent again uses that funding, anything else from the key meanwhile is
    /// refused as held (not short). Names app only. Through logosctl, write
    /// a text or key that is only digits, or `true`/`false`, as `str:2026`:
    /// logosctl 0.3.2 sends it as a number or boolean otherwise, and the host refuses the
    /// call ("dispatch_failed: expected string") before names runs.
    fn chain_inscribe(&mut self, from_key: String, text: String, send: bool) -> String;

    /// Any app: asks for `text` (a testimonial, 1-1024 bytes) to be inscribed on the
    /// Logos Blockchain. The person picks which of the node's chain accounts inscribes
    /// it and pays its fee, and approves it in the Names app. No LEZ access is needed
    /// first: the request names no account. Answers as `request_status` does, kind
    /// `inscribe`; once done, `tx` is the transaction's hash and `account` the chain key.
    /// Through logosctl, a text of only digits or `true`/`false` is written
    /// `str:<text>` (see `chain_inscribe`).
    fn propose_inscription(&mut self, text: String) -> String;

    /// The inscription requests waiting for the person, oldest first: `[{id, app, text,
    /// says, submission, publisher, choices: [{key, balance, label?, title?}]}]`, the choices the keys
    /// the node's wallet holds. When the node can't list them (stopped, say), each request
    /// is still listed, with `choices` empty and `node` saying why, so it can be rejected.
    /// A choice carries the `label` the person gave the account (or its keystore `title`),
    /// and an account hidden while it holds nothing is left out, as in `chain_accounts`.
    /// The fee for a choice is `chain_inscribe(key, text, false)`'s `fee_estimate`, which
    /// holds nothing. `pending_requests` doesn't list these. Names app only.
    fn pending_inscriptions(&mut self) -> String;

    /// Approves inscription request `request_id` with the node key `from_key`: it is
    /// inscribed (`chain_inscribe`) and the request is `done` with `tx`. Refused before
    /// anything is sent (a key the node doesn't hold, too little for the fee), the
    /// request stays pending. Names app only.
    fn approve_inscription(&mut self, request_id: String, from_key: String) -> String;

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
    /// The LEZ zones set up here and the one in use (`zones`). `sequencer`, `cli_wallet`
    /// and `token_program` above are the zone in use's, and are what every method reads;
    /// `program`, the registry, is always home's, where LNS names are kept.
    zones: zones::Zones,
    /// Whether home's sequencer was asked for its channel in this run (`home_channel`).
    channel_asked: bool,
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
    /// When `value` was read: how old a list kept through failed reads is.
    read_at: Option<std::time::Instant>,
}

/// Where this module keeps what it remembers between runs.
#[cfg(not(test))]
fn config_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".config/logos-names"))
}

/// Tests keep theirs in a folder of their own, never the person's: one per test (its
/// thread), since tests run at once and the history and sends files are read, changed
/// and written whole.
#[cfg(test)]
fn config_dir() -> Option<std::path::PathBuf> {
    let thread = format!("{:?}", std::thread::current().id()).chars().filter(char::is_ascii_digit).collect::<String>();
    Some(std::env::temp_dir().join(format!("logos-names-test-config-{}-{thread}", std::process::id())))
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
            zones: zones::Zones::new(bare_home(DEFAULT_SEQUENCER)),
            channel_asked: false,
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
    // Plain URL characters only, and no `user@` part: a backslash, a space or an `@` is
    // read one way here and another by the HTTP library, which let `http://` reach any
    // host (SF-2). A sequencer address never needs them.
    let plain = |c: char| c.is_ascii_alphanumeric() || "-._~:/?#[]!$&'()*+,;=%".contains(c);
    if url.is_empty() || !url.chars().all(plain) {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return rest.split(['/', '?', '#']).next().is_some_and(|host| !host.is_empty());
    }
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
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
const MODULE_VERSION: &str = "0.15.3";

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

/// The part of the configuration other apps may read: the network, not the person.
/// `sequencer` and `token_program` are the zone in use's, and `zone` says which that is
/// (never what the person calls it); `registry` is read through `names_sequencer`, home's.
fn public_config(config: &Value) -> Value {
    json!({
        "sequencer": config["sequencer"],
        "names_sequencer": config["names_sequencer"],
        "registry": config["registry"],
        "token_program": config["token_program"],
        "zone": { "id": config["zone"]["id"], "channel": config["zone"]["channel"], "home": config["zone"]["home"] },
        "module_version": config["module_version"],
    })
}

/// The home zone as the configuration's own fields give it: `sequencer`, and nothing
/// known or chosen yet.
fn bare_home(sequencer: &str) -> zones::Zone {
    zones::Zone { name: String::new(), channel: None, sequencer: sequencer.to_owned(), wallet: None, token_program: None }
}

/// How long a sequencer is given to say its channel when zones are set up or changed: a
/// person is waiting, and the host gives a call a few seconds more than this.
const CHANNEL_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// What a sequencer's address must be (`is_http_url`).
const NOT_A_SEQUENCER: &str = "a sequencer's address starts with https://, or with http:// when it is on this computer";

/// Puts a LEZ job that started (its reply `out`) in the Wallet's history, as `entry` with
/// the job's id; it settles when the job ends (`activity`).
fn log_started(out: &str, mut entry: Value) {
    let started: Value = serde_json::from_str(out).unwrap_or(Value::Null);
    if started["success"] != true {
        return;
    }
    if let (Some(dir), Some(job)) = (config_dir(), started["value"]["id"].as_str()) {
        entry["job"] = Value::from(job);
        activity::add(&dir, now(), entry);
    }
}

/// Refuses to pay the public account `to` (base58) by its ID when it is another wallet's
/// and LEZ has never seen it: the wallet's send to such an account lands, keeps its fee
/// and credits nothing (pay_account-to-unseen, F2X). One of this wallet's own accounts
/// is paid either way; a sequencer that can't be asked leaves the send to it.
fn payable_account(setup: &cli::Setup, to: &str) -> Result<(), String> {
    let own = cli::balances_now(setup).is_ok_and(|rows| rows.iter().any(|(account, _)| *account == format!("Public/{to}")));
    if own {
        return Ok(());
    }
    let id = registry::parse_account(to)?;
    match registry::account_seen(&setup.sequencer, &id) {
        Ok(false) => Err(format!(
            "Public/{to} has never been used on LEZ, so it can't be paid by its ID: the payment would cost its fee and credit nothing. Check the ID, or pay its holder's name instead."
        )),
        _ => Ok(()),
    }
}

/// Refuses a job while the sequencer doesn't answer, before anything is made or started: a
/// job that fails at its first read has already changed the wallet (`receive-privately`
/// made a new private account each time:
/// receive-privately-during-outage-leaves-orphan-private-accounts). The sequencer must say
/// its last block and, for a job about a name, read the registry's record of it (a
/// sequencer can do the first and fail account reads).
fn chain_answers(setup: &cli::Setup, about: Option<(&registry::AccountId, &str)>) -> Result<(), String> {
    let refused = |why: String| format!("{}. Nothing was started; try again when it answers.", why.trim().trim_end_matches(['.', ' ']));
    registry::sequencer_reachable(&setup.sequencer).map_err(refused)?;
    if let Some((program, name)) = about {
        registry::name_readable(&setup.sequencer, program, name).map_err(refused)?;
    }
    Ok(())
}

/// Notes what `chain`'s accounts hold now, as read from `scope` (the sequencer, for LEZ),
/// in the Wallet's history (arrivals).
fn note_balances(chain: &str, scope: &str, balances: &[(String, u128)]) {
    if let Some(dir) = config_dir() {
        activity::note_balances(&dir, now(), chain, scope, balances);
    }
}

fn reply(outcome: Result<Value, String>) -> String {
    match outcome {
        Ok(value) => json!({ "success": true, "value": value, "error": null }),
        Err(error) => json!({ "success": false, "value": null, "error": error }),
    }
    .to_string()
}

/// `reply`, with `marks` (an object, or null for none) beside a success: what a read that
/// is partial or stale says about itself, `{partial, unread, stale, as_of, warning}`, so it
/// is never mistaken for a whole, fresh one.
fn reply_marked(outcome: Result<(Value, Value), String>) -> String {
    match outcome {
        Ok((value, marks)) => merged(json!({ "success": true, "value": value, "error": null }), &marks).to_string(),
        Err(error) => reply(Err(error)),
    }
}

/// A span of time as a sentence says it: "40 s", "3 min", "2 h".
fn age_text(ms: u64) -> String {
    match ms / 1000 {
        secs @ 0..=89 => format!("{} s", secs.max(1)),
        secs @ 90..=5399 => format!("{} min", (secs + 30) / 60),
        secs => format!("{} h", (secs + 1800) / 3600),
    }
}

/// What a list kept from an earlier read says beside it while reading it again fails:
/// `{stale, as_of, warning}` (cli-mine-stale-list-unmarked-during-outage).
fn stale_marks(why: &str, age_ms: u64, what: &str) -> Value {
    let why = why.trim().trim_end_matches(['.', ' ']);
    let mut letters = why.chars();
    let why = letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect());
    json!({
        "stale": true,
        "as_of": now().saturating_sub(age_ms),
        "warning": format!("{what} as read {} ago. {why}.", age_text(age_ms)),
    })
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
        // The fields above are home's. The zone in use, if it is another, takes their place.
        let home = zones::Zone {
            wallet: self.cli_wallet.clone(),
            token_program: self.token_program.as_ref().map(registry::account_text),
            ..bare_home(&self.sequencer)
        };
        self.zones = zones::Zones::load(saved, home);
        // A zone whose wallet folder can't be used now is not entered: home is, and the
        // zone stays set up.
        if !self.on_home() && self.enter_zone().is_err() {
            self.zones.use_zone(0);
        }
    }

    /// What is saved. `sequencer`, `cli_wallet` and `token_program` are home's whichever
    /// zone is in use, as they were before there were zones: a version without zones
    /// reads them and finds home.
    fn config(&self) -> Value {
        let zones = self.zones_now();
        let home = zones.home();
        json!({
            "sequencer": home.sequencer,
            "registry": self.program.as_ref().map(registry::account_text),
            "wallet_config": self.wallet_files.as_ref().map(|(config, _)| config),
            "wallet_storage": self.wallet_files.as_ref().map(|(_, storage)| storage),
            "cli": self.cli,
            "cli_wallet": home.wallet,
            "follows": self.follows,
            "main_name": self.main_name,
            "bridge_key": self.bridge_key,
            "bridge_node": self.bridge_node,
            "likes": self.likes,
            "dm_names": self.dm_names,
            "chat_names": self.chat_names,
            "token_program": home.token_program,
            "zones": zones.json(),
            "zone": zones.current_id(),
        })
    }

    /// The configuration as `get_config` shows it: the CLI and its wallet
    /// directory as they will be used, defaults filled in (`cli` is null if
    /// no CLI is found).
    fn shown_config(&self) -> Value {
        let mut config = self.config();
        let zones = self.zones_now();
        // What is in use now: the zone's own sequencer, wallet and token program, with
        // home's sequencer (where names are read) beside them.
        config["names_sequencer"] = config["sequencer"].take();
        config["sequencer"] = json!(self.sequencer);
        config["token_program"] = json!(self.token_program.as_ref().map(registry::account_text));
        config["zone"] = json!({ "id": zones.current_id(), "name": zones.zone().name, "channel": zones.zone().channel, "home": zones.current() == 0 });
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
        // The balance was read from the old network (CR-3).
        cli::forget_balance();
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
        let read = reads.entry(key.to_owned()).or_default();
        read.value = Some(list);
        read.read_at = Some(std::time::Instant::now());
    }

    /// How long ago the value kept under `key` was read, in ms (0 if never told).
    fn read_age_ms(&self, key: &str) -> u64 {
        let reads = self.board_reads.lock().expect("the board reads lock");
        reads.get(key).and_then(|read| read.read_at).map_or(0, |at| u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX))
    }

    /// The CLI's names changed (a registration or transfer ended): the next
    /// `cli_mine` waits for a fresh list.
    fn forget_mine(&self) {
        self.board_reads.lock().expect("the board reads lock").retain(|key, _| !key.starts_with("cli_mine:"));
        // The wallet or its default account changed: so did the balance (CR-2).
        cli::forget_balance();
    }

    /// Drops what the cache knows about `typed`, after a change to it.
    fn forget_reads(&mut self, typed: &str) {
        let name = registry::normalize_name(typed);
        self.read_cache.retain(|(_, cached), _| *cached != name);
    }

    fn board_read(&self, key: String, fetch: impl FnOnce() -> Result<Value, String> + Send + 'static) -> Value {
        {
            let mut reads = self.board_reads.lock().expect("the board reads lock");
            // Any app may read the board, so what one can make the module do is bounded
            // (SF-1): so many answers kept, so many CLI runs at once.
            let fetching = reads.values().filter(|read| read.updating).count();
            if !reads.contains_key(&key) {
                if fetching >= MAX_BOARD_FETCHES {
                    return json!({ "posts": [], "updating": false, "error": "The board is busy; try again in a moment." });
                }
                let board = reads.keys().filter(|kept| is_board_key(kept)).count();
                if board >= MAX_BOARD_READS {
                    let spare = reads.iter().find(|(kept, read)| is_board_key(kept) && !read.updating).map(|(kept, _)| kept.clone());
                    match spare {
                        Some(spare) => {
                            reads.remove(&spare);
                        }
                        None => return json!({ "posts": [], "updating": false, "error": "The board is busy; try again in a moment." }),
                    }
                }
            }
            let read = reads.entry(key.clone()).or_default();
            if !read.updating && fetching < MAX_BOARD_FETCHES {
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
                            read.read_at = Some(std::time::Instant::now());
                        }
                        Err(error) => read.error = Some(error),
                    }
                });
            }
        }
        self.board_read_now(&key)
    }

    // ── Zones ────────────────────────────────────────────────────────────

    /// Whether the zone in use is home, where LNS names are kept.
    fn on_home(&self) -> bool {
        self.zones.current() == 0
    }

    /// Home's sequencer: the one names are read from and changed through.
    fn home_sequencer(&self) -> String {
        if self.on_home() {
            self.sequencer.clone()
        } else {
            self.zones.home().sequencer.clone()
        }
    }

    /// Home's CLI wallet folder as it was set (None for the default one).
    fn home_wallet(&self) -> Option<String> {
        if self.on_home() {
            self.cli_wallet.clone()
        } else {
            self.zones.home().wallet.clone()
        }
    }

    /// Whether home has a wallet already: what it holds was read from home's chain.
    fn home_has_wallet(&self) -> bool {
        cli::find_wallet(self.home_wallet().as_deref()).is_some_and(|dir| dir.join(cli::STORAGE_FILE).is_file())
    }

    /// The zones, with the one in use as the module's own fields have it now. `sequencer`,
    /// `cli_wallet` and `token_program` are what the methods read and the setters change,
    /// so they are written back here before the zones are saved, shown or changed.
    fn zones_now(&self) -> zones::Zones {
        let mut zones = self.zones.clone();
        let mut zone = zones.zone().clone();
        let home = zones.current() == 0;
        if zone.sequencer != self.sequencer {
            zone.sequencer = self.sequencer.clone();
            // Home's channel is known only for the sequencer that said it.
            if home {
                zone.channel = None;
            }
        }
        zone.wallet = match (&self.cli_wallet, &zone.channel) {
            // An added zone's own folder is its default, and isn't kept as a choice.
            (Some(dir), Some(channel)) if !home && config_dir().is_some_and(|config| zones::wallet_dir(&config, channel) == std::path::Path::new(dir)) => None,
            (dir, _) => dir.clone(),
        };
        zone.token_program = self.token_program.as_ref().map(registry::account_text);
        zones.put_current(zone);
        zones
    }

    /// The wallet folder an added zone uses: the one chosen for it, or its own under the
    /// module's folder, made here if it is new. Never home's: a LEZ wallet's file holds
    /// what it last read from one chain. A chosen folder that can't be used now (missing,
    /// or not this user's alone) is an error, not a reason to open another wallet.
    fn zone_wallet(zone: &zones::Zone) -> Result<String, String> {
        if let Some(dir) = zone.wallet.as_deref() {
            return if cli::is_owned_safely(std::path::Path::new(dir), true, false) {
                Ok(dir.to_owned())
            } else {
                Err(format!("{}'s wallet folder {dir} can't be used now: it is missing, or isn't yours alone. Put it back to use this zone.", zone.name))
            };
        }
        let channel = zone.channel.as_deref().ok_or("this zone has no channel")?;
        let dir = zones::wallet_dir(&config_dir().ok_or("this device has nowhere to keep a zone's wallet")?, channel);
        cli::private_dir(&dir).map_err(|error| format!("couldn't make {}'s wallet folder: {error}", zone.name))?;
        Ok(dir.to_string_lossy().into_owned())
    }

    /// Points the module at the zone in use: its sequencer, its wallet folder and its
    /// token program. Nothing changes when its wallet folder can't be used.
    fn enter_zone(&mut self) -> Result<(), String> {
        let zone = self.zones.zone().clone();
        let wallet = if self.on_home() { zone.wallet.clone() } else { Some(Self::zone_wallet(&zone)?) };
        self.sequencer = zone.sequencer;
        self.cli_wallet = wallet;
        self.token_program = zone
            .token_program
            .as_deref()
            .and_then(|program| registry::parse_account(program).ok())
            .filter(|program| *program != registry::NATIVE_TOKEN);
        Ok(())
    }

    /// Where the wallet of zone `at` of `zones` is kept; it may not be made yet.
    fn zone_dir(zones: &zones::Zones, at: usize) -> Option<std::path::PathBuf> {
        let zone = zones.at(at)?;
        if at == 0 {
            return cli::find_wallet(zone.wallet.as_deref());
        }
        match (&zone.wallet, &zone.channel) {
            (Some(dir), _) => Some(std::path::PathBuf::from(dir)),
            (None, Some(channel)) => config_dir().map(|dir| zones::wallet_dir(&dir, channel)),
            (None, None) => None,
        }
    }

    /// The zone, other than the one in use, whose wallet is kept in `dir`: one folder
    /// holds one chain's wallet.
    fn wallet_taken(&self, dir: &std::path::Path) -> Option<String> {
        let zones = self.zones_now();
        let real = |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        let wanted = real(dir);
        (0..zones.len())
            .filter(|at| *at != zones.current())
            .find(|at| Self::zone_dir(&zones, *at).is_some_and(|other| real(&other) == wanted))
            .and_then(|at| zones.at(at).map(|zone| zone.name.clone()))
    }

    /// The zones as the Names app lists them (`zone_list`).
    fn zones_view(&self) -> Value {
        let zones = self.zones_now();
        let list: Vec<Value> = (0..zones.len())
            .filter_map(|at| {
                let zone = zones.at(at)?;
                let dir = Self::zone_dir(&zones, at);
                Some(json!({
                    "id": zones.id(at),
                    "name": zone.name,
                    "channel": zone.channel,
                    "sequencer": zone.sequencer,
                    "home": at == 0,
                    "current": at == zones.current(),
                    "wallet_dir": dir.as_ref().map(|dir| dir.to_string_lossy().into_owned()),
                    "has_wallet": dir.as_ref().is_some_and(|dir| dir.join(cli::STORAGE_FILE).is_file()),
                    "token_program": zone.token_program,
                }))
            })
            .collect();
        json!({ "zones": list, "current": zones.current_id(), "max": zones::MAX_ZONES })
    }

    /// Home's channel. Until it is known, home's sequencer is asked for it once a run
    /// (`again`: now, whatever was asked before), and the answer is saved: from then on
    /// another sequencer is told to serve home or not, and home's balance marks are kept
    /// under the channel, so they survive a change of sequencer. None while the sequencer
    /// doesn't say, or names a channel an added zone already has.
    fn home_channel(&mut self, again: bool) -> Option<String> {
        let mut zones = self.zones_now();
        if let Some(channel) = &zones.home().channel {
            return Some(channel.clone());
        }
        if self.channel_asked && !again {
            return None;
        }
        self.channel_asked = true;
        let sequencer = zones.home().sequencer.clone();
        let channel = registry::channel_id_within(&sequencer, CHANNEL_WAIT).ok().and_then(|read| zones::channel(&read))?;
        if zones.holder(&channel, Some(0)).is_some() {
            return None;
        }
        zones.at_mut(0)?.channel = Some(channel.clone());
        self.zones = zones;
        if let Some(dir) = config_dir() {
            activity::move_marks(&dir, "lez", &sequencer, &channel);
        }
        self.save();
        Some(channel)
    }

    /// Has zone `index` use the sequencer at `url`, and returns the zone's channel. The
    /// zone stays the same zone: its wallet, accounts and history are untouched, so the new
    /// sequencer must name the zone's own channel (`zones::sequencer_change`). Home may
    /// take another chain when the person said so (`other_chain_ok`) or has no wallet yet.
    fn change_sequencer(&mut self, index: usize, url: &str, other_chain_ok: bool) -> Result<Option<String>, zones::Refused> {
        let refused = |why: &str| zones::Refused { why: why.to_owned(), other_chain: None };
        let url = url.trim();
        if !is_http_url(url) {
            return Err(refused(NOT_A_SEQUENCER));
        }
        if index == 0 {
            // What home is now, so the new address can be told to serve it or not.
            self.home_channel(true);
        }
        let mut zones = self.zones_now();
        let zone = zones.at(index).cloned().ok_or_else(|| refused("there is no such zone here"))?;
        if zone.sequencer == url {
            return Ok(zone.channel);
        }
        let read = registry::channel_id_within(url, CHANNEL_WAIT)
            .and_then(|read| zones::channel(&read).ok_or_else(|| "its channel isn't 64 hex characters".to_owned()));
        let holder = read.as_ref().ok().and_then(|channel| zones.holder(channel, Some(index))).and_then(|at| zones.at(at)).map(|zone| zone.name.clone());
        let take_other = index == 0 && (other_chain_ok || !self.home_has_wallet());
        let channel = zones::sequencer_change(&zone.name, zone.channel.as_deref(), read.as_deref().map_err(String::as_str), holder.as_deref(), take_other)?;
        let in_use = index == zones.current();
        if let Some(changed) = zones.at_mut(index) {
            changed.sequencer = url.to_owned();
            changed.channel = channel.clone();
        }
        self.zones = zones;
        if in_use {
            self.sequencer = url.to_owned();
        }
        if in_use || index == 0 {
            // Names are read from home's sequencer, everything else from the zone in use's.
            self.forget_all_reads();
            self.token_info.clear();
        }
        self.save();
        Ok(channel)
    }

    /// The CLI as names use it: home's sequencer and home's wallet, whichever zone is in use.
    fn names_setup(&self) -> Result<cli::Setup, String> {
        let mut setup = self.cli_setup()?;
        if !self.on_home() {
            setup.wallet = cli::find_wallet(self.home_wallet().as_deref()).ok_or("no wallet directory for the names CLI")?;
            setup.sequencer = self.home_sequencer();
        }
        Ok(setup)
    }

    /// Home's wallet's accounts: the ones that hold names.
    fn names_held(&self) -> Result<Held, String> {
        wallet::accounts(&self.names_setup()?)
    }

    /// Refuses a change to a name, or a payment to one, while another zone is in use:
    /// names are kept on home, and its wallet signs and pays for them.
    fn names_here(&self) -> Result<(), String> {
        if self.on_home() {
            return Ok(());
        }
        let (home, zone) = (&self.zones.home().name, &self.zones.zone().name);
        Err(format!("names are kept on {home}, and this wallet is using {zone} now. Switch to {home} to do this. Nothing was started."))
    }

    /// `log_started`, with the entry said to be on the zone in use.
    fn log_started(&self, out: &str, entry: Value) {
        log_started(out, activity::in_zone(entry, &self.zones.current_id()));
    }

    /// Notes what the zone in use's LEZ accounts hold now, in the Wallet's history (arrivals).
    fn note_lez(&self, rows: &[(String, u128)]) {
        if let Some(dir) = config_dir() {
            let zones = self.zones_now();
            activity::note_zone_balances(&dir, now(), zones.zone().scope(), &zones.current_id(), rows);
        }
    }

    /// `key` for a read kept in the background, as the zone in use's own: a read that
    /// ends after a switch is then never shown as another zone's.
    fn zone_key(&self, key: &str) -> String {
        format!("{key}@{}", self.zones.current_id())
    }

    /// Refuses a request another app made for another zone than the one in use, or for a
    /// chain the zone no longer is: the same account ID is another account there.
    fn asked_here(&self, request: &Value) -> Result<(), String> {
        let asked = request["zone"].as_str().unwrap_or(zones::HOME);
        let zones = self.zones_now();
        if asked == zones.current_id() {
            return match (request["zone_channel"].as_str(), zones.zone().channel.as_deref()) {
                (Some(asked), Some(now)) if asked != now => {
                    Err("the chain this was asked for has changed since. Reject it, and the app can ask again.".to_owned())
                }
                _ => Ok(()),
            };
        }
        match zones.index(asked).and_then(|at| zones.at(at)) {
            Some(zone) => Err(format!(
                "this was asked for {}, and the wallet is using {} now. Switch to {} to approve it, or reject it.",
                zone.name,
                zones.zone().name,
                zone.name
            )),
            None => Err("this was asked for a zone that has been removed since. Reject it, and the app can ask again.".to_owned()),
        }
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

    /// A registration the CLI is finishing in the background for this wallet, as a running job
    /// (`cli::background_registrations`), or None. It proves and saves the wallet like a job.
    fn background_job(&self) -> Option<Value> {
        let setup = self.cli_setup().ok()?;
        cli::background_registrations(&setup.wallet).into_iter().next()
    }

    /// What holds the wallet now: a running CLI job other than an install, or a registration
    /// finishing in the background; None when nothing does (`cli_status.busy`).
    fn wallet_busy(&self) -> Option<Value> {
        let running = self.jobs().ok().and_then(|jobs| jobs.running().into_iter().find(|job| job["name"] != cli::INSTALL_JOB));
        running.or_else(|| self.background_job())
    }

    /// Refuses while a registration finishes in the background (the module's own jobs are
    /// refused by `Jobs::prepare`): it saves the wallet when it ends, from the copy it read.
    fn no_background_job(&self) -> Result<(), String> {
        self.background_job().map_or(Ok(()), |job| Err(cli::busy_with(&job)))
    }

    /// Starts a wallet job (`cli_shield`, `cli_pay_account`).
    fn wallet_job(&self, kind: &str, build: impl FnOnce(&cli::Setup) -> Result<Vec<String>, String>) -> Result<Value, String> {
        let setup = self.cli_setup()?;
        setup.wallet_ready()?;
        let command = build(&setup)?;
        self.no_background_job()?;
        chain_answers(&setup, None)?;
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
            None => self.no_background_job(),
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
            Some("receive") if !running && !done => {
                // The CLI says what landed and what stopped; a run that claimed some is partly
                // done, and says so (partial-claim-job-and-pages-say-nothing-true). One that
                // claimed nothing fails like every other job, below.
                let whole = state["id"].as_str().and_then(|id| self.jobs().ok().map(|jobs| jobs.whole_log(id))).unwrap_or_else(|| log.clone());
                let (_, claims) = cli::parse_claims(&whole);
                if !claims.is_empty() {
                    state["problem"] = Value::from(format!("Claiming finished only in part: {}", cli::failure_cause(&log)));
                }
            }
            Some("token_send" | "token_receive") if !running => {
                if let Some(fields) = cli::token_outcome(&log).as_object() {
                    for (key, value) in fields {
                        state[key.as_str()] = value.clone();
                    }
                }
                // What the wallet holds has changed.
                self.forget_tokens();
            }
            _ => {}
        }
        // Every job that failed says what didn't happen and why in one line, built from
        // the CLI's error, never its last cause line (transfer-failure-text-is-last-cause-line).
        let failed = !running && !done && state["state"] != "unknown";
        if failed && !state["problem"].as_str().is_some_and(|problem| !problem.is_empty()) {
            let kind = state["kind"].as_str().unwrap_or_default().to_owned();
            state["problem"] = Value::from(cli::job_problem(&kind, &log));
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
            self.names_here()?;
            let setup = self.cli_setup()?;
            setup.wallet_ready()?;
            self.no_background_job()?;
            chain_answers(&setup, self.program.as_ref().map(|program| (program, name.as_str())))?;
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
        // Names are kept on home, whichever zone is in use.
        Ok(Registry::new(&self.home_sequencer(), program))
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
        self.names_here()?;
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
            self.names_here()?;
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
        self.names_here()?;
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

    /// Refuses a hand-over to `to` (`Public/<id>` or `Private/<id>`) that would most
    /// likely leave the name held by nobody: a program's account (the registry's, the
    /// token program's), or a public account LEZ has never seen that isn't this wallet's
    /// (a mistyped ID can't be told from a new account otherwise).
    fn may_hold(&self, to: &str) -> Result<(), String> {
        let id = registry::parse_account(to)?;
        self.not_a_sink(&id, "hold a name")?;
        if let (Some(bare), Ok(setup)) = (to.strip_prefix("Public/"), self.names_setup()) {
            let own = cli::balances_now(&setup).is_ok_and(|rows| rows.iter().any(|(account, _)| account == to));
            if !own && registry::account_seen(&setup.sequencer, &id) == Ok(false) {
                return Err(format!(
                    "Public/{bare} has never been used on LEZ: check the ID, since a mistyped one would leave the name held by nobody. A new account can receive any payment first."
                ));
            }
        }
        Ok(())
    }

    /// The wallet's account that holds `name` (`Public/…` or `Private/…`), found as the CLI
    /// finds it: the one whose tag the name's record holds. `None` when it can't be told
    /// (no record, the sequencer or the CLI not answering, an account `names balance`
    /// doesn't list).
    fn holder_of(&mut self, name: &str) -> Option<String> {
        let name = registry::parse_name(name).ok()?;
        let registry = self.registry().ok()?;
        let derived = self.derived(&name);
        let record = registry.record(&name, &derived).ok()??;
        let rows = cli::balances_now(&self.names_setup().ok()?).ok()?;
        rows.into_iter()
            .map(|(account, _)| account)
            .find(|account| registry::parse_account(account).is_ok_and(|id| registry::owner_tag(&derived.id, &id) == record.owner))
    }

    /// Refuses when `to` (`Public/…` or `Private/…`) already holds `name`, by the tag its
    /// record holds. A record that can't be read leaves it to the CLI.
    fn not_held_by(&mut self, name: &str, to: &str) -> Result<(), String> {
        let (Ok(registry), Ok(account)) = (self.registry(), registry::parse_account(to)) else { return Ok(()) };
        let derived = self.derived(name);
        match registry.record(name, &derived) {
            Ok(Some(record)) if record.owner == registry::owner_tag(&derived.id, &account) => {
                Err(format!("{} already holds {name}, so nothing was sent", registry::account_text(&account)))
            }
            // A name nobody registered has nothing to hand over: refused before any job
            // and any history entry (cli-transfer-unregistered-name-starts-failing-job).
            Ok(None) => Err(format!("{name} is not registered, so nothing was sent")),
            _ => Ok(()),
        }
    }

    /// Refuses an account nobody can use what is sent to it: the empty account (all 1s)
    /// and a program's own (the registry's, the token program's). `what` is what it
    /// would be given, in words ("hold a name", "take a bridge").
    fn not_a_sink(&self, id: &AccountId, what: &str) -> Result<(), String> {
        if id.iter().all(|byte| *byte == 0) {
            return Err("that is the empty account (all 1s), which no one holds".to_owned());
        }
        // Every registry this module knows, whichever is configured now
        // (not-a-sink-forgets-default-registry-after-set-registry).
        let known_registry = std::iter::once(DEFAULT_REGISTRY)
            .chain(RETIRED_REGISTRIES)
            .any(|program| registry::parse_account(program).is_ok_and(|program| program == *id));
        if known_registry || Some(*id) == self.program || Some(*id) == self.token_program {
            return Err(format!("that is a program's account, which can't {what}"));
        }
        Ok(())
    }

    /// Settles the history's entries still on their way, from the chain and the jobs, and
    /// returns the balances to note (`rows`, what the wallet's LEZ accounts hold as read
    /// from `sequencer`): an account a bridge goes to is read afresh, or left out when its
    /// read can't be told apart from that bridge's arrival.
    fn settle_activity(&self, dir: &std::path::Path, sequencer: &str, rows: &[(String, u128)], deadline: std::time::Instant) -> Vec<(String, u128)> {
        for entry in activity::all(dir).into_iter().filter(|entry| entry["state"] == "sending" || entry["state"] == "in_block") {
            let id = entry["id"].as_str().unwrap_or_default().to_owned();
            let mut settled = Value::Null;
            match (entry["chain"].as_str(), entry["state"].as_str()) {
                // On the Logos Blockchain: in a block once its input notes are spent.
                (Some("logos"), Some("sending")) => {
                    if let Some(hash) = entry["hash"].as_str() {
                        // Without its record here (chain-sent.json lost or pruned), the node
                        // is asked what it spends; a send that lands is never left sending
                        // (logos-send-stuck-sending-without-chain-sent-record).
                        let landed = match chain_landed(hash) {
                            Err(error) if error == NOT_SENT_HERE => entry["from"].as_str().map_or(Err(error), |from| chain_landed_asked(hash, from)),
                            other => other,
                        };
                        if landed == Ok(true) {
                            settled = json!({ "state": "in_block" });
                        } else if now().saturating_sub(entry["at"].as_u64().unwrap_or(0)) >= activity::SENDING_WAIT_S {
                            // Not seen in a block for this long: whether it ever will be is unknown.
                            settled = json!({ "state": "unknown" });
                        }
                    } else if let (Some(note), Some(from)) = (entry["note"].as_str(), entry["from"].as_str()) {
                        // A deposit whose call was cut off: made once its note is spent, not
                        // made if the note is still there once nothing holds it any more.
                        if let Ok(notes) = node::call("wallet_get_notes", json!([from, ""]), node::QUICK).map(|reply| chain::notes_of(&reply)) {
                            if !notes.contains_key(note) {
                                settled = json!({ "state": "in_block" });
                                // Its transaction, found by the deposit's op id in the recent
                                // blocks' events: its hash and the fee it paid
                                // (bridge-settled-after-unload-has-no-hash-or-fee).
                                if let Some((hash, fee)) = activity::bridge_op_id(&entry).and_then(|op| deposit_made(&op, from, note, &entry)) {
                                    settled["hash"] = json!(hash);
                                    settled["fee"] = fee.map_or(Value::Null, |fee| json!(fee.to_string()));
                                }
                            } else if now().saturating_sub(entry["at"].as_u64().unwrap_or(0)) >= NOTES_HELD_S {
                                settled = json!({ "state": "failed" });
                            }
                        }
                    }
                }
                _ => {}
            }
            // A LEZ job: as the job ended.
            if entry["chain"] == "lez" && entry["state"] == "sending" {
                if let (Some(job), Ok(jobs)) = (entry["job"].as_str(), self.jobs()) {
                    match jobs.state(job) {
                        Ok(state) if state["state"] != "running" => {
                            let log = jobs.whole_log(job);
                            settled = json!({
                                "state": match state["state"].as_str() { Some("done") => "done", Some("unknown") => "unknown", _ => "failed" },
                                "fee": cli::parse_fee(&log),
                            });
                            // A receive that claimed nothing is told apart from one that did.
                            if entry["kind"] == "receive" && cli::claimed_nothing(&log) {
                                settled["claimed"] = json!(false);
                            }
                            // Which account paid and which were credited, so their pages show
                            // the claim (receive-entry-has-no-account-amount).
                            if entry["kind"] == "receive" {
                                let (payer, claims) = cli::parse_claims(&log);
                                if let Some(payer) = payer {
                                    settled["from"] = json!(payer);
                                }
                                // A claim job that stopped after some claims landed is partly done,
                                // not failed: those accounts were credited and its payer paid
                                // (partial-claim-job-and-pages-say-nothing-true).
                                if settled["state"] == "failed" && !claims.is_empty() {
                                    settled["state"] = json!("partial");
                                }
                                // What still waits, by account, so the waiting account's page shows it.
                                let mut left = Map::new();
                                for (to, amount) in cli::parse_left(&log) {
                                    let had = left.get(&to).and_then(Value::as_str).and_then(|text| text.parse::<u128>().ok()).unwrap_or(0);
                                    left.insert(to, json!((had + amount).to_string()));
                                }
                                if !left.is_empty() {
                                    settled["left"] = Value::Object(left);
                                }
                                if !claims.is_empty() {
                                    let mut received = Map::new();
                                    for (to, amount) in &claims {
                                        let had = received.get(to).and_then(Value::as_str).and_then(|text| text.parse::<u128>().ok()).unwrap_or(0);
                                        received.insert(to.clone(), json!((had + amount).to_string()));
                                    }
                                    settled["amount"] = json!(claims.iter().map(|(_, amount)| amount).sum::<u128>().to_string());
                                    if received.len() == 1 {
                                        settled["to"] = json!(claims[0].0);
                                    }
                                    settled["received"] = Value::Object(received);
                                }
                            }
                        }
                        Ok(_) => {}
                        // The job's files were cleared: it ended long ago, outcome unknown
                        // (a failed send was shown done once its record was pruned).
                        Err(_) => settled = json!({ "state": "unknown" }),
                    }
                }
            }
            if settled.is_object() {
                activity::update(dir, &id, &settled);
            }
        }
        // A bridge arrives once LEZ has minted its own deposit (its receipt), never on a
        // balance that matches. Its account's balance is then read between two asks of the
        // receipts, so the read either holds the credit (minted at the first ask) or doesn't
        // (not minted at the second); a mint between them, or an ask LEZ doesn't answer,
        // leaves the account unread this time, so its credit is never also taken for money
        // that arrived (activity::note_balances).
        let at = now();
        let entries = activity::all(dir);
        // A bridge is followed on the zone it went to. One to the zone in use settles with
        // that zone's balances, below; one to another zone is only asked after at that
        // zone's own sequencer, and waits if the zone has been removed.
        let zones = self.zones_now();
        let here = zones.current_id();
        let went_to: HashMap<&str, &str> = entries.iter().filter_map(|entry| Some((entry["id"].as_str()?, activity::zone_of(entry)))).collect();
        let (followed, elsewhere): (Vec<_>, Vec<_>) = activity::bridges_followed(&entries, at)
            .into_iter()
            .partition(|(id, _, _)| went_to.get(id.as_str()).copied().unwrap_or(zones::HOME) == here);
        // Every read here shares the call's deadline, so the call answers inside the host's time
        // however many bridges wait; one past it is unread, and the bridge waits a round.
        let ask = |receipt: &registry::AccountId| registry::deposit_minted(sequencer, receipt, deadline).ok();
        let mut minted: std::collections::HashMap<registry::AccountId, Option<bool>> = followed.iter().map(|(_, _, receipt)| (*receipt, ask(receipt))).collect();
        for (id, _, receipt) in &elsewhere {
            let theirs = went_to.get(id.as_str()).and_then(|zone| zones.index(zone)).and_then(|at| zones.at(at)).map(|zone| zone.sequencer.clone());
            minted.insert(*receipt, theirs.and_then(|sequencer| registry::deposit_minted(&sequencer, receipt, deadline).ok()));
        }
        let mut rows = rows.to_vec();
        let accounts: std::collections::BTreeSet<&str> = followed.iter().map(|(_, to, _)| to.as_str()).collect();
        for to in accounts {
            let Some(at_row) = rows.iter().position(|(account, _)| account == to) else { continue };
            let receipts: Vec<registry::AccountId> = followed.iter().filter(|(_, bridge_to, _)| bridge_to == to).map(|(_, _, receipt)| *receipt).collect();
            let fresh = registry::parse_account(to).ok().and_then(|id| registry::public_balance(sequencer, &id, deadline).ok());
            let mut certain = fresh.is_some();
            for receipt in &receipts {
                match minted.get(receipt).copied().flatten() {
                    Some(true) => {}
                    Some(false) => {
                        let again = ask(receipt);
                        certain &= again == Some(false);
                        minted.insert(*receipt, again);
                    }
                    None => certain = false,
                }
            }
            match fresh.filter(|_| certain) {
                Some(balance) => {
                    rows[at_row].1 = balance;
                    // The wallet's next read must be no older than this one, or the
                    // account would seem to fall back and rise again, and the bridge's
                    // credit be told a second time as money that arrived.
                    cli::forget_balance();
                }
                None => {
                    rows.remove(at_row);
                }
            }
        }
        for (id, mut fields) in activity::bridge_updates(&entries, at, &|receipt| minted.get(receipt).copied().flatten()) {
            if fields["state"] == "arrived" {
                fields["arrived_at"] = json!(at);
            }
            activity::update(dir, &id, &fields);
        }
        rows
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
            // Names are kept on home: on another zone an account is only what it holds there.
            let home = self.on_home();
            let registry = if home { self.registry()? } else { Registry::new(&self.sequencer, self.program.unwrap_or([0; 32])) };
            let held = self.held()?;
            let summary = |account: &AccountId| -> Result<Value, String> {
                if home {
                    return registry.account_summary(account);
                }
                let balance = registry.balance(account)?;
                Ok(json!({ "account": registry::account_text(account), "balance": balance.to_string(), "funded": balance > 0, "name": null }))
            };
            // Each summary is a read over the network; they are read together.
            let mut accounts = std::thread::scope(|scope| {
                let readers: Vec<_> = held
                    .public
                    .iter()
                    .take(MAX_WALLET_ACCOUNTS)
                    .map(|account| scope.spawn(|| summary(account)))
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
            let held = self.names_held()?.public;
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
            let held = self.names_held()?.public;
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
        shortlist::use_cli(self.names_setup().ok());
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
        // Another app sees only what any reader of the chain could know: who you follow,
        // like and message, your main name and your files stay with the Names app (AUTH-1).
        if only_the_names_app().is_err() {
            return public_config(&self.shown_config()).to_string();
        }
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
        reply_marked((|| {
            let setup = self.names_setup()?;
            // The CLI reads every name it holds from the chain, which takes seconds:
            // answer with the last list and refresh it on its own thread.
            // Kept per wallet: another wallet holds other names.
            let key = format!("cli_mine:{}", setup.wallet.display());
            if self.mine_known(&key) {
                let cached = self.board_read(key.clone(), move || cli::run_mine(&setup).map(Value::from));
                // A refresh that failed leaves the last list, said to be from then.
                let marks = cached["error"].as_str().map_or(Value::Null, |why| stale_marks(why, self.read_age_ms(&key), "Your names"));
                return Ok((cached["posts"].clone(), marks));
            }
            let fresh = Value::from(cli::run_mine(&setup)?);
            self.remember_mine(&key, fresh.clone());
            Ok((fresh, Value::Null))
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
        // A page of text is held to what set-text and posts are (cli-set-page-forwards-control-chars).
        if let Err(error) = cli::page_text(&content_type, &body) {
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

    fn cli_transfer(&mut self, name: String, new_owner: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let command = match cli::transfer_args(&name, &new_owner) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        let to = command[3].clone();
        if let Err(refused) = self.may_hold(&to) {
            return reply(Err(refused));
        }
        // Handing a name to the account that holds it is refused here, before any job, as
        // the CLI would refuse it (module-self-transfer-starts-job-logs-failed).
        if let Err(refused) = self.not_held_by(&command[2], &to) {
            return reply(Err(refused));
        }
        let out = self.cli_job(&name, "transfer", |_, _, _| Ok((command, None, Value::Null)));
        self.log_started(&out, json!({ "kind": "transfer", "chain": "lez", "name": registry::normalize_name(&name), "to": to }));
        // The names it holds change: read them afresh.
        self.forget_mine();
        out
    }

    fn cli_receive_privately(&mut self, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let command = match cli::receive_privately_args(&name) {
            Ok(command) => command,
            Err(error) => return reply(Err(error)),
        };
        // The holder pays the fee, so its page lists it (receive-privately-entry-no-paying-account).
        let from = self.holder_of(&name);
        let out = self.cli_job(&name, "receive_privately", |_, _, _| Ok((command, None, Value::Null)));
        // In the history like a transfer, with the fee it paid once it ends.
        self.log_started(&out, json!({ "kind": "receive_privately", "chain": "lez", "name": registry::normalize_name(&name), "from": from }));
        out
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
        reply_marked((|| {
            let setup = self.cli_setup()?;
            setup.wallet_ready()?;
            let deadline = std::time::Instant::now() + cli::CALL_BUDGET;
            let read = cli::balance_read(&setup, deadline)?;
            // Only a read in which the sequencer gave every balance moves the History: an
            // unread balance is never a 0 (selective-degradation-cold-read-false-arrivals).
            if read.complete() {
                let rows = read.balances();
                // What settled first, so a send between the wallet's own accounts is not
                // also taken for money that arrived.
                let rows = match config_dir() {
                    Some(dir) => self.settle_activity(&dir, &setup.sequencer, &rows, deadline),
                    None => rows,
                };
                // The sequencer answers now: home's channel is learnt here if it isn't known.
                self.home_channel(false);
                self.note_lez(&rows);
            }
            Ok((read.reply(), read.marks()))
        })())
    }

    fn cli_bridge(&mut self, amount: String, from_key: String, node: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            // Everything is checked before anything is remembered or run.
            cli::amount(&amount)?;
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

    fn cli_bridge_to(&mut self, amount: String, from_key: String, to: String, node: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            cli::amount(&amount)?;
            let key = cli::bridge_key(&from_key)?;
            let node = cli::bridge_node(&node)?;
            // The destination too, before anything is remembered: a refused call leaves
            // config.json as it was (bridge-to-saves-before-validating).
            if let Some(to) = cli::bridge_to(&to)? {
                self.not_a_sink(&registry::parse_account(&to)?, "take a bridge")?;
            }
            let setup = self.cli_setup()?;
            if self.bridge_key.as_deref() != Some(key.as_str()) || self.bridge_node.as_deref() != Some(node.as_str()) {
                self.bridge_key = Some(key.clone());
                self.bridge_node = Some(node.clone());
                self.save();
            }
            cli::run_bridge_to(&setup, &amount, &key, &to, &node, send)
        })())
    }

    fn cli_faucet(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            if !self.on_home() {
                let home = &self.zones.home().name;
                return Err(format!("the faucet pays on {home} only. Switch to {home} to ask it."));
            }
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
                // Whatever holds the wallet: a wallet job, a change to a name, or a registration
                // finishing in the background (wallet-job-allowed-while-detached-registration-pending).
                "busy": self.wallet_busy().unwrap_or(Value::Null),
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
            self.no_background_job()?;
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
        let out = reply((|| {
            // An amount like every other, never 0; empty is setup's default.
            if !amount.trim().is_empty() {
                cli::amount(&amount)?;
            }
            let command = cli::setup_args(&amount)?;
            self.wallet_job("shield", |setup| {
                cli::default_public(setup)?;
                Ok(command)
            })
        })());
        let moved = cli::setup_args(&amount).ok().and_then(|command| command[1].strip_prefix("--shield=").map(str::to_owned));
        let (from, to) = self.cli_setup().ok().map_or((None, None), |setup| {
            (cli::default_public(&setup).ok().map(|id| format!("Public/{id}")), cli::default_private(&setup).ok().map(|id| format!("Private/{id}")))
        });
        self.log_started(&out, json!({ "kind": "shield", "chain": "lez", "from": from, "to": to, "amount": moved }));
        out
    }

    fn cli_receive(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let out = reply(self.wallet_job("receive", |setup| {
            if !cli::has_command(&setup.program, "receive") {
                return Err(cli::OLDER_CLI.to_owned());
            }
            Ok(vec!["receive".to_owned()])
        }));
        self.log_started(&out, json!({ "kind": "receive", "chain": "lez" }));
        out
    }

    fn cli_receive_all(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let out = reply(self.wallet_job("receive", |setup| {
            if !cli::has_command(&setup.program, "receive") {
                return Err(cli::OLDER_CLI.to_owned());
            }
            Ok(vec!["receive".to_owned(), "--all".to_owned()])
        }));
        self.log_started(&out, json!({ "kind": "receive", "chain": "lez", "all": true }));
        out
    }

    fn activity(&mut self, account: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let listed = (|| -> Result<(Value, Option<String>), String> {
            let dir = config_dir().ok_or("no folder for the Wallet's history")?;
            let deadline = std::time::Instant::now() + cli::CALL_BUDGET;
            let setup = self.cli_setup().ok();
            let read = setup.as_ref().and_then(|setup| cli::balances_now(setup).ok());
            let sequencer = setup.as_ref().map_or_else(|| self.sequencer.clone(), |setup| setup.sequencer.clone());
            let rows = self.settle_activity(&dir, &sequencer, read.as_deref().unwrap_or_default(), deadline);
            if setup.is_some() && read.is_some() {
                self.note_lez(&rows);
            }
            // A history file that couldn't be read was kept aside, never written over: say so
            // once. Only a lost history fails the call; a lost file of marks is a warning
            // beside the list.
            let damage = activity::damage(&dir);
            if let Some((true, why)) = damage {
                return Err(why);
            }
            // A LEZ account's page lists the zone in use only: the same keys make the same
            // account ID on every zone, and what one holds there is other money.
            let zones = self.zones_now();
            let here = zones.current_id();
            let lez_page = account.trim().starts_with("Public/") || account.trim().starts_with("Private/");
            let list = Value::from(
                activity::all(&dir)
                    .into_iter()
                    .filter(|entry| activity::involves(entry, &account))
                    .filter(|entry| !(lez_page && activity::on_lez(entry) && activity::zone_of(entry) != here))
                    .take(100)
                    .map(|mut entry| {
                        // What the zone is called now, once there is more than one to tell
                        // apart; null for a zone that has been removed.
                        if activity::on_lez(&entry) && (zones.len() > 1 || activity::zone_of(&entry) != zones::HOME) {
                            let name = zones.index(activity::zone_of(&entry)).and_then(|at| zones.at(at)).map(|zone| zone.name.clone());
                            entry["zone_name"] = json!(name);
                        }
                        entry
                    })
                    .collect::<Vec<_>>(),
            );
            Ok((list, damage.map(|(_, why)| why)))
        })();
        match listed {
            Ok((list, Some(warning))) => json!({ "success": true, "value": list, "error": null, "warning": warning }).to_string(),
            other => reply(other.map(|(list, _)| list)),
        }
    }

    fn cli_pay_account(&mut self, account: String, amount: String, from_private: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let out = reply((|| {
            // Checked before the CLI is looked for, so bad input fails the same way every time.
            let cli::PayTo::Account(to) = cli::pay_to(&account)? else {
                return Err("that is a name; pay it as a name".to_owned());
            };
            cli::amount(&amount)?;
            // A program's account and the empty one are refused as cli_bridge_to and
            // cli_transfer refuse them (pay-account-accepts-program-account).
            self.not_a_sink(&registry::parse_account(&to)?, "take a payment")?;
            if let Ok(setup) = self.cli_setup() {
                payable_account(&setup, &to)?;
            }
            self.wallet_job("pay_account", |setup| cli::pay_account_args(setup, &account, &amount, from_private))
        })());
        let from = self.cli_setup().ok().and_then(|setup| {
            if from_private { cli::default_private(&setup).ok().map(|id| format!("Private/{id}")) } else { cli::default_public(&setup).ok().map(|id| format!("Public/{id}")) }
        });
        self.log_started(&out, json!({ "kind": "pay_account", "chain": "lez", "from": from, "to": account.trim(), "amount": cli::amount_text(&amount).ok(), "private": from_private }));
        out
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
            let setup = self.names_setup()?;
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
            let setup = self.names_setup()?;
            let key = format!("author:{name}:{before}");
            Ok(self.board_read(key, move || cli::run_posts_of(&setup, &name, before)))
        })())
    }

    fn board_thread(&mut self, post: String) -> String {
        reply((|| {
            // Checked before it becomes a key: junk is refused, not kept (SF-1).
            let post = cli::post_ref(&post)?;
            let setup = self.names_setup()?;
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
            let setup = self.names_setup()?;
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
        let out = self.cli_job(&name, "pay", |_, _, _| Ok((command, None, Value::Null)));
        let from = self.cli_setup().ok().and_then(|setup| cli::default_private(&setup).ok()).map(|id| format!("Private/{id}"));
        self.log_started(&out, json!({ "kind": "pay", "chain": "lez", "from": from, "name": registry::normalize_name(&name), "amount": cli::amount_text(&amount).ok() }));
        out
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
                // It may have moved tokens: the next balance is read afresh.
                cli::forget_balance();
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
        let chosen = if dir.is_empty() {
            if self.on_home() {
                None
            } else {
                // An added zone's default is its own folder, never home's.
                let own = zones::Zone { wallet: None, ..self.zones.zone().clone() };
                match Self::zone_wallet(&own) {
                    Ok(dir) => Some(dir),
                    Err(_) => return false,
                }
            }
        } else if cli::is_owned_safely(std::path::Path::new(dir), true, false) {
            Some(dir.to_owned())
        } else {
            return false;
        };
        // One folder holds one chain's wallet: another zone's is refused.
        let used = chosen.as_deref().map(std::path::PathBuf::from).or_else(|| cli::find_wallet(None));
        if used.is_some_and(|dir| self.wallet_taken(&dir).is_some()) {
            return false;
        }
        self.cli_wallet = chosen;
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
        self.change_sequencer(self.zones.current(), &url, false).is_ok()
    }

    // ── Zones ───────────────────────────────────────────────────────────

    fn zone_list(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        self.home_channel(false);
        reply(Ok(self.zones_view()))
    }

    fn zone_add(&mut self, name: String, sequencer: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let url = sequencer.trim();
            if !is_http_url(url) {
                return Err(NOT_A_SEQUENCER.to_owned());
            }
            if !name.trim().is_empty() {
                zones::name(&name)?;
            }
            // Home's channel first: a sequencer that serves home is not another zone.
            if self.home_channel(true).is_none() {
                return Err(format!(
                    "{}'s own sequencer didn't say which chain it serves, so a new zone can't be told apart from it. Nothing was added; try again when it answers.",
                    self.zones.home().name
                ));
            }
            let read = registry::channel_id_within(url, CHANNEL_WAIT)
                .map_err(|why| format!("that sequencer didn't say which chain it serves, so it can't be added. ({})", why.trim().trim_end_matches('.')))?;
            let mut zones = self.zones_now();
            let at = zones.add(&name, url, &read)?;
            let added = zones.id(at);
            self.zones = zones;
            self.save();
            Ok(merged(self.zones_view(), &json!({ "added": added })))
        })())
    }

    fn zone_use(&mut self, zone: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let at = self.zones.find(&zone)?;
            let was = self.zones.current();
            if at != was {
                // A job saves the wallet it started with when it ends, and is followed by
                // the zone it runs on.
                self.no_job_running()?;
                self.zones = self.zones_now();
                self.zones.use_zone(at);
                if let Err(why) = self.enter_zone() {
                    self.zones.use_zone(was);
                    return Err(why);
                }
                // Everything read so far was the other zone's.
                self.forget_all_reads();
                self.token_info.clear();
                self.forget_tokens();
                self.forget_mine();
                self.save();
            }
            Ok(self.zones_view())
        })())
    }

    fn zone_remove(&mut self, zone: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let mut zones = self.zones_now();
            let at = zones.find(&zone)?;
            let dir = Self::zone_dir(&zones, at).map(|dir| dir.to_string_lossy().into_owned());
            let removed = zones.remove(at)?;
            self.zones = zones;
            self.save();
            // Its wallet folder stays where it is: adding the zone again finds it.
            Ok(merged(self.zones_view(), &json!({ "removed": { "name": removed.name, "channel": removed.channel, "wallet_dir": dir } })))
        })())
    }

    fn zone_rename(&mut self, zone: String, name: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let mut zones = self.zones_now();
            let at = zones.find(&zone)?;
            zones.rename(at, &name)?;
            self.zones = zones;
            self.save();
            Ok(self.zones_view())
        })())
    }

    fn zone_sequencer(&mut self, zone: String, url: String, other_chain_ok: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let at = self.zones.find(&zone)?;
            match self.change_sequencer(at, &url, other_chain_ok) {
                Ok(channel) => Ok(merged(self.zones_view(), &json!({ "changed": true, "channel": channel }))),
                // Home may take another chain, once the person has said so.
                Err(zones::Refused { why, other_chain: Some(channel) }) if at == 0 => {
                    Ok(json!({ "changed": false, "other_chain": true, "channel": channel, "why": why }))
                }
                Err(refused) => Err(refused.why),
            }
        })())
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

    fn watched_tokens(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let program = self.token_program_set()?;
            Ok(Value::from(tokens_watched(&program)))
        })())
    }

    fn watch_token(&mut self, definition: String, watch: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let definition = cli::token_definition(&definition)?;
            let program = self.token_program_set()?;
            let id = registry::account_text(&definition);
            let mut list = tokens_watched(&program);
            list.retain(|entry| entry["definition"] != id.as_str());
            if watch {
                if list.len() >= MAX_WATCHED_TOKENS {
                    return Err(format!("The wallet keeps at most {MAX_WATCHED_TOKENS} tokens added by address: remove one first."));
                }
                let info = self.token_info_of(&program, &definition)?;
                list.push(json!({ "definition": id, "name": info["name"] }));
            }
            save_tokens_watched(&program, &list)?;
            Ok(Value::from(list))
        })())
    }

    fn cli_token_send(&mut self, definition: String, amount: String, to: String, from_private: bool, to_public: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        let out = reply((|| {
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
        })());
        let from = self.cli_setup().ok().and_then(|setup| {
            if from_private { cli::default_private(&setup).ok().map(|id| format!("Private/{id}")) } else { cli::default_public(&setup).ok().map(|id| format!("Public/{id}")) }
        });
        self.log_started(&out, json!({ "kind": "token_send", "chain": "lez", "from": from, "to": to.trim(), "amount": cli::amount_text(&amount).ok(), "token": definition.trim() }));
        out
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
        // Accounts whose balance the sequencer didn't give are listed with it null, and
        // named beside the list (balance-accounts-zeroed-with-success-on-partial-failure).
        reply_marked((|| {
            let read = cli::run_accounts_any(&self.cli_setup()?)?;
            let marks = read.marks();
            Ok((Value::from(read.rows), marks))
        })())
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
            let known = keys.clone();
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
            // Labels, and the accounts made here (no notes yet, or not used before the node restarts).
            .map(|rows| chain_keys::annotate(rows, &known, &chain_keys::records(&known)))
            .inspect(|rows| {
                let held: Vec<(String, u128)> = rows
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|row| Some((row["key"].as_str()?.to_owned(), row["balance"].as_str()?.parse().ok()?)))
                    .collect();
                note_balances("logos", "", &held);
            })
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
            if !send {
                chain_fee_about(&mut plan, "fee_about", &chain_fee::Shape::Transfer { to: to_key.clone(), amount }, &from, None)?;
            }
            if send {
                let before = chain::notes_of(&node::call("wallet_get_notes", json!([from, ""]), node::QUICK)?);
                let sent = node::call("wallet_transfer_funds", json!([from, [from], to_key, amount.to_string(), ""]), node::PROVING)
                    .map_err(|error| in_flight_refusal(error, &from, &before))?;
                let hash = chain::hash_of(&sent);
                // Sent: from here nothing fails, so the hash always comes back.
                let stored = hash.as_ref().and_then(|hash| node::call("get_transaction", json!([hash]), node::QUICK).ok());
                let fee = stored.as_ref().and_then(|tx| chain::transfer_fee(tx, &before));
                if let (Some(hash), Some(tx)) = (&hash, &stored) {
                    let inputs = chain::transfer_parts(tx).0;
                    chain_sent_add(hash, json!({ "from": from, "inputs": inputs, "fee": fee.map(|f| f.to_string()), "at": now() }));
                }
                if let (Some(hash), Some(dir)) = (&hash, config_dir()) {
                    activity::add(&dir, now(), json!({ "kind": "send", "chain": "logos", "from": from, "to": to_key, "name": target["name"],
                        "amount": amount.to_string(), "hash": hash, "fee": fee.map(|f| f.to_string()) }));
                }
                plan["hash"] = hash.map_or(Value::Null, Value::from);
                plan["fee"] = fee.map_or(Value::Null, |f| Value::from(f.to_string()));
            }
            Ok(plan)
        })())
    }

    fn chain_split(&mut self, from_key: String, amount: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let from = chain::parse_key(&from_key).ok_or("choose the key to send from")?;
            let amount = chain::whole_amount(&amount)?;
            let notes = chain::notes_of(&node::call("wallet_get_notes", json!([from, ""]), node::QUICK)?);
            let exact = notes.values().any(|value| chain::amount(value) == u128::from(amount));
            let balance = notes.values().map(chain::amount).sum::<u128>();
            // The split pays its own fee, so the key needs more than the amount.
            if balance <= u128::from(amount) {
                return Err(format!("that key holds {balance}; splitting off {amount} needs a little more, for the fee"));
            }
            // Enough, but only with notes a send from here still spends: said as chain_send
            // says it (chain-split-concurrent-says-not-enough-not-in-flight).
            let busy = chain_notes_in_use(&from);
            let free = notes.iter().filter(|(id, _)| !busy.contains(*id)).map(|(_, value)| chain::amount(value)).sum::<u128>();
            if send && free <= u128::from(amount) {
                return Err(IN_FLIGHT.to_owned());
            }
            let mut plan = json!({ "from": from, "amount": amount.to_string(), "balance": balance.to_string(), "exact": exact });
            if !send {
                if exact {
                    plan["fee_about"] = json!("0");
                } else {
                    chain_fee_about(&mut plan, "fee_about", &chain_fee::Shape::Transfer { to: from.clone(), amount }, &from, None)?;
                }
            }
            // A note of exactly the amount is already there: a split would only cost a fee
            // and take notes out of the stake (CR-9).
            if send && exact {
                plan["hash"] = Value::Null;
                plan["fee"] = Value::Null;
                return Ok(plan);
            }
            if send {
                let sent = node::call("wallet_transfer_funds", json!([from, [from], from, amount.to_string(), ""]), node::PROVING)
                    .map_err(|error| in_flight_refusal(error, &from, &notes))?;
                let hash = chain::hash_of(&sent);
                let stored = hash.as_ref().and_then(|hash| node::call("get_transaction", json!([hash]), node::QUICK).ok());
                let fee = stored.as_ref().and_then(|tx| chain::transfer_fee(tx, &notes));
                // Recorded even when the node can't show the transaction yet, so the Bridge
                // screen can follow it: a split is in a block once a note of exactly the
                // amount is in the key's wallet (CR-4).
                if let Some(hash) = &hash {
                    let inputs = stored.as_ref().map(|tx| chain::transfer_parts(tx).0).unwrap_or_default();
                    chain_sent_add(hash, json!({ "from": from, "inputs": inputs, "split": amount.to_string(), "fee": fee.map(|f| f.to_string()), "at": now() }));
                    if let Some(dir) = config_dir() {
                        activity::add(&dir, now(), json!({ "kind": "split", "chain": "logos", "from": from, "amount": amount.to_string(), "hash": hash, "fee": fee.map(|f| f.to_string()) }));
                    }
                }
                plan["hash"] = hash.map_or(Value::Null, Value::from);
                plan["fee"] = fee.map_or(Value::Null, |f| Value::from(f.to_string()));
            }
            Ok(plan)
        })())
    }

    fn chain_bridge(&mut self, from_key: String, to: String, amount: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply((|| {
            let from = chain::parse_key(&from_key).ok_or("choose the key to send from")?;
            let amount = chain::whole_amount(&amount)?;
            let to = match cli::bridge_to(&to)? {
                Some(to) => to,
                None => cli::default_public(&self.cli_setup()?)?,
            };
            let account = registry::parse_account(&to)?;
            // As a transfer refuses them: a deposit to either lands and nobody can use it.
            self.not_a_sink(&account, "take a bridge")?;
            let notes = chain::notes_of(&node::call("wallet_get_notes", json!([from, ""]), node::QUICK)?);
            // A note a send from here still spends (not in a block yet) is not this one's:
            // two deposits of one note both got a hash and only one could land.
            let busy = chain_notes_in_use(&from);
            let note = notes
                .iter()
                .find(|(id, value)| chain::amount(value) == u128::from(amount) && !busy.contains(*id))
                .map(|(id, _)| id.clone());
            let exact_busy = note.is_none() && notes.iter().any(|(id, value)| chain::amount(value) == u128::from(amount) && busy.contains(id));
            // The fee comes from the key's other notes: the exact one goes whole into the deposit.
            let others: u128 = notes
                .iter()
                .filter(|(id, _)| note.as_ref() != Some(*id) && !busy.contains(*id))
                .map(|(_, value)| chain::amount(value))
                .sum();
            // Without an exact note, a split makes it from these same notes first: the fee is
            // then paid from what is left of them, less the split's own fee (needs-split-plan).
            // The split's fee isn't known until it is made (866 and 2,018 seen), so what it
            // may cost is set aside too: a plan said fee_short false with 100,400 left, the
            // split took 866, and the deposit was then refused at 99,534
            // (needs-split-plan-ignores-fee-after-split-regressed).
            let needs_split = note.is_none() && !exact_busy;
            let others = if needs_split { others.saturating_sub(u128::from(amount)).saturating_sub(u128::from(SPLIT_FEE_ALLOWANCE)) } else { others };
            let fee_short = others < u128::from(BRIDGE_MAX_FEE);
            let channel = registry::channel_id(&self.sequencer)?;
            // The deposit is for the zone in use, and a zone is its channel: a sequencer
            // that now names another one isn't that zone's, and a deposit can't be undone.
            let zones = self.zones_now();
            if let (Some(known), Some(read)) = (zones.zone().channel.as_deref(), zones::channel(&channel)) {
                if known != read {
                    return Err(format!(
                        "{}'s sequencer now names another chain (channel {}, not {}). Nothing was sent; check the zone's sequencer.",
                        zones.zone().name,
                        zones::short(&read),
                        zones::short(known)
                    ));
                }
            }
            // A deposit can't be undone: the channel the sequencer names has to be on the
            // chain the node follows (CR-1).
            let channel_known = node::call("get_channel_state", json!([channel]), node::QUICK).is_ok();
            let mut plan = json!({
                "sent": false, "hash": null, "to": to, "amount": amount.to_string(),
                "total": note.as_ref().map(|_| amount.to_string()), "notes": note.as_ref().map(|_| "1"),
                "fee": BRIDGE_MAX_FEE.to_string(), "fee_reserve": BRIDGE_MAX_FEE.to_string(), "note": note, "needs_split": note.is_none(), "channel": channel,
                "channel_known": channel_known, "zone": zones.current_id(), "zone_name": zones.zone().name, "fee_short": fee_short, "others": others.to_string(), "lines": [],
            });
            if needs_split {
                plan["split_fee_allowance"] = json!(SPLIT_FEE_ALLOWANCE.to_string());
            }
            if !send {
                let deposit = chain_fee::Shape::Deposit { channel: channel.clone(), metadata: registry::hex(&account).len() / 2 };
                match &note {
                    Some(note) => chain_fee_about(&mut plan, "fee_about", &deposit, &from, Some(note))?,
                    None => plan["fee_about"] = Value::Null,
                }
                if needs_split {
                    chain_fee_about(&mut plan, "split_fee_about", &chain_fee::Shape::Transfer { to: from.clone(), amount }, &from, None)?;
                }
            }
            if exact_busy {
                plan["needs_split"] = json!(false);
                plan["busy"] = json!(true);
            }
            if send {
                if exact_busy {
                    return Err(format!(
                        "Another transfer from this key is still using its note of exactly {amount}: wait for it to land (a minute or so), then try again. Nothing was sent."
                    ));
                }
                let note = note.ok_or_else(|| format!("no note of that key holds exactly {amount}: split it off first"))?;
                if !channel_known {
                    return Err(format!(
                        "Your node doesn't know LEZ's deposit channel {channel}: it may still be syncing, or your sequencer is on another network. Nothing was sent."
                    ));
                }
                if fee_short {
                    return Err(format!(
                        "The deposit fee (at most {BRIDGE_MAX_FEE}) comes from this key's other notes, which hold {others}. Mine or receive a little more first. Nothing was sent."
                    ));
                }
                // Its arrival on LEZ is told by the receipt LEZ writes when it mints this very
                // deposit, named by the deposit's op id: the channel, the one note and the
                // account, all known now (activity::bridge_updates). No balance of the account
                // is needed, so none is read: a destination LEZ has never seen (`null`) is
                // fine (chain-bridge-null-destination-read-taken-as-zero-strands-bridge).
                let channel_bytes = <[u8; 32]>::try_from(registry::unhex(&channel)?).map_err(|_| "LEZ named a channel that isn't 32 bytes".to_owned())?;
                let op_id = chain::deposit_op_id(&channel_bytes, std::slice::from_ref(&note), &account).ok_or("the note to deposit isn't 32 bytes of hex")?;
                let receipt = registry::account_text(&registry::deposit_receipt(&op_id));
                plan["op_id"] = json!(registry::hex(&op_id));
                plan["receipt"] = json!(receipt);
                // Recorded before the node is asked: a call cut off (the module unloaded while the
                // node proved) still leaves the deposit in the history and its note held, to be
                // settled by the note (deposit-lost-from-activity-on-unload). What the key's notes
                // held is kept with it, so the fee can be told once the deposit is found.
                let pending = format!("pending-{}-{note}", now());
                chain_sent_add(&pending, json!({ "from": from, "inputs": [note], "fee": null, "at": now(), "pending": true, "notes": notes }));
                let entry = config_dir().map(|dir| {
                    let entry = json!({ "kind": "bridge", "chain": "logos", "from": from, "to": format!("Public/{to}"),
                        "amount": amount.to_string(), "hash": null, "note": note, "channel": channel, "op_id": registry::hex(&op_id),
                        "receipt": receipt, "followed": true });
                    let id = activity::add(&dir, now(), activity::in_zone(entry, &zones.current_id()));
                    (dir, id)
                });
                // The deposit's metadata is the LEZ account's 32 bytes, as `names bridge` sends it.
                let deposited = match node::call(
                    "channel_deposit_with_notes",
                    json!([channel, [note], registry::hex(&account), from, [from], BRIDGE_MAX_FEE.to_string(), ""]),
                    node::PROVING,
                ) {
                    Ok(deposited) => deposited,
                    Err(error) => {
                        // Refused by the node: nothing was made. No answer (the module was
                        // unloaded mid-call, the node stopped, the wait ran out): it may have
                        // been, so the record stays for the note to settle.
                        if !error.contains("didn't answer") {
                            chain_sent_remove(&pending);
                            if let Some((dir, id)) = &entry {
                                activity::update(dir, id, &json!({ "state": "failed" }));
                            }
                        }
                        return Err(error);
                    }
                };
                let hash = chain::hash_of(&deposited);
                if let Some(hash) = &hash {
                    // Every note it spends, the fee's too, so it counts as in a block only
                    // once they are all spent (not just once the deposit's note left).
                    let stored = node::call("get_transaction", json!([hash]), node::QUICK).ok();
                    let mut inputs = stored.as_ref().map(chain::all_inputs).unwrap_or_default();
                    if !inputs.contains(&note) {
                        inputs.push(note.clone());
                    }
                    // What it paid, read back from the node as chain_send does (the cap, 100,000, is not it).
                    let fee = stored.as_ref().and_then(|tx| chain::deposit_fee(tx, &notes, u128::from(amount)));
                    plan["fee"] = fee.map_or(Value::Null, |fee| Value::from(fee.to_string()));
                    chain_sent_add(hash, json!({ "from": from, "inputs": inputs, "fee": fee.map(|f| f.to_string()), "at": now() }));
                    if let Some((dir, id)) = &entry {
                        activity::update(dir, id, &json!({ "hash": hash, "fee": plan["fee"] }));
                    }
                }
                // Only now: a call cut off before this still has what the key's notes held,
                // to tell the fee by (deposit_made).
                chain_sent_remove(&pending);
                plan["sent"] = json!(true);
                plan["hash"] = hash.map_or(Value::Null, Value::from);
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
                // A send in the Wallet's history whose record here was lost: the node says
                // what it spends.
                let logged = config_dir().and_then(|dir| activity::all(&dir).into_iter().find(|entry| entry["chain"] == "logos" && entry["hash"] == hash.as_str()));
                if let Some(from) = logged.as_ref().and_then(|entry| entry["from"].as_str()) {
                    let landed = chain_landed_asked(&hash, from)?;
                    return Ok(json!({ "hash": hash, "state": if landed { "in_block" } else { "pending" }, "fee": logged.as_ref().map_or(Value::Null, |entry| entry["fee"].clone()) }));
                }
                // Not sent from here: the node is asked, so a node that is down or has no
                // record of it says so. Even when it has it, which key it spends from (and
                // so whether it is in a block) isn't known here.
                node::call("get_transaction", json!([hash]), node::QUICK)?;
                return Err("That transaction wasn't sent from the Names app, so the app can't tell whether it is in a block.".to_owned());
            };
            let inputs: Vec<String> = known["inputs"].as_array().into_iter().flatten().filter_map(|i| i.as_str().map(str::to_owned)).collect();
            let notes = chain::notes_of(&node::call("wallet_get_notes", json!([known["from"], ""]), node::QUICK)?);
            // A split whose inputs weren't known is in a block once its exact note is there.
            let landed = if inputs.is_empty() {
                let split = known["split"].as_str().and_then(|text| text.parse::<u128>().ok());
                split.is_some_and(|split| notes.values().any(|value| chain::amount(value) == split))
            } else {
                chain::spent(&inputs, &notes)
            };
            Ok(json!({
                "hash": hash,
                "state": if landed { "in_block" } else { "pending" },
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
                let fee_about = chain_claim_fee_about();
                let reward = chain::amount(&vouchers["reward_amount"]);
                let mut plan = json!({
                    "claimable": chain::amount(&vouchers["total_claimable"]).to_string(),
                    "vouchers": count,
                    "reward_per_voucher": reward.to_string(),
                    "fee_about": fee_about,
                });
                if let Some(warning) = chain_fee::claim_warning(plan["fee_about"].as_str().and_then(|fee| fee.parse().ok()).filter(|_| count > 0), reward) {
                    plan["warning"] = Value::from(warning);
                }
                return Ok(plan);
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

    fn chain_account_new(&mut self, label: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(chain_keys::with_node(|ask, files, records| chain_keys::make(ask, files, records, &label, now())))
    }

    fn chain_account_label(&mut self, key: String, label: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(chain_keys::with_node(|ask, _, records| chain_keys::relabel(ask, records, &key, &label)))
    }

    fn chain_account_remove(&mut self, key: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(chain_keys::with_node(|ask, _, records| chain_keys::remove(ask, records, &key)))
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
            // A name is paid on home, where names are kept.
            if !self.on_home() && matches!(cli::pay_to(request["destination"].as_str().unwrap_or_default()), Ok(cli::PayTo::Name(_))) {
                return Err("the wallet is using a zone where a name can't be paid; ask to pay an account ID".to_owned());
            }
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
        // An inscription is paid from a chain account, not one of these: it has its own
        // list and approval (`pending_inscriptions`, `approve_inscription`).
        let pending: Vec<Value> =
            self.store().requests.iter().filter(|request| request["state"] == "pending" && request["kind"] != provider::INSCRIBE).cloned().collect();
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
        let zones = self.zones_now();
        let here = zones.current_id();
        let store = self.store();
        let views: Vec<Value> = pending
            .into_iter()
            .map(|mut request| {
                let app = request["app"].as_str().unwrap_or_default().to_owned();
                let granted = store.granted(&app);
                let kind = request["kind"].as_str().unwrap_or_default().to_owned();
                if kind == provider::TRANSFER || kind == provider::CALL {
                    // Which zone it was asked for, as it is called now, and whether that
                    // is the one in use (it can only be approved there).
                    let asked = request["zone"].as_str().unwrap_or(zones::HOME).to_owned();
                    let name = zones.index(&asked).and_then(|at| zones.at(at)).map(|zone| zone.name.clone());
                    request["zone_in_use"] = json!(asked == here);
                    request["zone_removed"] = json!(name.is_none());
                    request["zone_name"] = json!(name.or_else(|| request["zone_name"].as_str().map(str::to_owned)));
                    request["zone"] = json!(asked);
                }
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

    fn chain_inscribe(&mut self, from_key: String, text: String, send: bool) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(chain_inscribe::run(&from_key, &text, send))
    }

    fn propose_inscription(&mut self, text: String) -> String {
        reply(self.inscription_proposed(&text))
    }

    fn pending_inscriptions(&mut self) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(self.inscriptions_pending())
    }

    fn approve_inscription(&mut self, request_id: String, from_key: String) -> String {
        if let Err(refused) = only_the_names_app() {
            return reply(Err(refused));
        }
        reply(self.inscription_approved(request_id.trim(), &from_key))
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

    fn queue(&mut self, app: &str, mut request: Value, now: u64) -> Result<Value, String> {
        // A transfer or a call is for the zone in use when it is asked, and is approved
        // only there: the same account ID is another account on another zone.
        if request["kind"] == provider::TRANSFER || request["kind"] == provider::CALL {
            let zones = self.zones_now();
            request["zone"] = json!(zones.current_id());
            request["zone_name"] = json!(zones.zone().name);
            request["zone_channel"] = json!(zones.zone().channel);
        }
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
        let read = self.read_in_background(self.zone_key("provider:publics"), std::time::Duration::from_secs(30), move || {
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
        if kind == provider::TRANSFER || kind == provider::CALL {
            // Stays pending: the person can switch zones and approve it, or reject it.
            self.asked_here(&request)?;
        }
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
                    Some((false, provider::clean_text(&cli::failure_cause(log), 300)))
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
            let read = self.read_in_background(self.zone_key("provider:cli_balance"), BALANCE_EVERY, move || cli::run_balance(&setup));
            let balance = read["value"]["balance"].clone();
            merged(read, &json!({ "value": balance }))
        } else {
            let registry = Registry::new(&self.sequencer, self.program.unwrap_or([0; 32]));
            self.read_in_background(self.zone_key(&format!("provider:balance:{}", registry::account_text(&id))), BALANCE_EVERY, move || {
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
            "no token program is set: enter its program ID under Wallet, Add a token (no token program runs on the public testnet yet)".to_owned()
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

/// The most a bridge deposit's fee may be, as `names bridge` caps it by default.
const BRIDGE_MAX_FEE: u64 = 100_000;
/// What a bridge plan that needs a split sets aside for the split's own fee, which the
/// node tells only once the split is made (866, 2,018 and 2,054 seen for one-input transfers).
const SPLIT_FEE_ALLOWANCE: u64 = 10_000;

/// The board answers kept at once (feeds, authors' pages, threads).
const MAX_BOARD_READS: usize = 48;
/// The board's CLI runs at once.
const MAX_BOARD_FETCHES: usize = 3;

/// Whether a `board_reads` key is a board answer (not the wallet's names or tokens).
fn is_board_key(key: &str) -> bool {
    ["feed:", "author:", "thread:"].iter().any(|kind| key.starts_with(kind))
}

/// The most tokens the wallet keeps added by address, per token program.
const MAX_WATCHED_TOKENS: usize = 64;

/// The tokens added by address for `program`: `[{definition, name}]`.
fn tokens_watched(program: &AccountId) -> Vec<Value> {
    let all = config_dir().map(|dir| load_object(&dir.join("tokens-watched.json"))).unwrap_or_default();
    all.get(&registry::account_text(program)).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn save_tokens_watched(program: &AccountId, list: &[Value]) -> Result<(), String> {
    let dir = config_dir().ok_or("there is no config folder to keep the token list in")?;
    let path = dir.join("tokens-watched.json");
    let mut all = load_object(&path);
    all.insert(registry::account_text(program), Value::from(list.to_vec()));
    save_object(&path, &all);
    Ok(())
}

/// Sends made from this device, by hash: `{from, inputs, fee, at}`, for `chain_tx`.
/// Kept on disk, so a send is still followed after the module restarts.
/// The Logos Blockchain sends made here. A file that can't be read is set aside and told,
/// never written over by the next send (chain-sent-corrupt-file-silently-overwritten).
fn chain_sent() -> Map<String, Value> {
    config_dir().map(|dir| activity::load(&dir.join("chain-sent.json"))).unwrap_or_default()
}

/// Whether a send made here (`chain_sent`) is in a block: its inputs are spent, or for a
/// split whose inputs weren't known, its exact note is there.
fn chain_landed(hash: &str) -> Result<bool, String> {
    let Some(known) = chain_sent().remove(hash) else { return Err(NOT_SENT_HERE.to_owned()) };
    let inputs: Vec<String> = known["inputs"].as_array().into_iter().flatten().filter_map(|i| i.as_str().map(str::to_owned)).collect();
    let notes = chain::notes_of(&node::call("wallet_get_notes", json!([known["from"], ""]), node::QUICK)?);
    Ok(if inputs.is_empty() {
        let split = known["split"].as_str().and_then(|text| text.parse::<u128>().ok());
        split.is_some_and(|split| notes.values().any(|value| chain::amount(value) == split))
    } else {
        chain::spent(&inputs, &notes)
    })
}

/// What `chain_landed` says of a hash with no record here.
const NOT_SENT_HERE: &str = "not sent from here";

/// Whether the transaction `hash`, spending from `from`, is in a block, asked of the node
/// when no record of it is kept here: the inputs it stores for it are all spent.
fn chain_landed_asked(hash: &str, from: &str) -> Result<bool, String> {
    let stored = node::call("get_transaction", json!([hash]), node::QUICK)?;
    let inputs = chain::all_inputs(&stored);
    if inputs.is_empty() {
        return Err("the node doesn't show what that transaction spends".to_owned());
    }
    let notes = chain::notes_of(&node::call("wallet_get_notes", json!([from, ""]), node::QUICK)?);
    Ok(chain::spent(&inputs, &notes))
}

/// How long a send made here holds the notes it spends for itself: once in a block they
/// are gone anyway; one that never lands lets them go after this.
const NOTES_HELD_S: u64 = 15 * 60;

/// The notes of `from` that a recent send made here spends (`chain_sent` inputs), which
/// another transfer must not pick while it may still land.
fn chain_notes_in_use(from: &str) -> std::collections::HashSet<String> {
    let at = now();
    chain_sent()
        .values()
        .filter(|sent| sent["from"].as_str() == Some(from) && at.saturating_sub(sent["at"].as_u64().unwrap_or(0)) < NOTES_HELD_S)
        .flat_map(|sent| sent["inputs"].as_array().cloned().unwrap_or_default())
        .filter_map(|input| input.as_str().map(str::to_owned))
        .collect()
}

/// The refusal while a send made here still spends the notes a new one needs.
const IN_FLIGHT: &str = "Another transfer from this key is still waiting to land, and its notes can't be spent twice. Try again in a minute. Nothing was sent.";

/// A node's "not enough" for `from` while a send made here still spends some of its notes
/// (`notes`, the key's now): the key holds enough, its notes are in flight
/// (send-refusal-wording-concurrent: "doesn't hold enough" though it held 100,000).
fn in_flight_refusal(error: String, from: &str, notes: &Map<String, Value>) -> String {
    let short = error == chain::problem("wallet_transfer_funds", "not enough");
    if short && chain_notes_in_use(from).iter().any(|note| notes.contains_key(note)) {
        return IN_FLIGHT.to_owned();
    }
    // Held by the node for an inscription priced here (chain_inscribe.rs), not short.
    if short && chain_inscribe::plan_holds_notes_of(from, notes) {
        return chain_inscribe::HELD.to_owned();
    }
    error
}

/// Most blocks walked back from the node's tip to find a deposit whose call was cut off.
const DEPOSIT_SEARCH_BLOCKS: usize = 600;

/// The deposit `op_id` of `note` from `from` (the history's `entry`), made though its call
/// was cut off: its transaction's hash, found in the recent blocks' events, and the fee it
/// paid, from what the key's notes held when it was sent (kept with its pending record).
/// The hash is then kept like any send made here, so `chain_tx` can be asked of it.
fn deposit_made(op_id: &[u8; 32], from: &str, note: &str, entry: &Value) -> Option<(String, Option<u128>)> {
    let mut block = node::call("get_cryptarchia_info", json!([]), node::QUICK).ok()?["tip"].as_str()?.to_owned();
    let mut found = None;
    for _ in 0..DEPOSIT_SEARCH_BLOCKS {
        let events = node::call("get_block_events", json!([block]), node::QUICK).ok()?;
        if let Some(hash) = chain::deposit_in_events(&events, op_id) {
            found = Some(hash);
            break;
        }
        block = node::call("get_block", json!([block]), node::QUICK).ok()?["header"]["parent_block"].as_str()?.to_owned();
    }
    let hash = found?;
    let sent = chain_sent();
    // Cut off after its record was kept: that has its fee already.
    if let Some(known) = sent.get(&hash) {
        return Some((hash.clone(), known["fee"].as_str().and_then(|fee| fee.parse().ok())));
    }
    let pending = sent.into_iter().find(|(key, sent)| key.starts_with("pending-") && sent["inputs"] == json!([note]) && sent["from"] == from);
    let stored = node::call("get_transaction", json!([hash]), node::QUICK).ok();
    let amount = entry["amount"].as_str().and_then(|amount| amount.parse::<u128>().ok());
    let fee = match (&stored, &pending, amount) {
        (Some(tx), Some((_, sent)), Some(amount)) => sent["notes"].as_object().and_then(|notes| chain::deposit_fee(tx, notes, amount)),
        _ => None,
    };
    let mut inputs = stored.as_ref().map(chain::all_inputs).unwrap_or_default();
    if inputs.is_empty() {
        inputs.push(note.to_owned());
    }
    chain_sent_add(&hash, json!({ "from": from, "inputs": inputs, "fee": fee.map(|f| f.to_string()), "at": entry["at"] }));
    if let Some((key, _)) = pending {
        chain_sent_remove(&key);
    }
    Some((hash, fee))
}

/// Forgets a record `chain_sent_add` made (a deposit's pending one, once it has its hash).
fn chain_sent_remove(key: &str) {
    let Some(dir) = config_dir() else { return };
    let mut all = chain_sent();
    if all.remove(key).is_some() {
        save_object(&dir.join("chain-sent.json"), &all);
    }
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

/// About what `shape` will cost paid from `from`, set in `plan` as `field` (decimal text, or
/// null when the node can't say), with `cannot_pay_fee: true` when the key's notes can't
/// pay it and its fee, and `in_flight: true` when they could but some are a send's from
/// here still on its way: asked of the node with stand-ins of the key's notes, less `skip`
/// and those a send from here still spends, so no real note is reserved (chain_fee.rs).
/// `Err` when the node refuses it as it would refuse the send (a recipient that isn't a key).
fn chain_fee_about(plan: &mut Value, field: &str, shape: &chain_fee::Shape, from: &str, skip: Option<&str>) -> Result<(), String> {
    plan[field] = Value::Null;
    let Ok(notes) = node::call("wallet_get_notes", json!([from, ""]), node::QUICK).map(|reply| chain::notes_of(&reply)) else { return Ok(()) };
    let busy = chain_notes_in_use(from);
    let (held_busy, free): (Vec<_>, Vec<_>) = notes.iter().filter(|(id, _)| skip != Some(id.as_str())).partition(|(id, _)| busy.contains(*id));
    let values = |notes: Vec<(&String, &Value)>| notes.into_iter().map(|(_, value)| chain::amount(value)).collect::<Vec<u128>>();
    match chain_fee::with_busy(shape, from, &values(free), &values(held_busy), |request| node::call("wallet_fund_tx", json!([request]), node::QUICK))? {
        chain_fee::Estimate::Fee(fee) => plan[field] = Value::from(fee.to_string()),
        chain_fee::Estimate::Short => plan["cannot_pay_fee"] = Value::Bool(true),
        chain_fee::Estimate::InFlight => plan["in_flight"] = Value::Bool(true),
        chain_fee::Estimate::Unknown => {}
    }
    Ok(())
}

/// About what one `leader_claim` will cost: its LeaderClaim op and a transfer from the
/// node's leader funding key, which names can't tell from the others, so the most any
/// key would pay with its own notes (chain_fee::claim_estimate).
fn chain_claim_fee_about() -> Value {
    let keys = node::call("wallet_get_known_addresses", json!([]), node::QUICK).unwrap_or_default();
    let held: Vec<(String, Vec<u128>)> = keys
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(MAX_WALLET_ACCOUNTS)
        .filter_map(|key| {
            let notes = node::call("wallet_get_notes", json!([key, ""]), node::QUICK).map(|reply| chain::notes_of(&reply)).ok()?;
            Some((key.to_owned(), notes.values().map(chain::amount).collect()))
        })
        .collect();
    chain_fee::claim_estimate(&held, |request| node::call("wallet_fund_tx", json!([request]), node::QUICK)).map_or(Value::Null, |fee| Value::from(fee.to_string()))
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
            zones: zones::Zones::new(bare_home(DEFAULT_SEQUENCER)),
            channel_asked: false,
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

    /// transfer-case-mangled-id-loses-name: the module handed a name to an ID nobody
    /// holds. A program's account and an unseen public one are refused before any job.
    #[test]
    fn a_hand_over_to_a_program_or_an_unseen_public_account_is_refused() {
        let (mut names, dir) = with_fake_cli("handover", "exit 0");
        let registry_id = registry::account_text(&[7; 32]);
        let refused = parsed(&names.cli_transfer("xferalpha".into(), format!("Public/{registry_id}")));
        assert!(refused["error"].as_str().unwrap().contains("program's account"), "{refused}");
        let unseen = "9Dk3PQD467hXAbsbMzrFYVPu5HFvUqKkU7VdoQD9oirA";
        registry::UNSEEN.with(|list| list.borrow_mut().push(registry::parse_account(unseen).unwrap()));
        let refused = parsed(&names.cli_transfer("xferalpha".into(), format!("Public/{unseen}")));
        assert!(refused["error"].as_str().unwrap().contains("never been used on LEZ"), "{refused}");
        let lower = parsed(&names.cli_transfer("xferalpha".into(), format!("Public/{}", unseen.to_ascii_lowercase())));
        assert!(lower["error"].as_str().unwrap().contains("all one case"), "{lower}");
        assert!(!dir.join("jobs").exists() || std::fs::read_dir(dir.join("jobs")).unwrap().count() == 0, "nothing ran");
        // A private account is never on the chain to be seen: it goes.
        let derived = registry::derive("xferalpha");
        registry::RECORDS.with(|records| records.borrow_mut().push((derived.id, registry::Resolved { owner: [3; 32], expires: registry::NEVER, profile: registry::Profile::default(), claimed: false })));
        let started = parsed(&names.cli_transfer("xferalpha".into(), format!("Private/{unseen}")));
        let id = started["value"]["id"].as_str().expect("a job").to_owned();
        assert_eq!(job_end(&mut names, &id)["state"], "done");
        registry::UNSEEN.with(|list| list.borrow_mut().clear());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// failed-send-becomes-done-after-job-prune: a send that failed (exit 1) was shown
    /// done once its job record was pruned after 7 days.
    #[test]
    fn a_send_whose_job_record_is_gone_is_unknown_not_done() {
        let (mut names, dir) = with_fake_cli("pruned", "exit 1");
        let history = config_dir().unwrap();
        let id = activity::add(&history, now(), json!({ "kind": "pay_account", "chain": "lez", "to": "Public/GAj", "amount": "99999999999999999", "job": "-wallet-17" }));
        let rows = parsed(&names.activity(String::new()));
        let row = rows["value"].as_array().unwrap().iter().find(|e| e["id"] == id.as_str()).cloned().expect("listed");
        assert_eq!(row["state"], "unknown", "{row}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// receive-privately-not-in-activity: a receive_privately job (fee paid 417376)
    /// never appeared in the history.
    #[test]
    fn receiving_privately_is_in_the_history_with_its_fee() {
        let (mut names, dir) = with_fake_cli("privately", "echo 'applied in block 439'; echo 'fee paid: 417376'");
        let started = parsed(&names.cli_receive_privately("XferEcho".into()));
        let job = started["value"]["id"].as_str().expect("a job").to_owned();
        job_end(&mut names, &job);
        let rows = parsed(&names.activity("xferecho".into()));
        let row = rows["value"].as_array().unwrap().iter().find(|e| e["job"] == job.as_str()).cloned().expect("in the history");
        assert_eq!((row["kind"].as_str(), row["state"].as_str(), row["fee"]["paid"].as_str()), (Some("receive_privately"), Some("done"), Some("417376")), "{row}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// bridge-to-saves-before-validating: a cli_bridge_to refused for its 'to' had
    /// already saved its key and node to config.json.
    #[test]
    fn a_refused_bridge_to_remembers_nothing() {
        let (mut names, dir) = with_fake_cli("bridgeto", "exit 0");
        names.bridge_key = Some("ab".repeat(32));
        names.bridge_node = Some("http://127.0.0.1:8080".into());
        let refused = parsed(&names.cli_bridge_to("5".into(), "12".repeat(32), "NOT-AN-ACCOUNT".into(), "http://127.0.0.1:4321".into(), false));
        assert_eq!(refused["error"], "the bridge credits a public LEZ account: Public/ and its ID", "{refused}");
        assert_eq!((names.bridge_key.clone(), names.bridge_node.clone()), (Some("ab".repeat(32)), Some("http://127.0.0.1:8080".to_owned())));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// sub-fee-credits-unclaimable-through-module: cli_receive never passes --all, so
    /// credits worth less than claiming them could not be claimed from the app, and a
    /// receive that claimed nothing was logged as a done receive.
    #[test]
    fn credits_worth_less_than_a_claim_can_be_claimed_and_nothing_claimed_is_told() {
        let (mut names, dir) = with_fake_cli("receiveall", "echo 'left waiting: 3169135 to Private/x: claiming it costs about 5070616, more than it holds'; echo 'nothing worth claiming'");
        let started = parsed(&names.cli_receive());
        let job = started["value"]["id"].as_str().expect("a job").to_owned();
        job_end(&mut names, &job);
        let rows = parsed(&names.activity(String::new()));
        let row = rows["value"].as_array().unwrap().iter().find(|e| e["job"] == job.as_str()).cloned().expect("logged");
        assert_eq!((row["state"].as_str(), row["claimed"].as_bool()), (Some("done"), Some(false)), "{row}");
        let all = parsed(&names.cli_receive_all());
        let job = all["value"]["id"].as_str().expect("a job").to_owned();
        let ended = job_end(&mut names, &job);
        assert!(ended["log_tail"].as_str().unwrap().contains("[receive]\n[--all]"), "{ended}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// partial-claim-job-and-pages-say-nothing-true: the second of two claims was refused by the
    /// wallet; the job blamed the sequencer, reported no fee, showed the credited account as
    /// failed and left the payer's and the waiting account's pages empty.
    #[test]
    fn a_claim_job_that_stopped_partway_is_partly_done_on_every_page() {
        let (p, r1, r2) = (
            "Private/DetM9mjtAHfGp4rqpRNVWBkMjUr2rtEdCUGq3T8rNCdQ",
            "Private/9Dk3Yx4ZQnHxAZ2c4pkuDZ7cHKu8M1NC2vWqzTzS2r8q",
            "Private/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk",
        );
        let log = [
            format!("fee paid from {p}"),
            format!("fee: about 5070616, at most 8239751 (claim 1 of 2, paid from {p} inside the proof)"),
            format!("receiving 1000000 into {r1}; proving it on this computer (several minutes)"),
            format!("received in block 469; {r1} now holds 1000000"),
            "fee paid: 5070616".to_owned(),
            format!("fee: about 5070616, at most 8239751 (claim 2 of 2, paid from {p} inside the proof)"),
            format!("receiving 1000000 into {r2}; proving it on this computer (several minutes)"),
            format!("left waiting: 1000000 to {r2}: not claimed"),
            format!("Error: claimed 1 of 2 (1,000,000 into {r1}). The claim of 1,000,000 into {r2} stopped: the wallet refused this change before sending it: {p} pays its fee and can't hold the fee reserve of 5,070,616 on top of what the change moves. Nothing was sent for it."),
        ];
        let script = format!("case \"$*\" in *--help*) exit 0;; esac\n{}\nexit 1", log.iter().map(|line| format!("echo '{}'", line.replace('\'', "'\\''"))).collect::<Vec<_>>().join("\n"));
        let (mut names, dir) = with_fake_cli("partialclaim", &script);
        let started = parsed(&names.cli_receive_all());
        let job = started["value"]["id"].as_str().expect("a job").to_owned();
        let ended = job_end(&mut names, &job);
        assert_eq!(ended["state"], "failed", "{ended}");
        let problem = ended["problem"].as_str().unwrap();
        assert!(problem.starts_with("Claiming finished only in part: claimed 1 of 2"), "{problem}");
        assert!(!problem.contains("sequencer"), "{problem}");
        assert_eq!(ended["fee"], json!({ "about": "10141232", "at_most": "16479502", "paid": "5070616" }));
        for page in [p, r1, r2] {
            let rows = parsed(&names.activity(page.into()));
            let row = rows["value"].as_array().unwrap().iter().find(|e| e["job"] == job.as_str()).cloned().unwrap_or_else(|| panic!("not on {page}'s page: {rows}"));
            assert_eq!(row["state"], "partial", "{row}");
            assert_eq!((row["from"].as_str(), row["received"][r1].as_str(), row["left"][r2].as_str()), (Some(p), Some("1000000"), Some("1000000")), "{row}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// wallet-job-allowed-while-detached-registration-pending: while `names register --wait`
    /// finished a registration in the background, `cli_status.busy` was null and a wallet job
    /// that also saves the wallet was accepted.
    #[test]
    fn a_registration_finishing_in_the_background_keeps_the_wallet_busy() {
        use std::os::unix::fs::PermissionsExt;
        let (mut names, dir) = with_fake_cli("background", "case \"$*\" in *--help*) exit 0;; esac\nexit 0");
        // A process run as `names register racea --wait`, as the CLI starts one.
        let script = dir.join("bg").join("names");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut finishing = std::process::Command::new(&script).args(["register", "racea", "--wait"]).spawn().unwrap();
        let saved = json!({ "pending": { "racea": { "made": 7 } }, "background": { "racea": finishing.id() } });
        std::fs::write(dir.join("wallet").join("names.json"), saved.to_string()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let status = parsed(&names.cli_status());
        let receive = parsed(&names.cli_receive());
        let register = parsed(&names.cli_register("raceb".into(), false));
        let _ = finishing.kill();
        let _ = finishing.wait();
        // Where ps shows the script by its own name, the registration holds the wallet.
        if status["value"]["busy"]["background"] == true {
            assert_eq!(status["value"]["busy"]["name"], "racea", "{status}");
            for refused in [&receive, &register] {
                assert!(refused["error"].as_str().unwrap().contains("finishing the registration of racea"), "{refused}");
            }
        }
        // Once it has ended, the wallet is free.
        assert_eq!(parsed(&names.cli_status())["value"]["busy"], Value::Null);
        let _ = std::fs::remove_dir_all(dir);
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
        assert_eq!(parsed(&names.cli_mine())["value"], json!([{ "name": "jef", "display": "jef", "signer": "cli", "state": "held by a deposit (no expiry)", "lapsed": false, "registering": false, "failed": false, "private": null, "not_public": null }]));
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

    /// mine-sequencer-down-empties-names: the October CLI printed every name as "no
    /// longer held by this wallet" with the sequencer down (exit 0), and cli_mine answered
    /// success with [].
    #[test]
    fn names_are_not_taken_as_gone_when_the_sequencer_doesnt_answer() {
        let (mut names, dir) = with_fake_cli("mine-down", "echo 'jefdiesel  no longer held by this wallet'; echo 'anon-frost-frog  no longer held by this wallet'");
        // The sequencer says its last block (the old check passed) but fails account reads.
        names.sequencer = registry::stub_rpc(|request| if request["method"] == "getLastBlockId" { registry::rpc_ok(json!(7)) } else { (503, "upstream 503".into()) });
        let down = parsed(&names.cli_mine());
        assert_eq!(down["success"], false, "{down}");
        let error = down["error"].as_str().unwrap();
        assert!(error.contains("didn't give the record of jefdiesel, anon-frost-frog") && error.contains("HTTP 503"), "{down}");
        // With the sequencer answering that neither has a record, they really are gone.
        names.sequencer = registry::stub_rpc(|_| registry::rpc_ok(Value::Null));
        assert_eq!(parsed(&names.cli_mine())["value"], json!([]));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// cli-mine-empty-list-when-account-views-fail / cli-mine-stale-list-unmarked-during-outage:
    /// once a list was read, a refresh that couldn't read the names replaced it with []
    /// (the October CLI calls each "no longer held"), and during an outage the last list
    /// came back as a plain success. Now the last list stays, said to be stale and why.
    #[test]
    fn a_refresh_that_cant_read_the_names_keeps_the_list_and_says_its_stale() {
        let (mut names, dir) = with_fake_cli("mine-stale", "cat \"$(dirname \"$0\")/mine.txt\"");
        std::fs::write(dir.join("mine.txt"), "jefdiesel  held by a deposit (no expiry)\nanon-frost-frog  held by a deposit (no expiry)\n").unwrap();
        let healthy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let answers = healthy.clone();
        names.sequencer = registry::stub_rpc(move |_| {
            if answers.load(std::sync::atomic::Ordering::SeqCst) { registry::rpc_ok(Value::Null) } else { (503, "upstream 503".into()) }
        });
        let first = parsed(&names.cli_mine());
        assert_eq!(first["value"].as_array().map(Vec::len), Some(2), "{first}");
        assert_eq!(first["stale"], Value::Null, "{first}");
        // The outage: the CLI calls both gone, the sequencer gives neither record.
        healthy.store(false, std::sync::atomic::Ordering::SeqCst);
        std::fs::write(dir.join("mine.txt"), "jefdiesel  no longer held by this wallet\nanon-frost-frog  no longer held by this wallet\n").unwrap();
        let mut seen = parsed(&names.cli_mine());
        for _ in 0..40 {
            if seen["stale"] == true {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
            seen = parsed(&names.cli_mine());
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(seen["success"], true, "{seen}");
        assert_eq!(seen["value"].as_array().map(Vec::len), Some(2), "the last list stays: {seen}");
        assert_eq!(seen["stale"], true, "{seen}");
        let warning = seen["warning"].as_str().unwrap();
        assert!(warning.starts_with("Your names as read ") && warning.contains("HTTP 503"), "{warning}");
        assert!(seen["as_of"].as_u64().is_some(), "{seen}");
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
        // SF-2: read differently by the HTTP library, so refused outright.
        assert!(!is_http_url("http://192.168.1.56:3478\\@127.0.0.1"));
        assert!(!is_http_url("http://127.0.0.1@192.168.1.56"));
        assert!(!is_http_url("http://127.0.0.1 .evil.test"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("https://user@testnet.lez.logos.co"));
        assert!(is_http_url("http://[::1]:3040/"));
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

    /// The node answers `method` with a refusal.
    fn node_refuses(method: &str, error: &str) {
        node::ANSWERS.with(|answers| answers.borrow_mut().insert(method.to_owned(), json!({ "success": false, "value": null, "error": error })));
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
    fn a_split_is_a_send_to_the_same_key_and_a_bridge_deposits_one_exact_note() {
        let mut names = fresh();
        const E8: &str = "e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8";
        let asked_args = |method: &str| node::ASKED.with(|asked| asked.borrow().iter().rev().find(|(m, _)| m == method).map(|(_, a)| a.clone()));
        // One big note: a split is needed, and the bridge says so instead of moving it all.
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "b271", "value": "131296018" }] }));
        let split = parsed(&names.chain_split(NODE_KEY.into(), "1,000,000".into(), false));
        assert_eq!(split["value"], json!({ "from": NODE_KEY, "amount": "1000000", "balance": "131296018", "exact": false, "fee_about": null }), "{split}");
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let plan = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), false));
        assert_eq!((plan["value"]["needs_split"].as_bool(), plan["value"]["note"].clone()), (Some(true), Value::Null), "{plan}");
        let refused = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), true));
        assert!(refused["error"].as_str().unwrap().contains("split it off first"), "{refused}");
        assert!(!node_asked().contains(&"channel_deposit_with_notes".to_owned()), "nothing deposited without an exact note");
        // The split: from the key to itself, the change back to it.
        node_answers("wallet_transfer_funds", json!("cd".repeat(32)));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["b271"], "outputs": [{ "value": 1000000 }, { "value": 130294000 }] } }] } }));
        let made = parsed(&names.chain_split(NODE_KEY.into(), "1000000".into(), true));
        assert_eq!(made["value"]["hash"], "cd".repeat(32), "{made}");
        assert_eq!(made["value"]["fee"], "2018");
        assert_eq!(asked_args("wallet_transfer_funds"), Some(json!([NODE_KEY, [NODE_KEY], NODE_KEY, "1000000", ""])));
        // Now a note of exactly the amount: the bridge deposits just it, to the picked account.
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "c0de", "value": "130294000" }, { "id": "e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8", "value": "1000000" }] }));
        let ready = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), false));
        assert_eq!((ready["value"]["note"].as_str(), ready["value"]["needs_split"].as_bool(), ready["value"]["total"].as_str()), (Some("e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8"), Some(false), Some("1000000")), "{ready}");
        node_answers("channel_deposit_with_notes", json!("ab".repeat(32)));
        // The node doesn't know the channel: nothing is deposited.
        let unknown = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), true));
        assert!(unknown["error"].as_str().unwrap().contains("deposit channel"), "{unknown}");
        assert!(!node_asked().contains(&"channel_deposit_with_notes".to_owned()), "nothing deposited into an unknown channel");
        node_answers("get_channel_state", json!({ "accredited_keys": [], "configuration_threshold": 3 }));
        // chain-bridge-null-destination-read-taken-as-zero-strands-bridge: what the account
        // holds isn't asked (LEZ hasn't seen it here: no balance known); its arrival is told
        // by the deposit's receipt.
        // The deposit as the node stores it: the exact note, and a note for its fee.
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [
            { "opcode": 5, "payload": { "inputs": ["e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8"] } },
            { "opcode": 0, "payload": { "inputs": ["c0de"], "outputs": [{ "value": 130292386 }] } }] } }));
        let sent = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), true));
        assert_eq!((sent["value"]["sent"].as_bool(), sent["value"]["hash"].as_str()), (Some(true), Some("ab".repeat(32).as_str())), "{sent}");
        assert_eq!(sent["value"]["channel_known"], true);
        // bridge-fee-reported-as-max: the fee paid, read back (130294000 + 1000000 in, 130292386 back, 1000000 deposited).
        assert_eq!((sent["value"]["fee"].as_str(), sent["value"]["fee_reserve"].as_str()), (Some("1614"), Some("100000")), "{sent}");
        assert_eq!(parsed(&names.chain_tx("ab".repeat(32)))["value"]["fee"], "1614");
        // double-bridge-false-in-block: a second deposit of that note before the first is
        // in a block got a hash too and could never land. Now it is refused, and the plan says why.
        let again = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), true));
        assert!(again["error"].as_str().unwrap().contains("still using its note"), "{again}");
        let plan = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), false));
        assert_eq!((plan["value"]["busy"].as_bool(), plan["value"]["needs_split"].as_bool()), (Some(true), Some(false)), "{plan}");
        // In a block only once every note it spends is gone, the fee's too.
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "c0de", "value": "130294000" }] }));
        assert_eq!(parsed(&names.chain_tx("ab".repeat(32)))["value"]["state"], "pending");
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "f00d", "value": "130292386" }] }));
        assert_eq!(parsed(&names.chain_tx("ab".repeat(32)))["value"]["state"], "in_block");
        // The split and the bridge are in the Wallet's history, the bridge with where it lands.
        let history = activity::all(&config_dir().unwrap());
        let bridge = history.iter().find(|e| e["kind"] == "bridge" && e["hash"] == "ab".repeat(32)).expect("the bridge is in the history");
        // Followed by the receipt LEZ writes when it mints this deposit (op id of channel, note, account).
        let op = chain::deposit_op_id(&[2; 32], &[E8.to_owned()], &registry::parse_account(to).unwrap()).unwrap();
        assert_eq!((bridge["op_id"].as_str(), bridge["receipt"].as_str(), bridge["followed"].as_bool()),
            (Some(registry::hex(&op).as_str()), Some(registry::account_text(&registry::deposit_receipt(&op)).as_str()), Some(true)), "{bridge}");
        assert_eq!(sent["value"]["receipt"], bridge["receipt"], "the reply names it too");
        assert!(bridge.get("to_before").is_none(), "no balance read: {bridge}");
        assert_eq!(bridge["fee"], "1614", "the paid fee is in the history");
        assert_eq!((bridge["amount"].as_str(), bridge["to"].as_str(), bridge["state"].as_str()), (Some("1000000"), Some(format!("Public/{to}").as_str()), Some("sending")));
        assert!(history.iter().any(|e| e["kind"] == "split" && e["hash"] == "cd".repeat(32)), "the split is in the history");
        let account = registry::parse_account(to).unwrap();
        assert_eq!(
            asked_args("channel_deposit_with_notes"),
            Some(json!(["02".repeat(32), ["e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8e8"], registry::hex(&account), NODE_KEY, [NODE_KEY], "100000", ""])),
            "one exact note, the account's 32 bytes as metadata, the fee from the key's other notes"
        );
        // A private account is never a bridge's destination.
        assert!(parsed(&names.chain_bridge(NODE_KEY.into(), format!("Private/{to}"), "1000000".into(), false))["error"].as_str().unwrap().contains("public LEZ account"));
        // Nor one with a second prefix inside (account-prefix-double-strip: sent to the node as Public/<id>).
        assert!(parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/Private/{to}"), "1000000".into(), false))["error"].as_str().unwrap().contains("Public/ once"));
    }


    // ── From the code-review campaign of 2026-11-06 (CR-1..CR-9): each failed before its fix ──
    const REVIEW_OTHER_PRIVATE: &str = "52QsitjhXQF2KpNBTBqxLSz9XR6ofZVAsdwprxx9HHNR";

    /// RV1: after "Pay from this account" (cli_account_use) the balance is the NEW default's.
    #[test]
    fn review_balance_follows_the_new_default_after_account_use() {
        let other = REVIEW_OTHER_PRIVATE;
        let entry = format!(r#"{{"account":"Private/{other}","kind":"private","label":null,"balance":"999","default":true,"public_key":null}}"#);
        let tail = format!(
            "S=\"$(dirname \"$0\")/state\"\ncase \"$*\" in\n*\"module account-use\"*) echo B > \"$S\"; echo '{entry}' ;;\n*balance*) if [ \"$(cat \"$S\" 2>/dev/null)\" = B ]; then echo '  Private/{other}  999  (default)'; else echo '  Private/{PRIVATE}  250  (default)'; fi ;;\nesac"
        );
        let (mut names, dir) = with_wallet("review-use", &tail);
        let before = parsed(&names.cli_balance());
        assert_eq!(before["value"]["account"], PRIVATE, "{before}");
        let used = parsed(&names.cli_account_use(format!("Private/{other}")));
        assert_eq!(used["success"], true, "{used}");
        let after = parsed(&names.cli_balance());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(after["value"]["account"], other, "cli_balance still reports the OLD default after cli_account_use: {after}");
    }

    /// october-cli-balance-zero-when-sequencer-down: the October CLI printed every public
    /// account as 0 with the sequencer down (exit 0), and cli_balance answered success
    /// with each public balance "0".
    #[test]
    fn a_public_zero_is_not_believed_from_a_sequencer_that_doesnt_answer() {
        let tail = format!(
            "case \"$*\" in\n*balance*) echo '  Public/{PUBLIC}  0  (default)'; echo '  Public/{OTHER}  0'; echo '  Private/{PRIVATE}  250  (default)' ;;\nesac"
        );
        let (mut names, dir) = with_wallet("balance-down", &tail);
        names.sequencer = "http://127.0.0.1:9/".into();
        let down = parsed(&names.cli_balance());
        assert_eq!(down["success"], false, "{down}");
        assert!(down["error"].as_str().unwrap().contains("could not reach the sequencer"), "{down}");
        // Answering, a 0 is a 0.
        names.sequencer = registry::stub_rpc(|_| registry::rpc_ok(registry::view_holding(0)));
        let up = parsed(&names.cli_balance());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(up["value"]["public"]["balance"], "0", "{up}");
        assert_eq!(up["partial"], Value::Null, "{up}");
    }

    /// The October CLI's balance read with some account reads failing (a proxy answering
    /// 503 to getAccountBalance, or one read in three), and getLastBlockId answering.
    fn october_balance(label: &str, failing: &'static [&'static str]) -> (Names, std::path::PathBuf) {
        let tail = format!(
            "case \"$*\" in\n*balance*) echo 'public accounts (seen on chain):'; echo '  Public/{PUBLIC}  0  (default)'; echo '  Public/{OTHER}  0'; echo '  Private/{PRIVATE}  250  (default)'; echo 'private total: 250' ;;\nesac"
        );
        let (mut names, dir) = with_wallet(label, &tail);
        names.sequencer = registry::stub_rpc(move |request| {
            let account = registry::asked_account(request);
            if failing.contains(&account.as_str()) {
                (503, "upstream 503".into())
            } else if account == PUBLIC {
                registry::rpc_ok(registry::view_holding(39_733_291_875))
            } else {
                registry::rpc_ok(registry::view_holding(10_000_001))
            }
        });
        (names, dir)
    }

    /// selective-degradation-cold-read-false-arrivals / balance-accounts-zeroed-with-success-on-partial-failure:
    /// with getAccountBalance failing and getLastBlockId answering, the October CLI printed
    /// the main public account (39,733,291,875) and 9W9h.. (10,000,001) as 0 and exited 0;
    /// cli_balance said success with those 0s, and on a first read (no marks yet) the 0s
    /// became marks, so the next good read logged both as money that arrived. Now each 0 is
    /// read again: what the sequencer gives replaces it, what it doesn't is unknown and
    /// named, and a read with an unknown balance moves no mark.
    #[test]
    fn an_unread_balance_is_never_a_zero_and_moves_no_mark() {
        let (mut names, dir) = october_balance("balance-partial", &[PUBLIC]);
        let marks = config_dir().unwrap().join("activity-balances.json");
        let _ = std::fs::remove_file(&marks);
        let partial = parsed(&names.cli_balance());
        assert_eq!(partial["success"], true, "{partial}");
        assert_eq!(partial["partial"], true, "{partial}");
        assert_eq!(partial["unread"], json!([format!("Public/{PUBLIC}")]), "{partial}");
        assert_eq!(partial["value"]["public"]["balance"], Value::Null, "the main account is unknown, not 0: {partial}");
        let other = partial["value"]["accounts"].as_array().unwrap().iter().find(|row| row["account"] == format!("Public/{OTHER}")).cloned();
        assert_eq!(other.unwrap()["balance"], "10000001", "the CLI's 0 is replaced by what the sequencer says: {partial}");
        let warning = partial["warning"].as_str().unwrap();
        assert!(warning.contains("Public/CCVd…pKnk") && warning.contains("HTTP 503") && warning.contains("unknown"), "{warning}");
        assert!(!marks.exists(), "a partial read wrote balance marks");
        // The sequencer is back: the first whole read sets the marks, and nothing arrived.
        cli::forget_balance();
        let (back, back_dir) = october_balance("balance-partial-back", &[]);
        names.sequencer = back.sequencer.clone();
        let _ = std::fs::remove_dir_all(back_dir);
        let whole = parsed(&names.cli_balance());
        assert_eq!(whole["value"]["public"]["balance"], "39733291875", "{whole}");
        assert_eq!(whole["partial"], Value::Null, "{whole}");
        let history = activity::all(&config_dir().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(marks.exists(), "a whole read sets the marks");
        assert!(!history.iter().any(|entry| entry["kind"] == "arrived"), "no money arrived: {history:?}");
    }

    /// Every public balance unread is no reading: the call fails and says why.
    #[test]
    fn a_balance_read_with_no_public_balance_is_a_failure() {
        let (mut names, dir) = october_balance("balance-none", &[PUBLIC, OTHER]);
        let failed = parsed(&names.cli_balance());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(failed["success"], false, "{failed}");
        assert!(failed["error"].as_str().unwrap().contains("HTTP 503"), "{failed}");
    }

    /// The 0.4 CLI says `unknown` for a balance it couldn't read and exits 1: the rest of
    /// its read still answers, the unknown named.
    #[test]
    fn a_cli_that_says_unknown_gives_a_partial_read() {
        let tail = format!(
            "case \"$*\" in\n*balance*) echo '  Public/{PUBLIC}  unknown  (default)'; echo '  Public/{OTHER}  7'; echo '  Private/{PRIVATE}  250  (default)'; echo 'Error: the sequencer didn'\"'\"'t answer, so 1 public balance(s) are unknown; try again when it is reachable' >&2; exit 1 ;;\nesac"
        );
        let (mut names, dir) = with_wallet("balance-unknown", &tail);
        let partial = parsed(&names.cli_balance());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(partial["success"], true, "{partial}");
        assert_eq!(partial["unread"], json!([format!("Public/{PUBLIC}")]), "{partial}");
        assert_eq!(partial["value"]["balance"], "250", "{partial}");
    }

    /// balance-accounts-zeroed-with-success-on-partial-failure, for cli_accounts: an account
    /// the CLI couldn't read (null) is named beside the list, never a plain success.
    #[test]
    fn accounts_the_sequencer_didnt_give_are_named() {
        let list = format!(
            r#"[{{"account":"Public/{PUBLIC}","kind":"public","label":null,"balance":null,"default":true,"public_key":null,"npk":null}},{{"account":"Public/{OTHER}","kind":"public","label":null,"balance":"5","default":false,"public_key":null,"npk":null}}]"#
        );
        let (mut names, dir) = with_wallet("accounts-partial", &format!("case \"$*\" in\n*account-list*) echo '{list}' ;;\nesac"));
        let read = parsed(&names.cli_accounts());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read["success"], true, "{read}");
        assert_eq!(read["partial"], true, "{read}");
        assert_eq!(read["unread"], json!([format!("Public/{PUBLIC}")]), "{read}");
        assert_eq!(read["value"][0]["balance"], Value::Null, "{read}");
    }

    /// still-reading-message-wrong-for-a-hung-sequencer: with the sequencer accepting and
    /// never answering, a read past the call's wait was "still catching up with the chain;
    /// this answers in a moment", which never came. It now says the sequencer doesn't
    /// answer, and the read is stopped rather than left hanging.
    #[test]
    fn a_read_held_by_a_sequencer_that_doesnt_answer_says_so_and_stops() {
        let tail = format!("case \"$*\" in\n*balance*) sleep 8; touch \"$(dirname \"$0\")/finished\"; echo '  Private/{PRIVATE}  250  (default)' ;;\nesac");
        let (mut names, dir) = with_wallet("balance-hung", &tail);
        registry::SEQUENCER_DOWN.with(|down| down.set(true));
        let started = std::time::Instant::now();
        let first = parsed(&names.cli_balance());
        registry::SEQUENCER_DOWN.with(|down| down.set(false));
        assert!(started.elapsed() < std::time::Duration::from_secs(7), "{:?}", started.elapsed());
        assert_eq!(first["success"], false, "{first}");
        let error = first["error"].as_str().unwrap();
        assert!(error.contains("could not reach the sequencer") && !error.contains("catching up"), "{error}");
        std::thread::sleep(std::time::Duration::from_secs(5));
        let finished = dir.join("finished").exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!finished, "the read was left running");
    }

    /// slow-read-error-survives-recovery: a read that failed while the sequencer was slow
    /// was kept and given, in 0.2 s, to the first call after it came back. A failed read is
    /// never kept: the next call reads afresh.
    #[test]
    fn a_failed_slow_read_is_not_the_next_calls_answer() {
        let tail = format!(
            "case \"$*\" in\n*balance*) if [ -e \"$(dirname \"$0\")/slow\" ]; then sleep 6; echo 'Error: the names CLI took more than 90 s' >&2; exit 1; fi; echo '  Private/{PRIVATE}  250  (default)' ;;\nesac"
        );
        let (mut names, dir) = with_wallet("balance-recover", &tail);
        std::fs::write(dir.join("slow"), "").unwrap();
        let first = parsed(&names.cli_balance());
        assert_eq!(first["error"], cli::STILL_READING, "{first}");
        // The read fails meanwhile; the sequencer is back.
        std::thread::sleep(std::time::Duration::from_secs(3));
        std::fs::remove_file(dir.join("slow")).unwrap();
        let second = parsed(&names.cli_balance());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(second["success"], true, "the old failure was served: {second}");
        assert_eq!(second["value"]["balance"], "250", "{second}");
    }

    /// receive-privately-during-outage-leaves-orphan-private-accounts: with the sequencer
    /// down, each cli_receive_privately started a job whose CLI made a new private account
    /// before it failed. A job is now refused before anything starts.
    #[test]
    fn no_job_starts_while_the_sequencer_doesnt_answer() {
        let (mut names, dir) = with_wallet("job-down", &format!("touch \"$(dirname \"$0\")/ran\""));
        registry::SEQUENCER_DOWN.with(|down| down.set(true));
        let refused = parsed(&names.cli_receive_privately("jefdiesel".into()));
        let paid = parsed(&names.cli_set_text("jefdiesel".into(), "chat".into(), "x".into()));
        registry::SEQUENCER_DOWN.with(|down| down.set(false));
        let ran = dir.join("ran").exists();
        let jobs = std::fs::read_dir(dir.join("jobs")).map_or(0, |entries| entries.count());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(refused["success"], false, "{refused}");
        let error = refused["error"].as_str().unwrap();
        assert!(error.contains("could not reach the sequencer") && error.contains("Nothing was started"), "{error}");
        assert_eq!(paid["success"], false, "{paid}");
        assert!(!ran, "the CLI ran");
        assert_eq!(jobs, 0, "a job was made");
    }

    /// resolve-slow-sequencer-exceeds-host-timeout-no-result: a resolve on a sequencer that
    /// takes long answers inside the call, with a result field, saying it was slow.
    #[test]
    fn a_resolve_on_a_slow_sequencer_answers_within_the_call() {
        let mut names = fresh();
        names.program = Some([7; 32]);
        names.sequencer = registry::stub_rpc(|_| {
            std::thread::sleep(std::time::Duration::from_secs(30));
            registry::rpc_ok(Value::Null)
        });
        let started = std::time::Instant::now();
        let slow = parsed(&names.resolve("slowpoke".into()));
        assert!(started.elapsed() < registry::READ_BUDGET + std::time::Duration::from_secs(1), "{:?}", started.elapsed());
        assert_eq!(slow["success"], false, "{slow}");
        assert!(slow["error"].as_str().unwrap().contains("didn't answer within"), "{slow}");
    }

    /// first-call-no-result-field-slow-cli-exceeds-host-timeout: on a wallet behind the chain
    /// the first cli_accounts / cli_balance ran the CLI past the host's 20 s call limit, so
    /// the caller got METHOD_FAILED with no result. A call now waits a bounded time and is
    /// told the read goes on; the next call takes its result.
    #[test]
    fn a_slow_balance_read_answers_within_the_call_and_the_next_call_takes_it() {
        let tail = format!("case \"$*\" in\n*balance*) sleep 7; echo '  Private/{PRIVATE}  250  (default)' ;;\nesac");
        let (mut names, dir) = with_wallet("balance-slow", &tail);
        let started = std::time::Instant::now();
        let first = parsed(&names.cli_balance());
        assert!(started.elapsed() < std::time::Duration::from_secs(6), "answered within the call: {:?}", started.elapsed());
        assert_eq!(first["error"], cli::STILL_READING, "{first}");
        let second = parsed(&names.cli_balance());
        assert!(started.elapsed() < std::time::Duration::from_secs(14), "one CLI run, not two: {:?}", started.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(second["value"]["balance"], "250", "{second}");
    }

    /// RV2: after set_sequencer the balance is read from the new network, not the last one's output.
    #[test]
    fn review_balance_is_read_again_after_the_sequencer_changes() {
        let tail = format!(
            "case \"$*\" in\n*127.0.0.1:9*balance*) echo '  Private/{PRIVATE}  250  (default)' ;;\n*balance*) echo '  Private/{PRIVATE}  5  (default)' ;;\nesac"
        );
        let (mut names, dir) = with_wallet("review-seq", &tail);
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "250");
        assert!(names.set_sequencer("https://other-network.example".into()));
        let after = parsed(&names.cli_balance());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(after["value"]["balance"], "5", "the old network's balance is served after set_sequencer: {after}");
    }

    /// RV3: a split the node took is followable by chain_tx even when get_transaction
    /// didn't answer right after it (the Bridge screen polls chain_tx until in_block, forever).
    #[test]
    fn review_a_split_can_be_followed_when_the_node_cant_show_it_yet() {
        let mut names = fresh();
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "b271", "value": "131296018" }] }));
        node_answers("wallet_transfer_funds", json!("ce".repeat(32)));
        // get_transaction is not answered (node busy / not stored yet).
        let made = parsed(&names.chain_split(NODE_KEY.into(), "1000000".into(), true));
        assert_eq!(made["value"]["hash"], "ce".repeat(32), "{made}");
        let followed = parsed(&names.chain_tx("ce".repeat(32)));
        assert_eq!(followed["success"], true, "chain_tx can never report this split, so bridgeSplitHash never clears: {followed}");
    }

    /// RV4: no split (a fee, notes out of the stake) when a note of exactly the amount is already there.
    #[test]
    fn review_no_split_is_sent_when_an_exact_note_exists() {
        let mut names = fresh();
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "e8ac", "value": "1000000" }, { "id": "c0de", "value": "5000000" }] }));
        node_answers("wallet_transfer_funds", json!("cf".repeat(32)));
        let made = parsed(&names.chain_split(NODE_KEY.into(), "1000000".into(), true));
        assert!(!node_asked().contains(&"wallet_transfer_funds".to_owned()), "a split was sent though note e8ac already holds exactly 1000000: {made}");
    }

    /// needs-split-plan-ignores-fee-after-split: KB held 150100; the plan for 100,000 said
    /// needs_split, fee_short false; after paying for the split the bridge was refused as fee_short.
    #[test]
    fn a_plan_that_needs_a_split_counts_the_fee_from_what_the_split_leaves() {
        let mut names = fresh();
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "k150", "value": "150100" }] }));
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let plan = parsed(&names.chain_bridge("39".repeat(32), format!("Public/{to}"), "100,000".into(), false));
        assert_eq!((plan["value"]["needs_split"].as_bool(), plan["value"]["fee_short"].as_bool(), plan["value"]["others"].as_str()), (Some(true), Some(true), Some("40100")), "{plan}");
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "k250", "value": "250100" }] }));
        let plan = parsed(&names.chain_bridge("39".repeat(32), format!("Public/{to}"), "100,000".into(), false));
        assert_eq!((plan["value"]["needs_split"].as_bool(), plan["value"]["fee_short"].as_bool()), (Some(true), Some(false)), "{plan}");
    }

    /// needs-split-plan-ignores-fee-after-split-regressed (e1): KA held 700,000 in two notes;
    /// the plan for 599,600 said needs_split, fee_short false, others 100,400; the split took
    /// 866 and the bridge was then refused, the other notes holding 99,534.
    #[test]
    fn a_plan_that_needs_a_split_sets_aside_the_splits_own_fee() {
        let mut names = fresh();
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "a1", "value": "400000" }, { "id": "a2", "value": "300000" }] }));
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let plan = parsed(&names.chain_bridge("39".repeat(32), format!("Public/{to}"), "599600".into(), false));
        assert_eq!((plan["value"]["needs_split"].as_bool(), plan["value"]["fee_short"].as_bool()), (Some(true), Some(true)), "{plan}");
        assert_eq!((plan["value"]["others"].as_str(), plan["value"]["split_fee_allowance"].as_str()), (Some("90400"), Some("10000")), "{plan}");
        // Past the reserve and the split's fee: ready.
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "a1", "value": "410000" }, { "id": "a2", "value": "300000" }] }));
        let plan = parsed(&names.chain_bridge("39".repeat(32), format!("Public/{to}"), "599600".into(), false));
        assert_eq!((plan["value"]["fee_short"].as_bool(), plan["value"]["others"].as_str()), (Some(false), Some("100400")), "{plan}");
    }

    /// corrupt-balances-file-fails-whole-activity-call (G4): only the marks file damaged,
    /// and activity() answered success:false "a new history started".
    #[test]
    fn activity_lists_the_history_with_a_warning_when_only_the_marks_are_damaged() {
        let mut names = fresh();
        let dir = config_dir().unwrap();
        activity::add(&dir, now(), json!({ "kind": "arrived", "chain": "lez", "to": "Public/F", "amount": "12345", "state": "arrived" }));
        std::fs::write(dir.join("activity-balances.json"), "{\"lez@x:Public/F\":\"5").unwrap();
        activity::note_balances(&dir, now(), "lez", "x", &[("Public/F".to_owned(), 9)]);
        let shown = parsed(&names.activity(String::new()));
        assert_eq!(shown["success"], true, "{shown}");
        assert_eq!(shown["value"][0]["amount"], "12345", "{shown}");
        assert!(shown["warning"].as_str().unwrap().contains("activity-balances.json.corrupt-"), "{shown}");
        assert!(parsed(&names.activity(String::new()))["warning"].is_null(), "told once");
    }

    /// cli-set-page-forwards-control-chars: a text/html page with NUL, ESC and BEL in it
    /// was passed to the CLI as a job, while cli_set_text refused control characters.
    #[test]
    fn a_text_page_with_control_characters_is_refused_before_any_job() {
        let (mut names, dir) = with_fake_cli("page-control", "exit 0");
        for body in ["<p>x\u{0}y\u{1b}[31mz\u{7}</p>", "a\u{85}b"] {
            for content_type in ["text/html", "text/plain; charset=utf-8", ""] {
                let refused = parsed(&names.cli_set_page("alice".into(), content_type.into(), body.into()));
                assert!(refused["error"].as_str().unwrap_or_default().contains("control characters"), "{content_type}: {refused}");
            }
        }
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "no job");
        // Line breaks of either kind and tabs are text; a binary type is left as it is.
        let text = parsed(&names.cli_set_page("alice".into(), "text/html".into(), "<p>a</p>\r\n\t<p>b</p>\n".into()));
        assert_eq!(text["success"], true, "{text}");
        job_end(&mut names, text["value"]["id"].as_str().unwrap());
        let png = parsed(&names.cli_set_page("alice".into(), "image/png".into(), "\u{0}\u{1}PNG".into()));
        assert_eq!(png["success"], true, "{png}");
        job_end(&mut names, png["value"]["id"].as_str().unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// module-self-transfer-starts-job-logs-failed: cli_transfer to the account that
    /// already held the name started a job, the CLI refused it ("... already holds ovone,
    /// so nothing was sent"), and a failed transfer row was left in the history.
    #[test]
    fn handing_a_name_to_its_holder_is_refused_before_any_job() {
        let (mut names, dir) = with_wallet("self-transfer", "exit 0");
        let derived = registry::derive("ovone");
        let holder = registry::parse_account(PRIVATE).unwrap();
        registry::RECORDS.with(|records| records.borrow_mut().push((derived.id, registry::Resolved { owner: registry::owner_tag(&derived.id, &holder), expires: registry::NEVER, profile: registry::Profile::default(), claimed: false })));
        let refused = parsed(&names.cli_transfer("ovone".into(), format!("Private/{PRIVATE}")));
        assert_eq!(refused["error"], format!("{PRIVATE} already holds ovone, so nothing was sent"), "{refused}");
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "no job");
        assert!(!activity::all(&config_dir().unwrap()).iter().any(|e| e["kind"] == "transfer"), "no history row");
        // To another account it goes ahead.
        assert_eq!(parsed(&names.cli_transfer("ovone".into(), format!("Public/{PUBLIC}")))["success"], true);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// chain-split-concurrent-says-not-enough-not-in-flight: chain_split while the key's notes
    /// were in flight said "That key doesn't hold enough for this and its fee."
    #[test]
    fn a_split_refused_while_a_send_still_spends_the_keys_notes_says_so() {
        let mut names = fresh();
        let key = "e4".repeat(32);
        node_answers("wallet_get_balance", json!("400000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "a1", "value": "100000" }, { "id": "a2", "value": "300000" }] }));
        node_answers("wallet_transfer_funds", json!("d2".repeat(32)));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["a2"], "outputs": [{ "value": 250000 }, { "value": 48000 }] } }] } }));
        assert_eq!(parsed(&names.chain_send(key.clone(), "39".repeat(32), "250000".into(), true))["success"], true);
        // a2 is in flight: the key holds 400,000, but only a1 (100,000) is free.
        let split = parsed(&names.chain_split(key.clone(), "150000".into(), true));
        assert_eq!(split["error"], IN_FLIGHT, "{split}");
        // The node says it hasn't enough while one of the notes is in flight: the same words.
        node_refuses("wallet_transfer_funds", "Wallet does not have enough funds, available=0");
        let split = parsed(&names.chain_split(key.clone(), "50000".into(), true));
        assert_eq!(split["error"], IN_FLIGHT, "{split}");
        // A plan (no send) still answers with what the key holds.
        assert_eq!(parsed(&names.chain_split(key, "150000".into(), false))["success"], true);
    }

    /// cli-transfer-unregistered-name-starts-failing-job: a name nobody registered started
    /// a transfer job (which failed "not registered") and left a transfer row in the history.
    #[test]
    fn handing_over_a_name_nobody_registered_is_refused_before_any_job() {
        let (mut names, dir) = with_wallet("unregistered-transfer", "exit 0");
        // The registry holds no record for it: the sequencer has no state on its account.
        names.sequencer = registry::stub_http(registry::json_reply(r#"{"jsonrpc":"2.0","id":1,"result":null}"#), 8);
        let refused = parsed(&names.cli_transfer("nobodyhere".into(), format!("Private/{PRIVATE}")));
        assert_eq!(refused["error"], "nobodyhere is not registered, so nothing was sent", "{refused}");
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "no job");
        assert!(!activity::all(&config_dir().unwrap()).iter().any(|e| e["kind"] == "transfer" && e["name"] == "nobodyhere"), "no history row");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// pay-account-accepts-program-account: cli_pay_account paid the registry's account
    /// (5,000 -> 6,000) while cli_bridge_to and cli_transfer refused it.
    #[test]
    fn a_payment_to_a_programs_account_or_the_empty_one_is_refused() {
        let (mut names, dir) = with_wallet("pay-program", "exit 0");
        names.token_program = Some([5; 32]);
        for to in [registry::account_text(&[7; 32]), registry::account_text(&[5; 32]), DEFAULT_REGISTRY.to_owned(), RETIRED_REGISTRIES[1].to_owned()] {
            let refused = parsed(&names.cli_pay_account(format!("Public/{to}"), "1000".into(), false));
            assert_eq!(refused["error"], "that is a program's account, which can't take a payment", "{to}: {refused}");
        }
        let empty = parsed(&names.cli_pay_account(format!("Public/{}", registry::account_text(&[0; 32])), "1000".into(), false));
        assert_eq!(empty["error"], "that is the empty account (all 1s), which no one holds", "{empty}");
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "nothing ran");
        assert!(!activity::all(&config_dir().unwrap()).iter().any(|e| e["kind"] == "pay_account" && e["to"].as_str().is_some_and(|to| to.contains(DEFAULT_REGISTRY))), "no history row");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// not-a-sink-forgets-default-registry-after-set-registry: after set_registry(other),
    /// a 5,000,000 bridge to the default registry's account went out.
    #[test]
    fn every_known_registry_is_refused_as_a_destination_whichever_is_configured() {
        let mut names = fresh();
        names.program = Some([9; 32]);
        for program in std::iter::once(DEFAULT_REGISTRY).chain(RETIRED_REGISTRIES) {
            let id = registry::parse_account(program).unwrap();
            assert_eq!(names.not_a_sink(&id, "take a bridge"), Err("that is a program's account, which can't take a bridge".to_owned()), "{program}");
        }
        assert!(names.not_a_sink(&[9; 32], "take a bridge").is_err(), "the configured one");
        assert!(names.not_a_sink(&[4; 32], "take a bridge").is_ok(), "anyone else's");
        let bridged = parsed(&names.chain_bridge("66".repeat(32), format!("Public/{DEFAULT_REGISTRY}"), "5000000".into(), false));
        assert_eq!(bridged["error"], "that is a program's account, which can't take a bridge", "{bridged}");
    }

    /// receive-privately-entry-no-paying-account: the receive_privately entry had no
    /// `from`, so the holder account that paid its fee (479,472) showed nothing.
    #[test]
    fn a_receive_privately_entry_names_the_holder_that_pays() {
        let tail = format!("case \"$*\" in\n*balance*) echo '  Public/{PUBLIC}  267605984  (default)'; echo '  Private/{PRIVATE}  250  (default)' ;;\nesac");
        let (mut names, dir) = with_wallet("rp-holder", &tail);
        let derived = registry::derive("probealpha");
        let holder = registry::parse_account(PUBLIC).unwrap();
        registry::RECORDS.with(|records| records.borrow_mut().push((derived.id, registry::Resolved { owner: registry::owner_tag(&derived.id, &holder), expires: registry::NEVER, profile: registry::Profile::default(), claimed: false })));
        let started = parsed(&names.cli_receive_privately("probealpha".into()));
        assert_eq!(started["success"], true, "{started}");
        let history = activity::all(&config_dir().unwrap());
        let entry = history.iter().find(|e| e["kind"] == "receive_privately").expect("logged");
        assert_eq!(entry["from"], format!("Public/{PUBLIC}"), "{entry}");
        assert!(activity::involves(entry, &format!("Public/{PUBLIC}")));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// receive-on-october-cli-starts-failing-job: the October CLI has no `receive`, and
    /// cli_receive started a job that failed with clap's "For more information, try
    /// '--help'." and left a failed receive row in the history.
    #[test]
    fn receiving_with_a_cli_too_old_for_it_is_refused_up_front() {
        let (mut names, dir) = with_fake_cli("old-receive", "case \"$*\" in *receive*) echo \"error: unrecognized subcommand 'receive'\" >&2; echo \"For more information, try '--help'.\" >&2; exit 2 ;; esac");
        for answer in [names.cli_receive(), names.cli_receive_all()] {
            let refused = parsed(&answer);
            assert_eq!(refused["error"], cli::OLDER_CLI, "{refused}");
        }
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "no job");
        assert!(!activity::all(&config_dir().unwrap()).iter().any(|e| e["kind"] == "receive"), "no history row");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// chain-sent-corrupt-file-silently-overwritten (L5): a damaged chain-sent.json was read
    /// as empty and written over by the next send, with no set-aside and no warning, and
    /// the send in flight was forgotten.
    #[test]
    fn a_damaged_record_of_sends_is_kept_aside_and_told_and_its_send_still_followed() {
        let mut names = fresh();
        let dir = config_dir().unwrap();
        let hash = "c1".repeat(32);
        activity::add(&dir, now(), json!({ "kind": "send", "chain": "logos", "from": NODE_KEY, "to": "39".repeat(32), "amount": "111", "hash": hash }));
        std::fs::write(dir.join("chain-sent.json"), "{\"x\":").unwrap();
        chain_sent_add(&"c2".repeat(32), json!({ "from": NODE_KEY, "inputs": ["n112"], "at": now() }));
        let kept: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("chain-sent.json.corrupt-")).collect();
        assert_eq!(kept.len(), 1, "set aside");
        assert_eq!(std::fs::read_to_string(dir.join(&kept[0])).unwrap(), "{\"x\":", "kept as it was");
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["n111"] } }] } }));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "n112", "value": "5" }] }));
        let shown = parsed(&names.activity(String::new()));
        assert!(shown["warning"].as_str().unwrap().contains("chain-sent.json.corrupt-"), "{shown}");
        assert_eq!(shown["value"][0]["state"], "in_block", "{shown}");
        assert_eq!(parsed(&names.chain_tx(hash))["value"]["state"], "in_block");
    }

    /// logos-send-stuck-sending-without-chain-sent-record (L4): a send of 700 that landed
    /// stayed `sending` for good once its hash was missing from chain-sent.json, and held
    /// both keys' balance marks.
    #[test]
    fn a_logos_send_without_its_record_is_settled_by_the_node_or_becomes_unknown() {
        let mut names = fresh();
        let dir = config_dir().unwrap();
        let landed = activity::add(&dir, now(), json!({ "kind": "send", "chain": "logos", "from": NODE_KEY, "to": "39".repeat(32), "amount": "700", "hash": "aa".repeat(32) }));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["n700"], "outputs": [{ "value": 700 }] } }] } }));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "n9", "value": "93798" }] }));
        let state = |id: &str| activity::all(&dir).into_iter().find(|e| e["id"] == id).unwrap()["state"].clone();
        assert_eq!(parsed(&names.activity(String::new()))["success"], true);
        assert_eq!(state(&landed), "in_block", "the node shows its input spent");
        // Nothing can tell, for longer than any send takes: unknown, not sending for good.
        let old = activity::add(&dir, now() - activity::SENDING_WAIT_S - 1, json!({ "kind": "send", "chain": "logos", "from": NODE_KEY, "to": "39".repeat(32), "amount": "5", "hash": "bb".repeat(32) }));
        node_answers("get_transaction", json!(null));
        names.activity(String::new());
        assert_eq!(state(&old), "unknown");
    }

    /// bridge-destination-empty-or-program-account-accepted: chain_bridge and cli_bridge_to
    /// took the all-1s account and the registry's own as the LEZ destination (a real
    /// deposit to the empty account landed), while cli_transfer refused both.
    #[test]
    fn a_bridge_to_the_empty_account_or_a_programs_is_refused() {
        let empty = "Public/11111111111111111111111111111111".to_owned();
        let mut names = fresh();
        names.program = Some([7; 32]);
        let program = format!("Public/{}", registry::account_text(&[7; 32]));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "e8ac", "value": "5000000" }, { "id": "c0de", "value": "900000" }] }));
        for send in [false, true] {
            let refused = parsed(&names.chain_bridge(NODE_KEY.into(), empty.clone(), "5000000".into(), send));
            assert_eq!(refused["error"], "that is the empty account (all 1s), which no one holds", "{refused}");
            let refused = parsed(&names.chain_bridge(NODE_KEY.into(), program.clone(), "5000000".into(), send));
            assert_eq!(refused["error"], "that is a program's account, which can't take a bridge", "{refused}");
        }
        assert!(!node_asked().contains(&"channel_deposit_with_notes".to_owned()), "nothing deposited");
        let (mut names, dir) = with_fake_cli("bridge-sink", "echo 'ok bridge'");
        for to in [empty, format!("Public/{}", registry::account_text(&[7; 32]))] {
            let refused = parsed(&names.cli_bridge_to("5000".into(), NODE_KEY.into(), to.clone(), "http://127.0.0.1:9".into(), false));
            assert_eq!(refused["success"], false, "{to}: {refused}");
        }
        assert_eq!((names.bridge_key.clone(), names.bridge_node.clone()), (None, None), "nothing saved");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// RV5: a bridge plan isn't "ready" when the exact note is the key's only one (nothing pays the fee).
    #[test]
    fn review_a_bridge_plan_with_nothing_left_for_the_fee_is_not_ready() {
        let mut names = fresh();
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "e8ac", "value": "1000000" }] }));
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let plan = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), false));
        assert!(
            plan["success"] == false || plan["value"]["needs_split"] == true || plan["value"]["fee_short"] == true,
            "the plan says Ready although no other note of the key can pay the deposit fee: {plan}"
        );
    }

    /// RV6: the bridge checks that LEZ's deposit channel exists on the node's chain before depositing.
    #[test]
    fn review_the_bridge_checks_the_channel_on_the_node_before_depositing() {
        let mut names = fresh();
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "e9ac", "value": "1000000" }, { "id": "c1de", "value": "5000000" }] }));
        node_answers("channel_deposit_with_notes", json!("ac".repeat(32)));
        node_answers("get_channel_state", json!({ "configuration_threshold": 3 }));
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        let _ = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1000000".into(), true));
        let asked = node_asked();
        let deposit = asked.iter().position(|m| m == "channel_deposit_with_notes");
        let checked = asked.iter().position(|m| m == "get_channel_state");
        assert!(deposit.is_none() || checked.is_some_and(|c| c < deposit.unwrap()), "deposited without asking the node for the channel: {asked:?}");
    }

    #[test]
    fn a_send_refused_while_another_still_spends_the_keys_notes_says_so() {
        let mut names = fresh();
        let key = "e3".repeat(32);
        node_answers("wallet_get_balance", json!("100000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "f08a", "value": "100000" }] }));
        node_answers("wallet_transfer_funds", json!("d1".repeat(32)));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["f08a"], "outputs": [{ "value": 1000 }, { "value": 98166 }] } }] } }));
        assert!(parsed(&names.chain_send(key.clone(), "39".repeat(32), "1,000".into(), true))["success"] == true);
        // The node, its one note in flight, says it hasn't enough.
        node_refuses("wallet_transfer_funds", "Wallet does not have enough funds, available=0");
        let second = parsed(&names.chain_send(key.clone(), "39".repeat(32), "1,000".into(), true));
        assert!(second["error"].as_str().unwrap().contains("still waiting to land"), "{second}");
        // Once that note is spent, the node's words are kept.
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "c07e", "value": "500" }] }));
        let short = parsed(&names.chain_send(key, "39".repeat(32), "400".into(), true));
        assert_eq!(short["error"], "That key doesn't hold enough for this and its fee.", "{short}");
    }

    /// A bridge's account is read afresh between two asks of its receipt: the balance then
    /// holds its credit exactly when it arrives. A mint between the asks, or an ask LEZ
    /// doesn't answer, leaves the account unread this time (its credit would otherwise be
    /// told twice, as the bridge and as money that arrived).
    #[test]
    fn a_bridges_account_is_read_between_two_asks_of_its_receipt() {
        let names = fresh();
        let dir = config_dir().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let account = [5_u8; 32];
        let to = format!("Public/{}", registry::account_text(&account));
        registry::BALANCES.with(|held| held.borrow_mut().push((account, 23_000)));
        let id = activity::add(&dir, now(), json!({ "kind": "bridge", "chain": "logos", "to": to, "amount": "3000",
            "note": "11".repeat(32), "channel": "00".repeat(32), "state": "in_block" }));
        let receipt = activity::bridge_receipt(&activity::all(&dir)[0]).unwrap();
        let state = || activity::all(&dir).into_iter().find(|e| e["id"] == id.as_str()).unwrap()["state"].as_str().unwrap().to_owned();
        let stale = [(to.clone(), 20_000_u128), ("Public/other".to_owned(), 7)];
        // Not minted: the account is read afresh (the wallet's read may be older).
        assert_eq!(names.settle_activity(&dir, "seq", &stale, std::time::Instant::now() + cli::CALL_BUDGET), [(to.clone(), 23_000), ("Public/other".to_owned(), 7)]);
        assert_eq!(state(), "in_block");
        // LEZ doesn't answer for the receipt: the account isn't read this time.
        registry::MINTED_UNREAD.with(|unread| unread.set(true));
        assert_eq!(names.settle_activity(&dir, "seq", &stale, std::time::Instant::now() + cli::CALL_BUDGET), [("Public/other".to_owned(), 7)]);
        registry::MINTED_UNREAD.with(|unread| unread.set(false));
        assert_eq!(state(), "in_block");
        // Minted between the first ask and the second: arrived, and the account isn't read this time.
        let next = registry::MINTED_ASKED.with(std::cell::Cell::get);
        registry::MINTED.with(|minted| minted.borrow_mut().push((receipt, next + 1)));
        assert_eq!(names.settle_activity(&dir, "seq", &stale, std::time::Instant::now() + cli::CALL_BUDGET), [("Public/other".to_owned(), 7)]);
        assert_eq!(state(), "arrived");
        // Arrived, it isn't asked again; the account is the wallet's read.
        assert_eq!(names.settle_activity(&dir, "seq", &stale, std::time::Instant::now() + cli::CALL_BUDGET), stale);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// deposit-lost-from-activity-on-unload: names unloaded while chain_bridge waited on
    /// the node: the exact note was spent, and neither chain-sent nor the history had it.
    #[test]
    fn a_deposit_cut_off_mid_call_is_kept_and_settled_by_its_note() {
        const A640: &str = "a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6";
        let mut names = fresh();
        let key = "4d".repeat(32);
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6", "value": "640080" }, { "id": "fee1", "value": "5000000" }] }));
        node_answers("get_channel_state", json!({ "configuration_threshold": 3 }));
        // No answer, as when names is unloaded while the node proves (the real run's error
        // was the node "didn't answer", and the deposit had been made).
        node::ANSWERS.with(|answers| answers.borrow_mut().remove("channel_deposit_with_notes"));
        let cut = parsed(&names.chain_bridge(key.clone(), format!("Public/{to}"), "640,080".into(), true));
        assert_eq!(cut["success"], false, "{cut}");
        let mine = |names: &mut Names| parsed(&names.activity(key.clone()))["value"].as_array().unwrap().iter().find(|e| e["amount"] == "640080").cloned();
        let kept = mine(&mut names).expect("the deposit is in the history");
        assert_eq!((kept["state"].as_str(), kept["note"].as_str(), kept["hash"].clone()), (Some("sending"), Some("a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6"), Value::Null), "{kept}");
        // Its note stays held: it isn't deposited twice.
        let again = parsed(&names.chain_bridge(key.clone(), format!("Public/{to}"), "640,080".into(), true));
        assert!(again["error"].as_str().unwrap().contains("still using its note"), "{again}");
        // The node spent the note: the deposit was made. bridge-settled-after-unload-has-no-hash-or-fee:
        // its transaction is found in a block's events by the deposit's op id, with its fee.
        let op: Vec<u8> = registry::unhex(kept["op_id"].as_str().unwrap()).unwrap();
        let hash = "7a".repeat(32);
        node_answers("get_cryptarchia_info", json!({ "tip": "b2", "height": 2 }));
        node_answers("get_block", json!({ "header": { "parent_block": "b1" } }));
        node_answers("get_block_events", json!([{ "Tx": { "tx_hash": hash, "op_id": op, "payload": { "Deposit": { "amount": 640080 } } } }]));
        node_answers("get_transaction", json!({ "mantle_tx": { "ops": [
            { "opcode": 18, "payload": { "inputs": [A640] } },
            { "opcode": 0, "payload": { "inputs": ["fee1"], "outputs": [{ "value": 4998386 }] } }] } }));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "fee2", "value": "4998386" }] }));
        let settled = mine(&mut names).unwrap();
        assert_eq!((settled["state"].as_str(), settled["hash"].as_str(), settled["fee"].as_str()), (Some("in_block"), Some(hash.as_str()), Some("1614")), "{settled}");
        assert_eq!(parsed(&names.chain_tx(hash.clone()))["value"], json!({ "hash": hash, "state": "in_block", "fee": "1614" }));
        // A refusal, by contrast, leaves no record and holds nothing.
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7", "value": "700000" }, { "id": "fee3", "value": "5000000" }] }));
        node_refuses("channel_deposit_with_notes", "invalid public key");
        assert_eq!(parsed(&names.chain_bridge(key.clone(), format!("Public/{to}"), "700,000".into(), true))["success"], false);
        let refused = parsed(&names.activity(key.clone()))["value"].as_array().unwrap().iter().find(|e| e["amount"] == "700000").cloned().unwrap();
        assert_eq!(refused["state"], "failed", "{refused}");
        assert!(!chain_notes_in_use(&key).contains("b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7"));
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
        assert_eq!(plan["value"], json!({ "claimable": "0", "vouchers": 0, "reward_per_voucher": "13140", "fee_about": null }), "{plan}");
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
    fn the_board_keeps_few_answers_and_runs_few_fetches_at_once() {
        let names = fresh();
        let slow = || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            Ok(json!([]))
        };
        for round in 0..3 {
            for i in 0..100 {
                let _ = names.board_read(format!("author:ada:{}", round * 100 + i), slow);
            }
            {
                let reads = names.board_reads.lock().unwrap();
                assert!(reads.len() <= MAX_BOARD_READS, "{} answers kept", reads.len());
                assert!(reads.values().filter(|read| read.updating).count() <= MAX_BOARD_FETCHES);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let junk = parsed(&fresh().board_thread("x".repeat(5000)));
        assert_eq!(
            cli::parse_waiting("private total: 0\nwaiting to be claimed: 2 payment(s), 4000000 in all; run `names receive`\n"),
            json!({ "count": "2", "total": "4000000" })
        );
        assert_eq!(cli::parse_waiting("private total: 0\n"), Value::Null);
        assert!(junk["error"].as_str().unwrap().contains("not a post"), "{junk}");
    }

    #[test]
    fn another_app_reads_only_the_network_part_of_the_config() {
        let mut names = fresh();
        names.main_name = Some("jef".into());
        names.follows = vec!["ada".into()];
        let own = parsed(&names.get_config());
        assert_eq!(own["main_name"], "jef", "{own}");
        as_caller(logos_rust_sdk::LogosCaller::Module { name: "evil_ui".into(), instance: None });
        let other = parsed(&names.get_config());
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let keys: Vec<&String> = other.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["module_version", "names_sequencer", "registry", "sequencer", "token_program", "zone"], "{other}");
        let zone: Vec<&String> = other["zone"].as_object().unwrap().keys().collect();
        assert_eq!(zone, ["channel", "home", "id"], "{other}");
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
                        names.cli_status(), names.wallet(), names.job("victim-1".into()), names.board_feed(true),
                        names.cli_receive(), names.cli_receive_all(), names.cli_receive_privately("victim".into()),
                        names.cli_transfer("victim".into(), "Private/7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR".into()), names.activity(String::new())] {
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
        assert_eq!(parsed(&names.cli_shield("0".into()))["error"], "Enter an amount above 0.");
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
        // Another wallet's account LEZ has never seen: refused, nothing runs (F2X: the
        // send landed, kept a fee of 12088 and credited nothing, logged done).
        registry::UNSEEN.with(|unseen| unseen.borrow_mut().push(registry::parse_account(other).unwrap()));
        let refused = parsed(&names.cli_pay_account(format!("Public/{other}"), "1000".into(), false));
        assert!(refused["error"].as_str().unwrap().contains("never been used on LEZ"), "{refused}");
        assert!(parsed(&names.cli_pay_account(format!("Public/{other}"), "1000".into(), true))["error"].as_str().unwrap().contains("never been used"));
        registry::UNSEEN.with(|unseen| unseen.borrow_mut().clear());
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
        // The version the app shows in its corner is its package's.
        let shown = format!("readonly property string appVersion: \"{}\"", app["version"].as_str().expect("the app's version"));
        assert!(include_str!("../../../basecamp/qml/Main.qml").contains(&shown), "Main.qml must have {shown}");
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
    fn a_token_added_by_address_is_kept_and_a_wrong_one_refused() {
        let demo = registry::account_text(&DEMO);
        let missing = registry::account_text(&[8; 32]);
        let tail = format!(
            "case \"$*\" in\n\
             *\"module token info -- {missing}\"*) echo 'Error: not a token' >&2; exit 1 ;;\n\
             *\"module token info\"*) echo '{{\"definition\":\"{demo}\",\"name\":\"Demo\",\"supply\":\"1000\",\"authority\":null}}' ;;\n\
             esac"
        );
        let (mut names, dir) = with_wallet("token-watch", &tail);
        assert!(parsed(&names.watched_tokens())["error"].as_str().unwrap().contains("no token program"));
        names.token_program = Some(PROGRAM);
        let _ = std::fs::remove_file(config_dir().unwrap().join("tokens-watched.json"));
        assert_eq!(parsed(&names.watched_tokens())["value"], json!([]));
        // An ID that isn't a token of the program is refused, and nothing is kept.
        assert_eq!(parsed(&names.watch_token(missing, true))["success"], false);
        assert_eq!(parsed(&names.watch_token("not an id".into(), true))["success"], false);
        // A real one is read and kept, once, with its name; it survives a restart.
        let added = parsed(&names.watch_token(demo.clone(), true));
        assert_eq!(added["value"], json!([{ "definition": demo, "name": "Demo" }]), "{added}");
        names.watch_token(demo.clone(), true);
        let mut again = fresh();
        again.token_program = Some(PROGRAM);
        assert_eq!(parsed(&again.watched_tokens())["value"], json!([{ "definition": demo, "name": "Demo" }]));
        // Kept per program, and only the Names app may change it.
        again.token_program = Some([9; 32]);
        assert_eq!(parsed(&again.watched_tokens())["value"], json!([]));
        as_caller(app("evil_ui"));
        assert!(parsed(&names.watch_token(demo.clone(), false))["error"].as_str().unwrap().contains("only the Names app"));
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        assert_eq!(parsed(&names.watch_token(demo, false))["value"], json!([]));
        let _ = std::fs::remove_file(config_dir().unwrap().join("tokens-watched.json"));
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
            r#"[{{"account":"Public/{PUBLIC}","kind":"public","label":"Main","balance":"12","default":true,"public_key":"{key}"}},{{"account":"Private/{PRIVATE}","kind":"private","label":"bad\u0007","balance":"x","default":false,"public_key":"{key}","npk":"{key}"}}]"#
        );
        let (mut names, dir) = with_wallet("accounts-list", &format!("echo '{list}'"));
        let got = parsed(&names.cli_accounts());
        assert_eq!(got["success"], true, "{got}");
        let rows = got["value"].as_array().unwrap();
        assert_eq!(rows[0], json!({ "account": format!("Public/{PUBLIC}"), "kind": "public", "label": "Main", "balance": "12", "default": true, "public_key": key, "npk": null }));
        // A private account's npk comes through (a public key: a name's profile publishes it).
        assert_eq!(rows[1], json!({ "account": format!("Private/{PRIVATE}"), "kind": "private", "label": null, "balance": null, "default": false, "public_key": null, "npk": key }));
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

    /// LP-0022 accounts: the new methods are the Names app's alone, a new account is
    /// listed (with its label) before the node uses it, and without the node's keystore
    /// beside the module's data nothing is made.
    #[test]
    fn chain_accounts_are_the_names_apps_and_list_the_ones_made_here() {
        let mut names = fresh();
        let dir = config_dir().unwrap();
        let _ = std::fs::remove_file(chain_keys::records_path(&dir));
        as_caller(logos_rust_sdk::LogosCaller::Module { name: "evil_ui".into(), instance: None });
        for refused in [names.chain_account_new("x".into()), names.chain_account_label(NODE_KEY.into(), "x".into()), names.chain_account_remove(NODE_KEY.into())] {
            assert!(parsed(&refused)["error"].as_str().unwrap().contains("only the Names app"), "{refused}");
        }
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        assert!(node_asked().is_empty(), "nothing was asked for the other app");
        node_answers("wallet_get_known_addresses", json!([NODE_KEY]));
        node_answers("wallet_get_balance", json!("7"));
        // The test process has no --instance-persistence-path: no keystore, nothing made.
        assert_eq!(parsed(&names.chain_account_new("Savings".into()))["error"], chain_keys::NO_KEYSTORE);
        assert!(!node_asked().contains(&"generate_key".to_owned()));
        let made = "ab".repeat(32);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(chain_keys::records_path(&dir), json!({ made.clone(): { "title": "names-1", "label": "Savings", "at": 1 } }).to_string()).unwrap();
        let labelled = parsed(&names.chain_account_label(NODE_KEY.into(), "Main".into()));
        assert_eq!(labelled["value"], json!({ "key": NODE_KEY, "label": "Main" }), "{labelled}");
        let shown = parsed(&names.chain_accounts());
        assert_eq!(shown["value"], json!([
            { "key": NODE_KEY, "balance": "7", "label": "Main", "ready": true },
            { "key": made, "balance": null, "title": "names-1", "label": "Savings", "ready": false },
        ]), "{shown}");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(chain_keys::records_path(&dir)).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_file(chain_keys::records_path(&dir));
    }

    /// LP-0022 gas: each chain plan carries the node's own fee for it (`fee_about`), asked
    /// with stand-ins of the key's notes and no funding key, so no real note is reserved.
    /// The replies are a solo 0.3.4 node's (exp2: 100,000 in, 3,000 + 96,166 out = 834).
    #[test]
    fn chain_plans_carry_the_nodes_fee_estimate_without_naming_a_real_note() {
        let mut names = fresh();
        let to = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225";
        let fund_asked = || {
            node::ASKED.with(|asked| asked.borrow().iter().rev().find(|(m, _)| m == "wallet_fund_tx").map(|(_, a)| serde_json::from_str::<Value>(a[0].as_str().unwrap()).unwrap()))
        };
        node_answers("wallet_get_balance", json!("100000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "f08a", "value": "100000" }] }));
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "pk": to, "value": 3000 }, { "pk": NODE_KEY, "value": 96166 }] } }] }, "tip": "b3" }));
        let plan = parsed(&names.chain_send(NODE_KEY.into(), to.into(), "3,000".into(), false));
        assert_eq!(plan["value"]["fee_about"], "834", "{plan}");
        let request = fund_asked().expect("the node is asked for the fee");
        assert_eq!(request["funding_public_keys"], json!([]), "no key the node could take a real note from");
        assert_eq!(request["tx_builder"]["ledger_inputs"][0]["note"], json!({ "value": 100000, "pk": NODE_KEY }));
        assert!(!request.to_string().contains("f08a"), "the real note's id is never named: {request}");
        assert!(!node_asked().contains(&"wallet_transfer_funds".to_owned()), "a plan sends nothing");
        // A split is the same transfer, to the key itself; nothing to split, nothing to pay.
        let split = parsed(&names.chain_split(NODE_KEY.into(), "3,000".into(), false));
        assert_eq!(split["value"]["fee_about"], "834", "{split}");
        assert_eq!(fund_asked().unwrap()["tx_builder"]["pending_transfer"]["outputs"], json!([{ "value": 3000, "pk": NODE_KEY }]));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "f08a", "value": "100000" }, { "id": "e300", "value": "3000" }] }));
        assert_eq!(parsed(&names.chain_split(NODE_KEY.into(), "3,000".into(), false))["value"]["fee_about"], "0", "an exact note is already there");
        // When the node can't say, the plan still stands, without an estimate.
        node_refuses("wallet_fund_tx", "Failed to parse fund request: missing field `tx_builder`");
        let plan = parsed(&names.chain_send(NODE_KEY.into(), to.into(), "3,000".into(), false));
        assert_eq!((plan["success"].clone(), plan["value"]["fee_about"].clone()), (json!(true), Value::Null), "{plan}");
        assert_eq!(plan["value"]["cannot_pay_fee"], Value::Null, "a node that didn't say is not a short key");
        // The node says no set of the key's notes can pay it with its fee (the proof run:
        // 1,268 in two notes, a plan for 1,000): the plan says so instead of a figure.
        node_answers("wallet_get_balance", json!("1268"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "a1", "value": "1000" }, { "id": "a2", "value": "268" }] }));
        node_refuses("wallet_fund_tx", "Failed to fund tx: Wallet does not have enough funds, available=0");
        let plan = parsed(&names.chain_send(NODE_KEY.into(), to.into(), "1,000".into(), false));
        assert_eq!((plan["value"]["fee_about"].clone(), plan["value"]["cannot_pay_fee"].clone()), (Value::Null, json!(true)), "{plan}");
        // A note a send from here still spends is left out, as the node's wallet leaves it out.
        node_answers("wallet_get_balance", json!("1000000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "busy", "value": "900000" }, { "id": "free", "value": "100000" }] }));
        chain_sent_add(&"ee".repeat(32), json!({ "from": NODE_KEY, "inputs": ["busy"], "fee": null, "at": now() }));
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "value": 3000 }, { "value": 96166 }] } }] } }));
        assert_eq!(parsed(&names.chain_send(NODE_KEY.into(), to.into(), "3,000".into(), false))["value"]["fee_about"], "834");
        assert_eq!(fund_asked().unwrap()["tx_builder"]["ledger_inputs"].as_array().unwrap().len(), 1);
        assert_eq!(fund_asked().unwrap()["tx_builder"]["ledger_inputs"][0]["note"]["value"], 100000);
        chain_sent_remove(&"ee".repeat(32));
    }

    /// A claim's plan shows what one claim costs and warns when that is more than the
    /// one reward it cashes (the node's leader_claim has no fee cap names can set).
    #[test]
    fn a_claim_plan_shows_its_fee_and_warns_when_it_is_more_than_the_reward() {
        let mut names = fresh();
        node_answers("wallet_get_claimable_vouchers", json!({ "reward_amount": "38", "total_claimable": "76", "vouchers": [{ "nullifier": "n1" }, { "nullifier": "n2" }] }));
        node::ANSWERS.with(|answers| answers.borrow_mut().insert("wallet_get_known_addresses".into(), json!({ "success": true, "value": [NODE_KEY] })));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "af49", "value": "50000" }] }));
        // As the solo node funded a stand-in claim (exp2 claim-a): 50,000 in, 48,401 back.
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [
            { "opcode": 48, "payload": { "pk": NODE_KEY } },
            { "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "pk": NODE_KEY, "value": 48401 }] } }] } }));
        let plan = parsed(&names.chain_claim(false));
        assert_eq!(plan["value"]["fee_about"], "1599", "{plan}");
        assert!(plan["value"]["warning"].as_str().unwrap().contains("reward of 38 and pays a fee of about 1599"), "{plan}");
        assert!(!node_asked().contains(&"leader_claim".to_owned()), "a plan claims nothing");
        let asked = node::ASKED.with(|asked| asked.borrow().iter().rev().find(|(m, _)| m == "wallet_fund_tx").map(|(_, a)| a.clone())).unwrap();
        let request: Value = serde_json::from_str(asked[0].as_str().unwrap()).unwrap();
        assert_eq!(request["tx_builder"]["mantle_tx"]["ops"][0]["opcode"], 0x30);
        assert_eq!(request["funding_public_keys"], json!([]));
    }

    /// A bridge plan: the deposit's own fee, from the key's other notes (the exact note goes
    /// whole into the deposit and is never named); with no exact note, the split's.
    #[test]
    fn a_bridge_plan_estimates_the_deposit_or_the_split_before_it() {
        let mut names = fresh();
        let to = "7wx3U7qMRWopjKXH67t9WpuNiNyB12pXUhB6CNA1LznR";
        node_answers("get_channel_state", json!({}));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "e8ac", "value": "1000000" }, { "id": "fee1", "value": "5000000" }] }));
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [
            { "opcode": 18, "payload": { "inputs": ["03e8"] } },
            { "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "value": 4998386 }] } }] } }));
        let plan = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "1,000,000".into(), false));
        assert_eq!(plan["value"]["fee_about"], "1614", "{plan}");
        let asked = node::ASKED.with(|asked| asked.borrow().iter().rev().find(|(m, _)| m == "wallet_fund_tx").map(|(_, a)| a.clone())).unwrap();
        let request: Value = serde_json::from_str(asked[0].as_str().unwrap()).unwrap();
        assert_eq!(request["tx_builder"]["ledger_inputs"].as_array().unwrap().len(), 1);
        assert_eq!(request["tx_builder"]["ledger_inputs"][0]["note"]["value"], 5000000, "the fee note, not the deposit's");
        assert!(!request.to_string().contains("e8ac"));
        // No exact note: the split comes first, so that is what the plan prices.
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "value": 700000 }, { "value": 4299134 }] } }] } }));
        let plan = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{to}"), "700,000".into(), false));
        assert_eq!((plan["value"]["needs_split"].clone(), plan["value"]["fee_about"].clone(), plan["value"]["split_fee_about"].clone()), (json!(true), Value::Null, json!("866")), "{plan}");
    }

    /// adv.py H4: the key's only note (100,000) is the input of a send made here that isn't in
    /// a block yet. A plan then is short only because of it: it says the note is in flight,
    /// not that the key can't pay ("send a little less"); once the send lands, the fee.
    #[test]
    fn a_plan_while_the_keys_notes_are_in_flight_says_so_not_send_less() {
        let mut names = fresh();
        let to = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225";
        node_answers("wallet_get_balance", json!("100000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "f08a", "value": "100000" }] }));
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "value": 1000 }, { "value": 98166 }] } }] } }));
        let hash = "d6".repeat(32);
        chain_sent_add(&hash, json!({ "from": NODE_KEY, "inputs": ["f08a"], "fee": "834", "at": now() }));
        for plan in [names.chain_send(NODE_KEY.into(), to.into(), "1,000".into(), false), names.chain_split(NODE_KEY.into(), "1,000".into(), false)] {
            let plan = parsed(&plan);
            let shown = &plan["value"];
            assert_eq!((shown["in_flight"].clone(), shown["cannot_pay_fee"].clone(), shown["fee_about"].clone()), (json!(true), Value::Null, Value::Null), "{plan}");
        }
        // Short with the busy note counted too: that is "send less", as before.
        node_refuses("wallet_fund_tx", "Failed to fund tx: Wallet does not have enough funds, available=0");
        let plan = parsed(&names.chain_send(NODE_KEY.into(), to.into(), "1,000".into(), false));
        assert_eq!((plan["value"]["in_flight"].clone(), plan["value"]["cannot_pay_fee"].clone()), (Value::Null, json!(true)), "{plan}");
        // Landed: its input is gone from the wallet, the record lapses, the plan has a fee.
        chain_sent_remove(&hash);
        node_answers("wallet_fund_tx", json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["01"], "outputs": [{ "value": 1000 }, { "value": 98166 }] } }] } }));
        let plan = parsed(&names.chain_send(NODE_KEY.into(), to.into(), "1,000".into(), false));
        assert_eq!((plan["value"]["in_flight"].clone(), plan["value"]["fee_about"].clone()), (Value::Null, json!("834")), "{plan}");
    }

    /// adv.py H2 and the fixer's probe: a plan to 64 hex characters that aren't a key. The
    /// node refused the stand-in with "Parsed bytes are bigger than the modulus" and the send
    /// with "That isn't a Logos Blockchain public key"; the plan now says what the send says.
    #[test]
    fn a_plan_to_a_recipient_that_isnt_a_key_is_refused_as_the_send_is() {
        let mut names = fresh();
        node_answers("wallet_get_balance", json!("100000"));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "f08a", "value": "100000" }] }));
        node_refuses("wallet_fund_tx", "Failed to parse fund request: Parsed bytes are bigger than the modulus, got 77648812782670860460512307594061302913369283834606025297048026922953510464427, expected 21888242871839275222246405745257275088548364400416034343698204186575808495617 at line 1 column 553");
        let plan = parsed(&names.chain_send(NODE_KEY.into(), "ab".repeat(32), "1,000".into(), false));
        assert_eq!((plan["success"].clone(), plan["error"].clone()), (json!(false), json!("That isn't a Logos Blockchain public key: check it and try again.")), "{plan}");
        assert!(!node_asked().contains(&"wallet_transfer_funds".to_owned()), "a plan sends nothing");
    }

    // ── Zones (LP-0022) ─────────────────────────────────────────────────

    /// Where the tests' second zone has its sequencer.
    const ZONE_B: &str = "http://127.0.0.1:7";

    /// Has the sequencer at `url` name `channel`, or not answer and say why.
    fn sequencer_says(url: &str, channel: Result<&str, &str>) {
        registry::CHANNELS.with(|known| {
            let mut known = known.borrow_mut();
            known.retain(|(known_url, _)| known_url != url);
            known.push((url.to_owned(), channel.map(str::to_owned).map_err(str::to_owned)));
        });
    }

    /// `with_wallet`, its sequencer serving channel 01…, with a second zone "Second"
    /// (channel ab…, at `ZONE_B`) added and not in use. Returns the zone's id too.
    fn with_two_zones(label: &str, tail: &str) -> (Names, std::path::PathBuf, String) {
        let (mut names, dir) = with_wallet(label, tail);
        let _ = std::fs::remove_dir_all(config_dir().unwrap());
        sequencer_says("http://127.0.0.1:9", Ok(&"01".repeat(32)));
        sequencer_says(ZONE_B, Ok(&"AB".repeat(32)));
        let added = parsed(&names.zone_add("Second".into(), ZONE_B.into()));
        assert_eq!(added["value"]["added"], "ab".repeat(32), "{added}");
        (names, dir, "ab".repeat(32))
    }

    /// Uses zone `id`, and gives its wallet folder a wallet with home's accounts if it
    /// has none (the same keys make the same account IDs on every zone).
    fn use_zone(names: &mut Names, id: &str) {
        let used = parsed(&names.zone_use(id.into()));
        assert_eq!(used["value"]["current"], id, "{used}");
        let wallet = std::path::PathBuf::from(names.cli_wallet.clone().expect("a wallet folder"));
        if !wallet.join(cli::STORAGE_FILE).exists() {
            std::fs::write(wallet.join(cli::STORAGE_FILE), "{}").unwrap();
            std::fs::write(wallet.join("names.json"), format!("{{\"public\":\"{PUBLIC}\",\"private\":\"{PRIVATE}\"}}")).unwrap();
        }
    }

    fn zone_test_end(dir: std::path::PathBuf) {
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(config_dir().unwrap());
    }

    #[test]
    fn zones_are_added_listed_renamed_and_removed() {
        let (mut names, dir, second) = with_two_zones("zones-list", "exit 0");
        let home_wallet = names.cli_wallet.clone().unwrap();
        let listed = parsed(&names.zone_list())["value"].clone();
        assert_eq!((listed["current"].as_str(), listed["max"].as_u64(), listed["zones"].as_array().unwrap().len()), (Some("home"), Some(16), 2), "{listed}");
        let (home, added) = (&listed["zones"][0], &listed["zones"][1]);
        assert_eq!(
            (home["id"].as_str(), home["name"].as_str(), home["channel"].as_str(), home["sequencer"].as_str(), home["home"].as_bool(), home["current"].as_bool()),
            (Some("home"), Some("Home"), Some("01".repeat(32).as_str()), Some("http://127.0.0.1:9"), Some(true), Some(true)),
            "{home}"
        );
        assert_eq!((home["wallet_dir"].as_str(), home["has_wallet"].as_bool()), (Some(home_wallet.as_str()), Some(true)), "{home}");
        assert_eq!(
            (added["id"].as_str(), added["name"].as_str(), added["sequencer"].as_str(), added["home"].as_bool(), added["current"].as_bool(), added["has_wallet"].as_bool()),
            (Some(second.as_str()), Some("Second"), Some(ZONE_B), Some(false), Some(false), Some(false)),
            "{added}"
        );
        // Its wallet is a folder of its own, never home's.
        assert!(added["wallet_dir"].as_str().unwrap().ends_with(&format!("zones/{second}/wallet")), "{added}");

        // A zone is its channel: the same chain at another address is not a second zone,
        // and neither is home's.
        sequencer_says("https://again.example", Ok(&"ab".repeat(32)));
        let again = parsed(&names.zone_add("Again".into(), "https://again.example".into()));
        assert!(again["error"].as_str().unwrap().contains("serves Second, a zone already set up here"), "{again}");
        sequencer_says("https://home-too.example", Ok(&"01".repeat(32)));
        let home_too = parsed(&names.zone_add(String::new(), "https://home-too.example".into()));
        assert!(home_too["error"].as_str().unwrap().contains("serves Home"), "{home_too}");
        // A sequencer that doesn't say which chain it serves can't be told apart.
        sequencer_says("https://quiet.example", Err("it didn't answer in time."));
        let quiet = parsed(&names.zone_add("Quiet".into(), "https://quiet.example".into()));
        assert!(quiet["error"].as_str().unwrap().contains("didn't say which chain it serves, so it can't be added. (it didn't answer in time)"), "{quiet}");
        // An address is checked as every sequencer's is; a name as a zone's is.
        for url in ["ftp://x.example", "http://evil.example", ""] {
            assert_eq!(parsed(&names.zone_add("X".into(), url.into()))["error"], NOT_A_SEQUENCER, "{url}");
        }
        sequencer_says("https://third.example", Ok(&"cd".repeat(32)));
        assert_eq!(parsed(&names.zone_add("second".into(), "https://third.example".into()))["success"], false, "a name in use");
        assert_eq!(parsed(&names.zone_add("x".repeat(zones::MAX_NAME + 1), "https://third.example".into()))["success"], false, "too long");
        // With no name it is called by its channel.
        let third = parsed(&names.zone_add(String::new(), "https://third.example".into()))["value"].clone();
        assert_eq!(third["zones"][2]["name"], "Zone cdcdcdcd", "{third}");

        let renamed = parsed(&names.zone_rename(second.clone(), "Shop zone".into()))["value"].clone();
        assert_eq!(renamed["zones"][1]["name"], "Shop zone", "{renamed}");
        assert_eq!(parsed(&names.zone_rename(second.clone(), "Zone cdcdcdcd".into()))["success"], false, "another zone's name");
        assert_eq!(parsed(&names.zone_rename("nope".into(), "X".into()))["error"], "there is no such zone here");

        // Home stays, and so does the zone in use; any other is forgotten, its folder kept.
        assert!(parsed(&names.zone_remove("home".into()))["error"].as_str().unwrap().contains("home zone can't be removed"));
        use_zone(&mut names, &second);
        assert!(parsed(&names.zone_remove(second.clone()))["error"].as_str().unwrap().contains("is the zone in use"));
        let zone_wallet = names.cli_wallet.clone().unwrap();
        use_zone(&mut names, "home");
        let removed = parsed(&names.zone_remove(second.clone()))["value"].clone();
        assert_eq!((removed["removed"]["name"].as_str(), removed["removed"]["wallet_dir"].as_str()), (Some("Shop zone"), Some(zone_wallet.as_str())), "{removed}");
        assert_eq!(removed["zones"].as_array().unwrap().len(), 2, "{removed}");
        assert!(std::path::Path::new(&zone_wallet).join(cli::STORAGE_FILE).is_file(), "the wallet folder stays");
        assert_eq!(parsed(&names.zone_use(second.clone()))["error"], "there is no such zone here");
        // Added again, it finds its wallet.
        let back = parsed(&names.zone_add("Second".into(), ZONE_B.into()))["value"].clone();
        let back = back["zones"].as_array().unwrap().iter().find(|zone| zone["id"] == second.as_str()).cloned().unwrap();
        assert_eq!((back["wallet_dir"].as_str(), back["has_wallet"].as_bool()), (Some(zone_wallet.as_str()), Some(true)), "{back}");

        // Only the Names app sets zones up.
        as_caller(app("evil_ui"));
        for refused in [names.zone_list(), names.zone_add("E".into(), "https://e.example".into()), names.zone_use(second.clone()), names.zone_remove(second.clone()), names.zone_rename(second.clone(), "E".into()), names.zone_sequencer(second.clone(), "https://e.example".into(), true)] {
            assert_eq!(parsed(&refused)["success"], false, "{refused}");
        }
        assert_eq!(names.sequencer, "http://127.0.0.1:9");
        zone_test_end(dir);
    }

    #[test]
    fn the_zone_in_use_has_its_own_sequencer_wallet_and_token_program_and_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        let (mut names, dir, second) = with_two_zones("zones-kept", "exit 0");
        let home_wallet = names.cli_wallet.clone().unwrap();
        names.token_program = Some([5; 32]);
        use_zone(&mut names, &second);
        let zone_wallet = names.cli_wallet.clone().unwrap();
        assert_eq!(names.sequencer, ZONE_B);
        assert_eq!(std::path::PathBuf::from(&zone_wallet), zones::wallet_dir(&config_dir().unwrap(), &second));
        assert_eq!(std::fs::metadata(&zone_wallet).unwrap().permissions().mode() & 0o777, 0o700, "the folder is this user's alone");
        assert_eq!(names.token_program, None, "home's token program isn't this zone's");
        names.token_program = Some([9; 32]);

        // What is saved: home in the fields a version without zones reads, and the zones.
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(config_dir().unwrap().join("config.json")).unwrap()).unwrap();
        assert_eq!(saved["zone"], second.as_str(), "{saved}");
        let config = names.config();
        assert_eq!(
            (config["sequencer"].as_str(), config["cli_wallet"].as_str(), config["token_program"].as_str(), config["zone"].as_str()),
            (Some("http://127.0.0.1:9"), Some(home_wallet.as_str()), Some(registry::account_text(&[5; 32]).as_str()), Some(second.as_str())),
            "{config}"
        );
        assert_eq!(config["zones"][1]["token_program"], registry::account_text(&[9; 32]), "{config}");
        assert_eq!(config["zones"][1]["wallet"], Value::Null, "a zone's own folder isn't kept as a choice: {config}");

        // A restart reads it back and is on the same zone.
        let mut again = fresh();
        again.apply_saved(&config);
        assert_eq!((again.sequencer.as_str(), again.cli_wallet.as_deref(), again.token_program), (ZONE_B, Some(zone_wallet.as_str()), Some([9; 32])));
        assert_eq!(again.config(), config);
        // What the app is shown is the zone in use, with home's sequencer beside it;
        // another app is told which network, never what the person calls it.
        let shown = parsed(&again.get_config());
        assert_eq!((shown["sequencer"].as_str(), shown["names_sequencer"].as_str(), shown["cli_wallet"].as_str()), (Some(ZONE_B), Some("http://127.0.0.1:9"), Some(zone_wallet.as_str())), "{shown}");
        assert_eq!(shown["zone"], json!({ "id": second, "name": "Second", "channel": second, "home": false }), "{shown}");
        as_caller(app("evil_ui"));
        let other = parsed(&again.get_config());
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        assert_eq!(other["zone"], json!({ "id": second, "channel": second, "home": false }), "{other}");
        assert!(!other.to_string().contains("Second") && other.get("cli_wallet").is_none(), "{other}");

        // Back on home everything is home's again.
        use_zone(&mut again, "home");
        assert_eq!((again.sequencer.as_str(), again.cli_wallet.as_deref(), again.token_program), ("http://127.0.0.1:9", Some(home_wallet.as_str()), Some([5; 32])));
        assert_eq!(again.config()["zones"][1]["token_program"], registry::account_text(&[9; 32]));

        // A version without zones reads home; one before zones wrote nothing else.
        let mut old = config.clone();
        old.as_object_mut().unwrap().remove("zones");
        old.as_object_mut().unwrap().remove("zone");
        let mut before = fresh();
        before.apply_saved(&old);
        assert_eq!((before.sequencer.as_str(), before.cli_wallet.as_deref(), before.zones.len()), ("http://127.0.0.1:9", Some(home_wallet.as_str()), 1));

        // A zone whose chosen wallet folder is gone is not entered: home is, the zone stays.
        let mut gone = config.clone();
        gone["zones"][1]["wallet"] = json!(dir.join("unplugged").to_string_lossy());
        let mut unplugged = fresh();
        unplugged.apply_saved(&gone);
        assert_eq!((unplugged.sequencer.as_str(), unplugged.zones.current(), unplugged.zones.len()), ("http://127.0.0.1:9", 0, 2));
        let refused = parsed(&unplugged.zone_use(second.clone()));
        assert!(refused["error"].as_str().unwrap().contains("wallet folder") && refused["error"].as_str().unwrap().contains("can't be used now"), "{refused}");
        assert_eq!((unplugged.sequencer.as_str(), unplugged.cli_wallet.as_deref()), ("http://127.0.0.1:9", Some(home_wallet.as_str())), "nothing changed");
        zone_test_end(dir);
    }

    #[test]
    fn a_zones_wallet_folder_is_never_another_zones() {
        let (mut names, dir, second) = with_two_zones("zones-folder", "exit 0");
        let home_wallet = names.cli_wallet.clone().unwrap();
        use_zone(&mut names, &second);
        let own = names.cli_wallet.clone().unwrap();
        // Home's folder holds home's wallet.
        assert!(!names.set_cli_wallet(home_wallet.clone()));
        assert_eq!(names.cli_wallet.as_deref(), Some(own.as_str()));
        // Another folder of the person's is taken, and an empty choice goes back to its own.
        let other = dir.join("other-wallet");
        cli::private_dir(&other).unwrap();
        assert!(names.set_cli_wallet(other.to_string_lossy().into_owned()));
        assert_eq!(names.config()["zones"][1]["wallet"], json!(other.to_string_lossy()));
        assert!(names.set_cli_wallet(String::new()));
        assert_eq!(names.cli_wallet.as_deref(), Some(own.as_str()));
        // And home can't take the zone's.
        use_zone(&mut names, "home");
        assert!(!names.set_cli_wallet(own.clone()));
        assert_eq!(names.cli_wallet.as_deref(), Some(home_wallet.as_str()));
        zone_test_end(dir);
    }

    #[test]
    fn names_stay_on_home_while_another_zone_is_in_use() {
        let (mut names, dir, second) = with_two_zones("zones-names", "exit 0");
        let home_wallet = names.cli_wallet.clone().unwrap();
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        use_zone(&mut names, &second);
        // Reads about names go to home's sequencer with home's wallet; the rest to the zone's.
        let (for_names, for_money) = (names.names_setup().unwrap(), names.cli_setup().unwrap());
        assert_eq!((for_names.sequencer.as_str(), for_names.wallet.to_string_lossy().as_ref()), ("http://127.0.0.1:9", home_wallet.as_str()));
        assert_eq!((for_money.sequencer.as_str(), for_money.wallet.clone()), (ZONE_B, zones::wallet_dir(&config_dir().unwrap(), &second)));
        assert_eq!(names.home_sequencer(), "http://127.0.0.1:9");
        // A change to a name, or a payment to one, is refused before anything runs.
        for refused in [
            names.cli_set_text("jef".into(), "chat".into(), "hi".into()),
            names.cli_pay("jef".into(), "5".into()),
            names.cli_receive_privately("jef".into()),
        ] {
            let refused = parsed(&refused);
            assert_eq!(refused["error"], "names are kept on Home, and this wallet is using Second now. Switch to Home to do this. Nothing was started.", "{refused}");
        }
        assert_eq!(parsed(&names.cli_faucet())["error"], "the faucet pays on Home only. Switch to Home to ask it.");
        as_caller(app("tips_ui"));
        let to_name = parsed(&names.propose_transfer("LEZ".into(), "5".into(), "jef".into()));
        assert!(to_name["error"].as_str().unwrap().contains("a name can't be paid"), "{to_name}");
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "no job ran");
        assert!(activity::all(&config_dir().unwrap()).is_empty(), "nothing is in the history");
        // On home they run as before.
        use_zone(&mut names, "home");
        let started = parsed(&names.cli_set_text("jef".into(), "chat".into(), "hi".into()));
        let ended = job_end(&mut names, started["value"]["id"].as_str().expect("a job"));
        assert!(ended["log_tail"].as_str().unwrap().contains("[--rpc=http://127.0.0.1:9]\n[set-text]"), "{ended}");
        zone_test_end(dir);
    }

    #[test]
    fn a_sequencer_is_changed_within_its_zone_and_no_further() {
        let (mut names, dir, second) = with_two_zones("zones-sequencer", "exit 0");
        sequencer_says("https://b2.example", Ok(&"ab".repeat(32)));
        sequencer_says("https://c.example", Ok(&"cd".repeat(32)));
        sequencer_says("https://quiet.example", Err("it didn't answer in time"));
        // The same chain at another address: the zone moves there, and stays itself.
        let moved = parsed(&names.zone_sequencer(second.clone(), "https://b2.example".into(), false))["value"].clone();
        assert_eq!((moved["changed"].as_bool(), moved["channel"].as_str(), moved["zones"][1]["sequencer"].as_str(), moved["zones"][1]["id"].as_str()), (Some(true), Some(second.as_str()), Some("https://b2.example"), Some(second.as_str())), "{moved}");
        assert_eq!(names.sequencer, "http://127.0.0.1:9", "home is still in use");
        // Another chain, or one that doesn't say, is refused: also when the person insists,
        // since only home may become another chain.
        for insist in [false, true] {
            let other = parsed(&names.zone_sequencer(second.clone(), "https://c.example".into(), insist));
            assert!(other["error"].as_str().unwrap().contains("serves another chain (channel cdcdcdcd"), "{other}");
        }
        let quiet = parsed(&names.zone_sequencer(second.clone(), "https://quiet.example".into(), false));
        assert!(quiet["error"].as_str().unwrap().contains("didn't say which chain it serves"), "{quiet}");
        assert_eq!(parsed(&names.zone_sequencer(second.clone(), "ftp://x".into(), false))["error"], NOT_A_SEQUENCER);
        assert_eq!(parsed(&names.zone_list())["value"]["zones"][1]["sequencer"], "https://b2.example");

        // Home, with a wallet: another chain is told, and taken only when the person says so.
        assert!(!names.set_sequencer("https://c.example".into()));
        let asked = parsed(&names.zone_sequencer("home".into(), "https://c.example".into(), false));
        assert_eq!((asked["success"].as_bool(), asked["value"]["changed"].as_bool(), asked["value"]["other_chain"].as_bool(), asked["value"]["channel"].as_str()), (Some(true), Some(false), Some(true), Some("cd".repeat(32).as_str())), "{asked}");
        assert_eq!(names.sequencer, "http://127.0.0.1:9");
        // A sequencer that serves a zone already here is never home's.
        let taken = parsed(&names.zone_sequencer("home".into(), "https://b2.example".into(), true));
        assert!(taken["error"].as_str().unwrap().contains("serves Second, a zone already set up here"), "{taken}");
        assert!(!names.set_sequencer("https://quiet.example".into()), "home's chain is known, so a sequencer must say it");
        let said = parsed(&names.zone_sequencer("home".into(), "https://c.example".into(), true))["value"].clone();
        assert_eq!((said["changed"].as_bool(), said["zones"][0]["channel"].as_str(), said["zones"][0]["sequencer"].as_str()), (Some(true), Some("cd".repeat(32).as_str()), Some("https://c.example")), "{said}");
        assert_eq!(names.sequencer, "https://c.example");

        // The zone in use is the one `set_sequencer` changes, wallet and all untouched.
        use_zone(&mut names, &second);
        let wallet = names.cli_wallet.clone();
        sequencer_says("https://b3.example", Ok(&"ab".repeat(32)));
        assert!(names.set_sequencer("https://b3.example".into()));
        assert!(!names.set_sequencer("https://c.example".into()), "home's chain is not this zone's");
        assert_eq!((names.sequencer.as_str(), names.cli_wallet.clone()), ("https://b3.example", wallet));
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(config_dir().unwrap().join("config.json")).unwrap()).unwrap();
        assert_eq!((saved["sequencer"].as_str(), saved["zones"][1]["sequencer"].as_str(), saved["zone"].as_str()), (Some("https://c.example"), Some("https://b3.example"), Some(second.as_str())), "{saved}");
        zone_test_end(dir);
    }

    /// A home with no wallet yet has nothing read from a chain: its sequencer is set
    /// freely, as it was before there were zones.
    #[test]
    fn a_home_without_a_wallet_takes_any_sequencer() {
        let mut names = fresh();
        let empty = std::env::temp_dir().join(format!("logos-names-module-zones-empty-{}-{}", std::process::id(), now()));
        cli::private_dir(&empty).unwrap();
        names.cli_wallet = Some(empty.to_string_lossy().into_owned());
        sequencer_says(DEFAULT_SEQUENCER, Ok(&"01".repeat(32)));
        sequencer_says("https://other.example", Ok(&"03".repeat(32)));
        sequencer_says("https://quiet.example", Err("no answer"));
        assert!(names.set_sequencer("https://other.example".into()));
        assert_eq!(parsed(&names.zone_list())["value"]["zones"][0]["channel"], "03".repeat(32));
        // One that doesn't say is still refused once home's chain is known.
        assert!(!names.set_sequencer("https://quiet.example".into()));
        zone_test_end(empty);
    }

    /// The same keys make the same account ID on every zone, and what it holds there is
    /// other money: a balance read on one zone is never compared with another's.
    #[test]
    fn each_zone_has_its_own_balances_and_history() {
        let tail = format!(
            "case \"$*\" in\n*127.0.0.1:9*balance*) echo '  Private/{PRIVATE}  250  (default)' ;;\n*127.0.0.1:7*balance*) echo '  Private/{PRIVATE}  5  (default)' ;;\n*auth-transfer*) sleep 1 ;;\nesac"
        );
        let (mut names, dir, second) = with_two_zones("zones-history", &tail);
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "250");
        use_zone(&mut names, &second);
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "5", "the zone's own balance, not home's kept");
        use_zone(&mut names, "home");
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "250");
        use_zone(&mut names, &second);
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "5");
        let history = activity::all(&config_dir().unwrap());
        assert!(history.is_empty(), "245 was told as money that arrived: {history:?}");

        // What is sent on a zone says so, and a zone isn't left while a job runs on it.
        let paid = parsed(&names.cli_pay_account(format!("Public/{OTHER}"), "1000".into(), false));
        let job = paid["value"]["id"].as_str().expect("a job").to_owned();
        let busy = parsed(&names.zone_use("home".into()));
        assert!(busy["error"].as_str().unwrap().contains("to finish first"), "{busy}");
        assert_eq!(names.sequencer, ZONE_B);
        let ended = job_end(&mut names, &job);
        assert!(ended["log_tail"].as_str().unwrap().contains("[--rpc=http://127.0.0.1:7]"), "{ended}");
        let on_zone = parsed(&names.activity(format!("Public/{PUBLIC}")))["value"].clone();
        assert_eq!((on_zone[0]["kind"].as_str(), on_zone[0]["zone"].as_str(), on_zone[0]["zone_name"].as_str()), (Some("pay_account"), Some(second.as_str()), Some("Second")), "{on_zone}");
        // On home the account's page doesn't list it; the whole history does, by zone.
        use_zone(&mut names, "home");
        assert_eq!(parsed(&names.activity(format!("Public/{PUBLIC}")))["value"], json!([]));
        let all = parsed(&names.activity(String::new()))["value"].clone();
        assert_eq!((all.as_array().unwrap().len(), all[0]["zone_name"].as_str(), all[0]["state"].as_str()), (1, Some("Second"), Some("done")), "{all}");
        // A home entry says home once there is more than one zone.
        let dir_now = config_dir().unwrap();
        activity::add(&dir_now, now(), json!({ "kind": "pay_account", "chain": "lez", "from": format!("Public/{PUBLIC}"), "to": format!("Public/{OTHER}"), "amount": "7", "state": "done" }));
        let all = parsed(&names.activity(String::new()))["value"].clone();
        assert_eq!((all[0]["amount"].as_str(), all[0]["zone_name"].as_str(), all[0].get("zone").is_none()), (Some("7"), Some("Home"), true), "{all}");
        // A removed zone's entries stay, with no name to give.
        parsed(&names.zone_remove(second.clone()));
        let all = parsed(&names.activity(String::new()))["value"].clone();
        assert_eq!((all[1]["zone"].as_str(), all[1]["zone_name"].clone()), (Some(second.as_str()), Value::Null), "{all}");
        zone_test_end(dir);
    }

    /// Another app's transfer or call is for the zone in use when it asks, and is only
    /// approved there: on another zone the same account ID is another account.
    #[test]
    fn another_apps_request_is_approved_only_on_the_zone_it_was_asked_for() {
        let (mut names, dir, second) = with_two_zones("zones-provider", "exit 0");
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        let on_home = parsed(&names.propose_transfer(String::new(), "250".into(), format!("Public/{OTHER}")))["value"].clone();
        // The app is told the network, never what the person calls it.
        assert_eq!((on_home["zone"].as_str(), on_home["zone_channel"].as_str(), on_home.get("zone_name").is_none()), (Some("home"), Some("01".repeat(32).as_str()), true), "{on_home}");
        let home_id = on_home["request"].as_str().unwrap().to_owned();
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        use_zone(&mut names, &second);
        as_caller(app("tips_ui"));
        let on_second = parsed(&names.propose_transfer(String::new(), "300".into(), format!("Public/{OTHER}")))["value"].clone();
        assert_eq!((on_second["zone"].as_str(), on_second["zone_channel"].as_str()), (Some(second.as_str()), Some(second.as_str())), "{on_second}");
        let second_id = on_second["request"].as_str().unwrap().to_owned();
        assert_eq!(parsed(&names.request_status(home_id.clone()))["value"]["zone"], "home");

        // The prompt says which zone each was asked for and whether it is the one in use.
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        let pending = parsed(&names.pending_requests())["value"].clone();
        let shown = |id: &str| pending.as_array().unwrap().iter().find(|request| request["id"] == id).cloned().unwrap();
        assert_eq!((shown(&home_id)["zone_name"].as_str(), shown(&home_id)["zone_in_use"].as_bool()), (Some("Home"), Some(false)), "{pending}");
        assert_eq!((shown(&second_id)["zone_name"].as_str(), shown(&second_id)["zone_in_use"].as_bool()), (Some("Second"), Some(true)), "{pending}");
        // Approving home's on this zone is refused, and it stays to be approved there.
        let wrong = approve(&mut names, &home_id, &format!("Public/{PUBLIC}"));
        assert_eq!(wrong["error"], "this was asked for Home, and the wallet is using Second now. Switch to Home to approve it, or reject it.", "{wrong}");
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "nothing ran");
        assert_eq!(parsed(&names.request_status(home_id.clone()))["value"]["state"], "pending");
        // This zone's own runs on this zone's sequencer.
        let started = approve(&mut names, &second_id, &format!("Public/{PUBLIC}"));
        let ended = job_end(&mut names, started["value"]["job"].as_str().expect("a job"));
        assert!(ended["log_tail"].as_str().unwrap().contains(&format!("[--rpc=http://127.0.0.1:7]")) && ended["log_tail"].as_str().unwrap().contains("[--amount=300]"), "{ended}");

        // A request for a zone that was removed since can only be rejected.
        as_caller(app("tips_ui"));
        let orphan = parsed(&names.propose_transfer(String::new(), "400".into(), format!("Public/{OTHER}")))["value"]["request"].as_str().unwrap().to_owned();
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        use_zone(&mut names, "home");
        let started = approve(&mut names, &home_id, &format!("Public/{PUBLIC}"));
        let ended = job_end(&mut names, started["value"]["job"].as_str().expect("a job"));
        assert!(ended["log_tail"].as_str().unwrap().contains("[--rpc=http://127.0.0.1:9]") && ended["log_tail"].as_str().unwrap().contains("[--amount=250]"), "{ended}");
        let elsewhere = approve(&mut names, &orphan, &format!("Public/{PUBLIC}"));
        assert!(elsewhere["error"].as_str().unwrap().contains("this was asked for Second"), "{elsewhere}");
        parsed(&names.zone_remove(second.clone()));
        let removed = approve(&mut names, &orphan, &format!("Public/{PUBLIC}"));
        assert_eq!(removed["error"], "this was asked for a zone that has been removed since. Reject it, and the app can ask again.", "{removed}");
        let pending = parsed(&names.pending_requests())["value"].clone();
        let left = pending.as_array().unwrap().iter().find(|request| request["id"] == orphan.as_str()).cloned().unwrap();
        assert_eq!((left["zone_removed"].as_bool(), left["zone_name"].as_str(), left["zone_in_use"].as_bool()), (Some(true), Some("Second"), Some(false)), "{left}");
        assert_eq!(parsed(&names.reject_request(orphan))["success"], true);
        zone_test_end(dir);
    }

    /// A request asked for home is not approved once home has become another chain.
    #[test]
    fn a_request_is_not_approved_on_a_chain_home_became_since() {
        let (mut names, dir, _) = with_two_zones("zones-provider-chain", "exit 0");
        connect(&mut names, "tips_ui", &format!("Public/{PUBLIC}"));
        as_caller(app("tips_ui"));
        let id = parsed(&names.propose_transfer(String::new(), "250".into(), format!("Public/{OTHER}")))["value"]["request"].as_str().unwrap().to_owned();
        as_caller(logos_rust_sdk::LogosCaller::HostAnchor);
        sequencer_says("https://c.example", Ok(&"cd".repeat(32)));
        assert_eq!(parsed(&names.zone_sequencer("home".into(), "https://c.example".into(), true))["value"]["changed"], true);
        let refused = approve(&mut names, &id, &format!("Public/{PUBLIC}"));
        assert_eq!(refused["error"], "the chain this was asked for has changed since. Reject it, and the app can ask again.", "{refused}");
        assert!(std::fs::read_dir(dir.join("jobs")).map_or(true, |entries| entries.count() == 0), "nothing ran");
        zone_test_end(dir);
    }

    /// A deposit can't be undone: the plan names the zone it is for, and nothing is sent
    /// when the zone's sequencer names another chain than the zone's.
    #[test]
    fn a_bridge_names_its_zone_and_stops_when_the_sequencer_names_another_chain() {
        let mut names = fresh();
        let _ = std::fs::remove_dir_all(config_dir().unwrap());
        sequencer_says(DEFAULT_SEQUENCER, Ok(&"01".repeat(32)));
        node_answers("wallet_get_notes", json!({ "notes": [{ "id": "e9ac", "value": "1000000" }, { "id": "c1de", "value": "5000000" }] }));
        node_answers("channel_deposit_with_notes", json!("ac".repeat(32)));
        node_answers("get_channel_state", json!({ "configuration_threshold": 3 }));
        assert_eq!(parsed(&names.zone_list())["value"]["zones"][0]["channel"], "01".repeat(32));
        let plan = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{OTHER}"), "1000000".into(), false));
        assert_eq!((plan["value"]["zone"].as_str(), plan["value"]["zone_name"].as_str(), plan["value"]["channel"].as_str()), (Some("home"), Some("Home"), Some("01".repeat(32).as_str())), "{plan}");
        sequencer_says(DEFAULT_SEQUENCER, Ok(&"03".repeat(32)));
        for send in [false, true] {
            let refused = parsed(&names.chain_bridge(NODE_KEY.into(), format!("Public/{OTHER}"), "1000000".into(), send));
            assert_eq!(refused["error"], "Home's sequencer now names another chain (channel 03030303…0303, not 01010101…0101). Nothing was sent; check the zone's sequencer.", "{refused}");
        }
        assert!(!node_asked().contains(&"channel_deposit_with_notes".to_owned()), "nothing was deposited");
        let _ = std::fs::remove_dir_all(config_dir().unwrap());
    }

    /// Home's channel is learnt from its sequencer once, saved, and its balance marks
    /// are kept under it from then on: they survive a change of sequencer.
    #[test]
    fn homes_channel_is_learnt_once_and_its_marks_follow_it() {
        let tail = format!("case \"$*\" in\n*balance*) echo '  Private/{PRIVATE}  250  (default)' ;;\nesac");
        let (mut names, dir) = with_wallet("zones-home-channel", &tail);
        let config = config_dir().unwrap();
        let _ = std::fs::remove_dir_all(&config);
        sequencer_says("http://127.0.0.1:9", Err("no answer"));
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "250");
        let marks = || std::fs::read_to_string(config.join("activity-balances.json")).unwrap();
        assert!(marks().contains("lez@http://127.0.0.1:9:"), "{}", marks());
        // Not asked again on every read while it doesn't say.
        let asked = registry::CHANNELS_ASKED.with(std::cell::Cell::get);
        parsed(&names.cli_balance());
        assert_eq!(registry::CHANNELS_ASKED.with(std::cell::Cell::get), asked);
        // The zone list asks once more; then it is known, saved, and the marks are its.
        sequencer_says("http://127.0.0.1:9", Ok(&"01".repeat(32)));
        names.channel_asked = false;
        assert_eq!(parsed(&names.zone_list())["value"]["zones"][0]["channel"], "01".repeat(32));
        assert!(marks().contains(&format!("lez@{}:", "01".repeat(32))) && !marks().contains("127.0.0.1:9"), "{}", marks());
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(config.join("config.json")).unwrap()).unwrap();
        assert_eq!(saved["zones"][0]["channel"], "01".repeat(32), "{saved}");
        // The same chain at another address: nothing arrives by the move.
        sequencer_says("http://127.0.0.1:5", Ok(&"01".repeat(32)));
        assert!(names.set_sequencer("http://127.0.0.1:5".into()));
        assert_eq!(parsed(&names.cli_balance())["value"]["balance"], "250");
        assert!(activity::all(&config).is_empty(), "{:?}", activity::all(&config));
        zone_test_end(dir);
    }
}
