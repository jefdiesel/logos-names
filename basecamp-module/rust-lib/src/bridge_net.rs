//! The bridge test network (LEZ 0.4), read only: what the person's public accounts hold there,
//! beside the public Logos testnet (LEZ 0.3) the rest of the Wallet uses.
//!
//! The bridge test network is the RFP-022 bridge's own chain (`RPC`), reset now and then. Its
//! reads follow the bridge repository's `public-demo/WALLET-READ.md`:
//! - one `getAccount` of an account gives its native balance (the actor state under the native
//!   token program, a u128) and its tokens (the actor state under the token program: borsh
//!   `Holdings`, a map of definition to u128). There is no separate holding account;
//! - the bridged tokens are found in the bridge's Config (a PDA of `BRIDGE_PROGRAM`), which names
//!   the token program, each token's definition and its decimals. A holding whose definition the
//!   Config doesn't list is not a bridged token, whatever its name says;
//! - the chain is known by its channel (`getChannelId`): a new channel, a lower height on the same
//!   channel, or the Config gone means the chain was reset, and nothing read before applies.
//!
//! Nothing here signs or sends. Private accounts are not read (that needs the wallet's LEZ 0.4
//! viewing keys and LEZ 0.4 code). Every read runs on its own thread, at most every `EVERY`.

use crate::registry::{self, AccountId, Deadline};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

// ── The one place to change when the network moves (rc3) ─────────────────

/// The bridge test network's JSON-RPC endpoint (POST only, behind a filter): the default; the
/// read uses LEZ 0.4's configured sequencer (config.json `lez04.sequencer`), the same chain.
pub const RPC: &str = "https://lez.nameswallet.app/";
/// The bridge program, by default. Its Config lists the bridged tokens, so their definitions
/// (new at every re-genesis) are never written here. A redeployed or second bridge has a new
/// id: the bridges are a setting, config.json `lez04.bridge_programs`, changed with the
/// module's `set_bridge_programs` (no rebuild).
pub const DEFAULT_BRIDGE_PROGRAM: &str = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7";
/// The fixed bridge (v3), deployed beside v2 on 2026-11-14.
pub const BRIDGE_V3_PROGRAM: &str = "4eaeK3yiqATXhpCEYEFMMtjUn1bS7HLxPwfrH31mXCvr";
/// The bridge of the vault that releases by proof (v4), deployed 2026-11-15: the bridge module's
/// default deployment since 0343cb0.
pub const BRIDGE_V4_PROGRAM: &str = "BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf";
/// The bridge of the vault with no release delay (v5), deployed 2026-11-15 evening: the one the
/// wallet uses by default from then on. Same token program as v3 and v4.
pub const BRIDGE_V5_PROGRAM: &str = "CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA";
/// The bridge of vault v6 (2026-11-18): deposit roots from the RFP-021 light client, releases by
/// proof (vault 0x95cea20e…427e). Same token program as v3 to v5, its own wrapped definitions.
pub const BRIDGE_V6_PROGRAM: &str = "4PxaLAfPZDvpfZaR8SsC2Y7pYMPNwPPGVGMpjEVPQWed";
/// The bridge of vault v7 (2026-11-18): v6's way in, and out only through a 2-of-3 signer gate,
/// releases by proof (vault 0x4d7b360c…d23e). Same token program, its own wrapped definitions.
pub const BRIDGE_V7_PROGRAM: &str = "CdijfhGnf1NjyCvYTZcGijQZzZ6soNJQ75Piex1ZTM93";
/// The bridges read when none is set: v2 to v7 (MAX_BRIDGES).
pub const DEFAULT_BRIDGE_PROGRAMS: [&str; 6] = [DEFAULT_BRIDGE_PROGRAM, BRIDGE_V3_PROGRAM, BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM];
/// What each known bridge is called on a balance row, by its program: the names of the
/// deployments (rfp-022's v2 to v7). Another bridge shows its short program ID.
pub const BRIDGE_LABELS: [(&str, &str); 6] = [
    (DEFAULT_BRIDGE_PROGRAM, "bridge v2"),
    (BRIDGE_V3_PROGRAM, "bridge v3"),
    (BRIDGE_V4_PROGRAM, "bridge v4"),
    (BRIDGE_V5_PROGRAM, "bridge v5"),
    (BRIDGE_V6_PROGRAM, "bridge v6"),
    (BRIDGE_V7_PROGRAM, "bridge v7"),
];

/// The label of the bridge `program` (`BRIDGE_LABELS`), or None.
pub fn bridge_label(program: &str) -> Option<&'static str> {
    BRIDGE_LABELS.iter().find(|(id, _)| *id == program).map(|(_, label)| *label)
}
/// Token programs the Wallet always refuses as a relayed call's root, whatever a sequencer says:
/// the default bridge's (read from its Config on our 0.4 on 2026-11-14), so a sequencer answering
/// "no Config" can't take it off the list. Every token program read from a bridge's Config since
/// is kept too (`SEEN_TOKEN_PROGRAMS`, append-only).
pub const KNOWN_TOKEN_PROGRAMS: [&str; 2] = [
    // v2's (Config HugSoWM7…, read 2026-11-14)
    "3gF78HRJNCfabkWFiszahVVacq181aU8SSGua8Cj9ZE4",
    // v3's (Config 7nQ38yMp…, read 2026-11-14)
    "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7",
];
/// The file, beside config.json, that keeps every bridge token program ever read (append-only).
/// To take a wrong one out, delete the file: the next check reads every bridge again.
pub const SEEN_TOKEN_PROGRAMS: &str = "bridge-token-programs.json";
/// The most token programs kept there; past it, new ones are still refused when read, just not
/// kept.
pub const MAX_SEEN_TOKEN_PROGRAMS: usize = 16;

/// The token programs kept in `dir` (`SEEN_TOKEN_PROGRAMS`), canonical; none when there is no
/// file yet. An error when the file is there but can't be read as a list of program IDs: the
/// caller then signs no relayed call (it can't tell what the file would have refused), and the
/// file is left as it is for the person to look at; deleting it is the reset.
pub fn seen_token_programs(dir: &Path) -> Result<Vec<String>, String> {
    let path = dir.join(SEEN_TOKEN_PROGRAMS);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("can't read {} ({error})", path.display())),
    };
    let list = serde_json::from_str::<Vec<String>>(&text).map_err(|_| format!("{} isn't a list of program IDs", path.display()))?;
    list.iter()
        .map(|id| registry::parse_account(id).map(|id| registry::account_text(&id)).map_err(|_| format!("{} holds something that isn't a program ID", path.display())))
        .collect()
}

/// Adds token programs to the ones kept in `dir`; never removes one, and never writes over a
/// file it can't read.
pub fn keep_token_programs(dir: &Path, programs: &[String]) {
    // Background reads keep what they read too: one writer at a time, so none is lost.
    static ONE_WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = ONE_WRITER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Ok(mut kept) = seen_token_programs(dir) else { return };
    let before = kept.len();
    for program in programs {
        if !kept.contains(program) && kept.len() < MAX_SEEN_TOKEN_PROGRAMS {
            kept.push(program.clone());
        }
    }
    if kept.len() != before {
        let _ = std::fs::create_dir_all(dir);
        let _ = crate::cli::write_private(&dir.join(SEEN_TOKEN_PROGRAMS), Value::from(kept).to_string().as_bytes());
    }
}

/// The most bridges the Wallet knows: each is one Config read (plus its tokens' definitions,
/// once an hour) in every balance read and every relayed-call check. Six since v6 and v7: a read
/// of MAX_ACCOUNTS accounts, the six Configs and the chain's two stays under the edge's 50
/// requests per 10 s per IP.
pub const MAX_BRIDGES: usize = 6;
/// Short symbols of the bridged tokens, by their Ethereum token address (the same on every
/// re-genesis). A token the Config lists without a symbol here shows its address.
pub const SYMBOLS: [(&str, &str); 2] = [
    ("0x0000000000000000000000000000000000000000", "wETH"),
    ("0xac207b9bfebf3ef7c26297ea6abadeeccc4aa71e", "dUSDC"),
];

// ─────────────────────────────────────────────────────────────────────────

/// What the network is called everywhere in the Wallet.
pub const NAME: &str = "bridge test network";
/// Its LEZ version, shown beside the name.
pub const VERSION: &str = "LEZ 0.4";
/// The `chain` of its History entries ("lez" is the public Logos testnet's).
pub const CHAIN: &str = "lez-0.4";
/// The `zone` its History entries and balance marks are kept under, so no LEZ 0.3 entry is
/// ever counted against a balance here (`activity::touching`).
pub const ZONE: &str = "bridge-0.4";
/// How often it is read at most (WALLET-READ.md, polling rule).
pub const EVERY: Duration = Duration::from_secs(30);
/// The most accounts one read asks about: one request each, a short pause apart, so a read of
/// all of them (plus the chain's 2, up to MAX_BRIDGES Configs and, once an hour, their tokens'
/// definitions) stays under the edge's 50 requests per 10 s per IP.
pub const MAX_ACCOUNTS: usize = 24;
/// The pause between two account reads.
const ACCOUNT_PAUSE: Duration = Duration::from_millis(120);
/// The most token definitions whose names one read asks for (each is one request).
const NAMES_PER_READ: usize = 4;
/// How long the token list is kept before the Config is read again (seconds).
const TOKENS_FRESH_S: u64 = 3600;
/// How long a reset is told after it was seen (seconds).
const RESET_TOLD_S: u64 = 24 * 3600;
/// One read's whole budget.
const READ_BUDGET: Duration = Duration::from_secs(25);

/// The public PDA prefix (LEZ `for_public_pda`, the same in rc1 and rc3) and the bridge
/// Config's seed (`lez-bridge/bridge/src/lib.rs`, `CONFIG_SEED`).
const PDA_PREFIX: &[u8; 32] = b"/LEE/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00\x00";
const CONFIG_SEED: &[u8; 32] = b"/RFP022/bridge/config/v1/000000/";

/// The `read_at` of the last read whose balances went to the History, so each goes once.
pub static LAST_NOTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What the state file next to the History remembers between reads and runs.
const STATE_FILE: &str = "bridge-net.json";
/// The accounts the person asked to watch here, besides the wallet's own.
const WATCH_FILE: &str = "bridge-net-watch.json";
/// The most accounts watched.
pub const MAX_WATCHED: usize = 8;

/// The accounts watched on the bridge test network, `Public/<id>`, in the order added.
pub fn watched(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join(WATCH_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|text| public_account(&text).ok().map(|id| format!("Public/{}", registry::account_text(&id))))
        .take(MAX_WATCHED)
        .collect()
}

/// Adds (`on`) or removes a public account from the watched ones; gives the list. Any public
/// account ID can be watched: one the network has never seen reads as nothing there.
pub fn watch(dir: &Path, text: &str, on: bool) -> Result<Vec<String>, String> {
    let account = format!("Public/{}", registry::account_text(&public_account(text)?));
    let mut list = watched(dir);
    list.retain(|item| *item != account);
    if on {
        if list.len() >= MAX_WATCHED {
            return Err(format!("at most {MAX_WATCHED} accounts can be watched; stop watching one first"));
        }
        list.push(account);
    }
    std::fs::create_dir_all(dir).map_err(|error| format!("can't keep the list: {error}"))?;
    crate::cli::write_private(&dir.join(WATCH_FILE), Value::from(list.clone()).to_string().as_bytes()).map_err(|error| format!("can't keep the list: {error}"))?;
    Ok(list)
}

/// The Config's address: `SHA256(PDA prefix ‖ program ‖ seed)`.
pub fn config_address(program: &AccountId) -> AccountId {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(PDA_PREFIX);
    hash.update(program);
    hash.update(CONFIG_SEED);
    hash.finalize().into()
}

/// Borsh bytes, read front to back; any short read fails.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|end| *end <= self.bytes.len()).ok_or("the bytes end too soon")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn flag(&mut self) -> Result<bool, String> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("a flag that is neither 0 nor 1".to_owned()),
        }
    }
    fn u32(&mut self) -> Result<usize, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes")) as usize)
    }
    fn u128(&mut self) -> Result<u128, String> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().expect("16 bytes")))
    }
    fn id(&mut self) -> Result<AccountId, String> {
        Ok(self.take(32)?.try_into().expect("32 bytes"))
    }
    fn option_id(&mut self) -> Result<Option<AccountId>, String> {
        Ok(if self.flag()? { Some(self.id()?) } else { None })
    }
    /// Skips a vec of `size`-byte items.
    fn skip_vec(&mut self, size: usize) -> Result<(), String> {
        let count = self.u32()?;
        self.take(count.checked_mul(size).ok_or("a vec too long")?)?;
        Ok(())
    }
    fn end(&self) -> Result<(), String> {
        if self.at == self.bytes.len() { Ok(()) } else { Err("bytes left over".to_owned()) }
    }
}

/// One bridged token, as the Config lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub eth_token: String,
    pub definition: AccountId,
    pub decimals: u8,
    /// The amounts a claim or a burn may be, base units, in the Config's order.
    pub denoms: Vec<u128>,
    pub registered: bool,
    pub burn_frozen: bool,
}

/// A bridge Config as far as the Wallet reads it: its token program, whether burns are frozen
/// for every token, and the tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeConfig {
    pub token_program: AccountId,
    pub burn_frozen: bool,
    pub tokens: Vec<Token>,
}

impl Token {
    pub fn symbol(&self) -> String {
        SYMBOLS.iter().find(|(eth, _)| *eth == self.eth_token).map_or_else(|| self.eth_token.clone(), |(_, symbol)| (*symbol).to_owned())
    }
}

/// The bridge Config's token program and tokens (`Config` and `TokenEntry` in
/// `lez-bridge/bridge/src/lib.rs`). Fails on anything that isn't exactly one Config.
pub fn decode_config(bytes: &[u8]) -> Result<(AccountId, Vec<Token>), String> {
    decode_bridge_config(bytes).map(|config| (config.token_program, config.tokens))
}

/// What binds a Config to its vault: its `cfg_id`, the vault and the Ethereum chain id (the first
/// 60 bytes, borsh: 32, 20, u64 little-endian). A burn's opening must name these (burn_opening.rs).
pub fn decode_config_identity(bytes: &[u8]) -> Result<([u8; 32], [u8; 20], u64), String> {
    let mut r = Reader::new(bytes);
    let cfg: [u8; 32] = r.take(32)?.try_into().expect("32 bytes");
    let vault: [u8; 20] = r.take(20)?.try_into().expect("20 bytes");
    let chain = u64::from_le_bytes(r.take(8)?.try_into().expect("8 bytes"));
    // The whole Config must read, or none of it is taken.
    decode_bridge_config(bytes)?;
    Ok((cfg, vault, chain))
}

/// The same Config, with the burn freeze and each token's denominations and flags.
pub fn decode_bridge_config(bytes: &[u8]) -> Result<BridgeConfig, String> {
    let mut r = Reader::new(bytes);
    r.take(32 + 20)?; // cfg_id, vault
    r.take(8)?; // evm_chain_id
    r.take(32 + 32)?; // lez_chain_id, zone_id
    let token_program = r.id()?;
    r.id()?; // admin
    r.option_id()?; // pending_admin
    r.id()?; // freezer
    r.id()?; // attestor
    r.flag()?; // mint_frozen
    let all_burns_frozen = r.flag()?;
    let count = r.u32()?;
    if count > 32 {
        return Err("more tokens than a Config holds".to_owned());
    }
    let mut tokens = Vec::with_capacity(count);
    for _ in 0..count {
        let eth_token = format!("0x{}", hex(r.take(20)?));
        let definition = r.id()?;
        let decimals = r.u8()?;
        let denom_count = r.u32()?;
        if denom_count > 16 {
            return Err("more denominations than a Config holds".to_owned());
        }
        let mut denoms = Vec::with_capacity(denom_count);
        for _ in 0..denom_count {
            denoms.push(r.u128()?);
        }
        r.skip_vec(8)?; // claims
        r.skip_vec(8)?; // burns
        r.u128()?; // claimed_value
        r.u128()?; // burned_value
        let registered = r.flag()?;
        r.flag()?; // mint_frozen
        let burn_frozen = r.flag()?;
        if decimals > 38 {
            return Err("a token with more decimals than any amount has digits".to_owned());
        }
        tokens.push(Token { eth_token, definition, decimals, denoms, registered, burn_frozen });
    }
    match r.u8()? {
        0 => {}
        1 => {
            r.take(32 + 32 + 8)?;
        }
        _ => return Err("a root source this version doesn't know".to_owned()),
    }
    r.u128()?; // sponsor_amount
    r.end()?;
    Ok(BridgeConfig { token_program, burn_frozen: all_burns_frozen, tokens })
}

// ── Burns to a bridge (Send back to Ethereum) ──────────────────────────────

/// The bridges a relayed burn may go to: the ones whose vault releases by proof (v4 to v7). Built
/// in, not a setting: v2 and v3 share their token program, and a burn to them destroys the tokens
/// for good (their vaults can never release). Never `set_bridge_programs`. All four share one
/// token program, so a burn is held to the bridge it notifies: its definition must be one that
/// bridge's own Config lists (`provider_public::burn_check`), and every bridge has its own.
pub const BURN_BRIDGES: [&str; 4] = [BRIDGE_V4_PROGRAM, BRIDGE_V5_PROGRAM, BRIDGE_V6_PROGRAM, BRIDGE_V7_PROGRAM];

/// The bridge Config's seed, as a relayed call's program account names it (`seed`, hex).
pub fn config_seed_hex() -> String {
    hex(CONFIG_SEED)
}

/// `Message::BurnWithNotice` of the bridge's token program (`lez-bridge/token/src/lib.rs`), borsh:
/// variant 10, then the definition, the amount (u128, little-endian), the notified actor (its
/// account, then its program) and the 32-byte memo (the exit commitment E). 145 bytes.
pub const BURN_WITH_NOTICE_TAG: u8 = 10;
pub const BURN_MESSAGE_LEN: usize = 1 + 32 + 16 + 32 + 32 + 32;

/// A decoded `BurnWithNotice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurnMessage {
    pub definition: AccountId,
    pub amount: u128,
    pub notify_account: AccountId,
    pub notify_program: AccountId,
    pub memo: [u8; 32],
}

/// Decodes `bytes` as exactly one `BurnWithNotice`: the tag, the exact length, nothing left over.
pub fn decode_burn(bytes: &[u8]) -> Result<BurnMessage, String> {
    if bytes.len() != BURN_MESSAGE_LEN {
        return Err(format!("a burn to a bridge is {BURN_MESSAGE_LEN} bytes; this message is {}", bytes.len()));
    }
    let mut r = Reader::new(bytes);
    if r.u8()? != BURN_WITH_NOTICE_TAG {
        return Err("the message isn't a burn with notice".to_owned());
    }
    let definition = r.id()?;
    let amount = r.u128()?;
    let notify_account = r.id()?;
    let notify_program = r.id()?;
    let memo = r.id()?;
    r.end()?;
    Ok(BurnMessage { definition, amount, notify_account, notify_program, memo })
}

/// The bytes of a `BurnWithNotice` (tests and the shared vector).
#[cfg(test)]
pub fn encode_burn(burn: &BurnMessage) -> Vec<u8> {
    let mut out = Vec::with_capacity(BURN_MESSAGE_LEN);
    out.push(BURN_WITH_NOTICE_TAG);
    out.extend_from_slice(&burn.definition);
    out.extend_from_slice(&burn.amount.to_le_bytes());
    out.extend_from_slice(&burn.notify_account);
    out.extend_from_slice(&burn.notify_program);
    out.extend_from_slice(&burn.memo);
    out
}

/// The whole Config of the bridge `program` on the chain at `url`, read at most `max_age` ago
/// (`config_state`), or `None` when the chain has none. Any failed read is an error (fail
/// closed), as in `token_program_of`.
pub fn config_of(url: &str, program: &AccountId, max_age: Duration) -> Result<Option<BridgeConfig>, String> {
    match config_state(url, program, max_age)? {
        None => Ok(None),
        Some(bytes) => decode_bridge_config(&bytes).map(Some),
    }
}

// ── Each bridge's Config, kept: one read serves the balances, the sheets and the signing ──
//
// A bridge's Config (its token program, its tokens, whether burns are paused) is one account,
// and three paths used to read it on their own: the balance read, the relayed-call sheet (every
// 30 s, for each bridge, twice: its token program and, for a burn bridge, the whole Config) and
// each approval. Now one read is kept per endpoint and bridge, and each path says how old it may
// be: an hour for what a balance shows and which programs a call may not have as its root
// (`CONFIG_FRESH`), five minutes for a burn's sheet (`BURN_SHEET_FRESH`), a minute for signing a
// burn (`BURN_SIGN_FRESH`).

/// How old a kept Config may be for showing tokens and for the root-only list.
pub const CONFIG_FRESH: Duration = Duration::from_secs(3600);
/// How old the target bridge's Config may be for a burn's sheet.
pub const BURN_SHEET_FRESH: Duration = Duration::from_secs(300);
/// How old the target bridge's Config may be when a burn is approved, signed or proposed: a
/// pause of burns (`burn_frozen`) set on the chain is seen within this.
pub const BURN_SIGN_FRESH: Duration = Duration::from_secs(60);
/// One Config read's wait.
const CONFIG_WAIT: Duration = Duration::from_secs(8);

#[derive(Default)]
struct KeptConfig {
    /// The last good read: when, and the state bytes (None: no Config on that chain).
    good: Option<(std::time::Instant, Option<Vec<u8>>)>,
    /// The last failed read: when, and why.
    failed: Option<(std::time::Instant, String)>,
    reading: bool,
}

type ConfigKey = (String, AccountId);

fn configs() -> std::sync::MutexGuard<'static, Vec<(ConfigKey, KeptConfig)>> {
    static CONFIGS: std::sync::Mutex<Vec<(ConfigKey, KeptConfig)>> = std::sync::Mutex::new(Vec::new());
    CONFIGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn config_read_ended() -> &'static std::sync::Condvar {
    static ENDED: std::sync::Condvar = std::sync::Condvar::new();
    &ENDED
}

fn kept_config<'a>(kept: &'a mut Vec<(ConfigKey, KeptConfig)>, url: &str, program: &AccountId) -> &'a mut KeptConfig {
    let key = (url.to_owned(), *program);
    let at = match kept.iter().position(|(known, _)| *known == key) {
        Some(at) => at,
        None => {
            kept.push((key, KeptConfig::default()));
            kept.len() - 1
        }
    };
    &mut kept[at].1
}

/// Reads bridge `program`'s Config account on `url` now (one `getAccount`) and keeps a good read.
/// Reads bridge `program`'s Config account on `url` now (one `getAccount`), for the one read of it
/// under way (`reading`, set by the caller): keeps a good read, then lets the others waiting for
/// it go on.
fn read_config_now(url: &str, program: &AccountId, deadline: Deadline) -> Result<Option<Vec<u8>>, String> {
    let deadline = deadline.min_with(CONFIG_WAIT);
    let read = call(url, "getAccount", json!([registry::account_text(&config_address(program))]), deadline)
        .and_then(|config| if config.is_null() { Ok(None) } else { state_under(&config, program).map(|bytes| (!bytes.is_empty()).then_some(bytes)) });
    let mut kept = configs();
    let entry = kept_config(&mut kept, url, program);
    match &read {
        Ok(state) => entry.good = Some((std::time::Instant::now(), state.clone())),
        Err(why) => entry.failed = Some((std::time::Instant::now(), why.clone())),
    }
    entry.reading = false;
    drop(kept);
    config_read_ended().notify_all();
    read
}

/// The state bytes of bridge `program`'s Config on `url` (None: the chain has no Config there),
/// read at most `max_age` ago: what was kept, else read now. Blocks for the read: for background
/// reads (and tests) only; a call's own thread uses `config_within`.
pub fn config_state(url: &str, program: &AccountId, max_age: Duration) -> Result<Option<Vec<u8>>, String> {
    config_state_by(url, program, max_age, Deadline::after(CONFIG_WAIT))
}

/// `config_state` within a read's `deadline` (a balance read's, all its requests together). One
/// read of a Config at a time: a second reader waits for the one under way and takes its answer.
fn config_state_by(url: &str, program: &AccountId, max_age: Duration, deadline: Deadline) -> Result<Option<Vec<u8>>, String> {
    let asked = std::time::Instant::now();
    let mut kept = configs();
    loop {
        let entry = kept_config(&mut kept, url, program);
        if let Some((at, state)) = &entry.good {
            if at.elapsed() <= max_age {
                return Ok(state.clone());
            }
        }
        if !entry.reading {
            // The read another reader waited for failed: its answer is this one's too.
            if let Some((at, why)) = &entry.failed {
                if *at >= asked {
                    return Err(why.clone());
                }
            }
            entry.reading = true;
            break;
        }
        let left = deadline.left();
        if left.is_zero() {
            return Err(problem(deadline.too_slow()));
        }
        kept = config_read_ended().wait_timeout(kept, left).unwrap_or_else(std::sync::PoisonError::into_inner).0;
    }
    drop(kept);
    read_config_now(url, program, deadline)
}

/// What the wallet knows of a bridge's Config at a call (`config_within`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigNow {
    /// Read within the age asked for: the state bytes, None when the chain has no Config.
    Read(Option<Vec<u8>>),
    /// The last read failed and nothing young enough is kept: why.
    Unread(String),
    /// Being read; nothing young enough is kept yet.
    Reading,
}

/// Bridge `program`'s Config on `url` as a call may use it, never waiting on the network for more
/// than `wait`: a read at most `max_age` old, else a read started on its own thread and waited
/// for `wait` at most.
pub fn config_within(url: &str, program: &AccountId, max_age: Duration, wait: Duration) -> ConfigNow {
    let asked = std::time::Instant::now();
    let mut kept = configs();
    {
        let entry = kept_config(&mut kept, url, program);
        if let Some((at, state)) = &entry.good {
            if at.elapsed() <= max_age {
                return ConfigNow::Read(state.clone());
            }
        }
        if !entry.reading {
            entry.reading = true;
            let (url, program) = (url.to_owned(), *program);
            crate::served::spawn(move || {
                let _ = read_config_now(&url, &program, Deadline::after(CONFIG_WAIT));
            });
        }
    }
    let until = asked + wait;
    loop {
        let entry = kept_config(&mut kept, url, program);
        if let Some((at, state)) = &entry.good {
            if at.elapsed() <= max_age {
                return ConfigNow::Read(state.clone());
            }
        }
        if !entry.reading {
            return match &entry.failed {
                Some((at, why)) if *at >= asked || entry.good.is_none() => ConfigNow::Unread(why.clone()),
                _ => ConfigNow::Reading,
            };
        }
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return ConfigNow::Reading;
        }
        kept = config_read_ended().wait_timeout(kept, left).unwrap_or_else(std::sync::PoisonError::into_inner).0;
    }
}

/// Forgets every kept Config of `url` (the chain there was reset).
pub fn forget_configs(url: &str) {
    for ((known, _), entry) in configs().iter_mut() {
        if known == url {
            entry.good = None;
            entry.failed = None;
        }
    }
}

/// The Config bytes of the bridge `program` in a `getAccount` reply (tests' fixtures).
#[cfg(test)]
pub fn config_bytes_in(reply: &Value, program: &AccountId) -> Result<Vec<u8>, String> {
    state_under(reply, program)
}

/// A token definition's own name (`TokenState::Definition(Fungible { name, .. })`), cleaned
/// for showing, or `None` for bytes that aren't one.
pub fn decode_definition_name(bytes: &[u8]) -> Option<String> {
    let mut r = Reader::new(bytes);
    if r.u8().ok()? != 1 || r.u8().ok()? != 0 {
        return None;
    }
    let length = r.u32().ok()?;
    if length > 64 {
        return None;
    }
    let name = std::str::from_utf8(r.take(length).ok()?).ok()?;
    let cleaned = crate::text::clean_remote(name, 64);
    (!cleaned.is_empty()).then_some(cleaned)
}

/// What an account holds under the token program (`TokenState::Holdings`): amount by
/// definition. Empty bytes hold nothing; a definition's own state (tag 1) holds nothing.
pub fn decode_holdings(bytes: &[u8]) -> Result<BTreeMap<AccountId, u128>, String> {
    let mut held = BTreeMap::new();
    if bytes.is_empty() {
        return Ok(held);
    }
    let mut r = Reader::new(bytes);
    match r.u8()? {
        0 => {}
        1 => return Ok(held),
        _ => return Err("token state this version doesn't know".to_owned()),
    }
    let count = r.u32()?;
    if count > 32 {
        return Err("more token kinds than an account holds".to_owned());
    }
    for _ in 0..count {
        let definition = r.id()?;
        held.insert(definition, r.u128()?);
    }
    r.end()?;
    Ok(held)
}

/// An actor state as the chain gives it: a JSON array of bytes.
fn bytes_of(value: &Value) -> Result<Vec<u8>, String> {
    let Some(items) = value.as_array() else {
        return if value.is_null() { Ok(Vec::new()) } else { Err("an actor state that isn't a list of bytes".to_owned()) };
    };
    items.iter().map(|item| item.as_u64().and_then(|byte| u8::try_from(byte).ok()).ok_or_else(|| "an actor state that isn't a list of bytes".to_owned())).collect()
}

/// The bytes under `program` in a `getAccount` result (empty when it has none there).
fn state_under(account: &Value, program: &AccountId) -> Result<Vec<u8>, String> {
    let states = &account["data"]["actor_states"];
    if !states.is_object() {
        return Err("an account reply without actor states".to_owned());
    }
    bytes_of(&states[registry::account_text(program).as_str()])
}

/// The native balance in a `getAccount` result: 16 bytes, little-endian (none: 0).
pub fn native_of(account: &Value) -> Result<u128, String> {
    let bytes = state_under(account, &registry::NATIVE_TOKEN)?;
    if bytes.is_empty() {
        return Ok(0);
    }
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| "a native balance that isn't 16 bytes".to_owned())?;
    Ok(u128::from_le_bytes(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A public account the person typed or the wallet lists, as `Public/<id>`: an optional
/// `Public/` and an ID of exactly 32 bytes. A private account is refused: it can't be read here.
pub fn public_account(text: &str) -> Result<AccountId, String> {
    let text = text.trim();
    if text.starts_with("Private/") {
        return Err("a private account can't be read on the bridge test network yet".to_owned());
    }
    registry::bare_account(text.strip_prefix("Public/").unwrap_or(text)).map_err(|_| "that isn't a public account ID".to_owned())
}

/// The accounts a `bridge_net_balances` call names: a JSON list of `Public/<id>` (or bare IDs),
/// in order, without repeats. Private ones are counted and left out. The caller keeps at most
/// `MAX_ACCOUNTS` of them and says how many it left out.
pub fn accounts_asked(text: &str) -> Result<(Vec<AccountId>, usize), String> {
    let list: Vec<String> = match text.trim() {
        "" => Vec::new(),
        text => serde_json::from_str(text).map_err(|_| "give the accounts as a JSON list of Public/<id>".to_owned())?,
    };
    let (mut accounts, mut private) = (Vec::new(), 0);
    for item in &list {
        if item.trim().starts_with("Private/") {
            private += 1;
            continue;
        }
        let account = public_account(item)?;
        if !accounts.contains(&account) {
            accounts.push(account);
        }
    }
    Ok((accounts, private))
}

/// A failed request, said about this network rather than "the sequencer".
fn problem(why: String) -> String {
    let why = why.replace("could not reach the sequencer: ", "").replace("the sequencer", "it");
    format!("can't reach the bridge test network: {why}")
}

fn call(url: &str, method: &str, params: Value, deadline: Deadline) -> Result<Value, String> {
    registry::rpc(url, method, params, deadline).map_err(problem)
}

/// The token program the bridge `program`'s Config names on the chain at `url`: where the
/// person's bridged tokens sit. `None` when the chain has no Config for that bridge (then it
/// holds no bridged tokens). Any failed read is an error, never `None`: a caller that must not
/// sign a call to it can't take "unknown" for "none" (fail closed).
/// Read at most `max_age` ago (`config_state`).
pub fn token_program_of(url: &str, program: &AccountId, max_age: Duration) -> Result<Option<AccountId>, String> {
    match config_state(url, program, max_age)? {
        None => Ok(None),
        Some(bytes) => decode_config(&bytes).map(|(token_program, _)| Some(token_program)),
    }
}

// ── The test-LEZ faucet on the bridge test network's endpoint (bridge_faucetInfo, bridge_faucet) ──

/// How long a faucet call may take: the faucet answers once its transfer is in a block.
pub const FAUCET_WAIT: Duration = Duration::from_secs(120);
/// How long the faucet's info may take: it answers from its own state.
pub const FAUCET_INFO_WAIT: Duration = Duration::from_secs(10);

/// One faucet call on `url` (JSON-RPC POST). The endpoint's own refusals come back as a result
/// `{ok: false, code, reason, retry}`; its HTTP 429 is said the same way (`rate_limited`), and a
/// 502 or 503 as `busy` with `retry: true`. Anything else that fails is an error in words.
/// `wait`: the claim's `FAUCET_WAIT`, or the info's `FAUCET_INFO_WAIT`. An endpoint that asked to be
/// left alone (`registry::slowed_down`) isn't asked until then: said as `rate_limited` at once.
pub fn faucet_call(url: &str, method: &str, params: Value, wait: Duration) -> Result<Value, String> {
    crate::served::blocking("bridge_net::faucet_call");
    let limited = || json!({ "ok": false, "code": "rate_limited", "reason": "Too many requests to the bridge test network just now. Try again in a minute.", "retry": false });
    if registry::slowed_down(url).is_some() {
        return Ok(limited());
    }
    let response = ureq::post(url)
        .timeout(wait)
        .send_json(json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }));
    let body = match response {
        Ok(response) => response.into_string().map_err(|error| format!("the bridge test network's reply broke off: {error}"))?,
        Err(ureq::Error::Status(429, response)) => {
            registry::slow_down(url, registry::retry_after(429, response.header("retry-after")).unwrap_or(registry::RETRY_AFTER_MAX));
            return Ok(limited());
        }
        Err(ureq::Error::Status(502 | 503, _)) => {
            return Ok(json!({ "ok": false, "code": "busy", "reason": "The faucet is busy. Try again in a few seconds.", "retry": true }))
        }
        Err(ureq::Error::Status(status, _)) => return Err(format!("the bridge test network answered HTTP {status}")),
        Err(error) => return Err(problem(error.to_string())),
    };
    if body.len() > 64 * 1024 {
        return Err("the bridge test network sent a reply too large for a faucet answer".to_owned());
    }
    let reply: Value = serde_json::from_str(&body).map_err(|_| "the bridge test network's faucet reply isn't JSON".to_owned())?;
    if let Some(error) = reply.get("error").filter(|e| !e.is_null()) {
        let message = error["message"].as_str().unwrap_or("refused");
        return Err(format!("the bridge test network refused {method}: {}", crate::text::clean_remote(message, 160)));
    }
    Ok(reply["result"].clone())
}

/// The faucet's answer, kept to what the app shows: `{amount, per_account_hours, available,
/// reason}` for the info; `{ok, tx, block, amount, account}` or `{ok: false, code, reason, retry}`
/// for a claim. Text the endpoint sends is cleaned for showing.
pub fn faucet_shown(value: &Value) -> Value {
    let text = |key: &str| value[key].as_str().map(|t| crate::text::clean_remote(t, 200));
    let mut out = serde_json::Map::new();
    for key in ["ok", "available", "retry"] {
        if let Some(flag) = value[key].as_bool() {
            out.insert(key.into(), json!(flag));
        }
    }
    for key in ["amount", "per_account_hours", "block"] {
        match &value[key] {
            Value::String(t) if t.chars().all(|c| c.is_ascii_digit()) && !t.is_empty() => {
                out.insert(key.into(), json!(t));
            }
            Value::Number(n) => {
                out.insert(key.into(), json!(n.to_string()));
            }
            _ => {}
        }
    }
    for key in ["reason", "code", "tx", "account"] {
        if let Some(t) = text(key) {
            out.insert(key.into(), json!(t));
        }
    }
    Value::Object(out)
}

/// What the state file says, or an empty object.
fn load_state(dir: &Path) -> Value {
    std::fs::read_to_string(dir.join(STATE_FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn save_state(dir: &Path, state: &Value) {
    let _ = std::fs::create_dir_all(dir);
    let _ = crate::cli::write_private(&dir.join(STATE_FILE), state.to_string().as_bytes());
}

/// The chain and height the last read saw (the state file in `dir`), if any.
pub fn last_seen(dir: &Path) -> (Option<String>, Option<u64>) {
    let state = load_state(dir);
    (state["channel"].as_str().map(str::to_owned), state["height"].as_u64())
}

/// Starts the state file again on the chain `channel` at `height`, nothing kept and no reset
/// told: the person chose that chain (Settings, Network), so a read there next is no reset.
pub fn reseed(dir: &Path, channel: &str, height: u64) {
    save_state(dir, &json!({ "channel": channel, "height": height, "reset_at": null, "bridges": [] }));
}

/// Forgets what the state file kept (our LEZ 0.4 chosen again): the next read starts it.
pub fn forget_state(dir: &Path) {
    save_state(dir, &json!({}));
}

/// Records a reset seen outside a balance read (a network check found the height lower on the
/// same chain): told for a day as a read's would be, and nothing kept from before is used.
pub fn mark_reset(dir: &Path, channel: &str, height: u64, now: u64) {
    save_state(dir, &json!({ "channel": channel, "height": height, "reset_at": now, "bridges": [] }));
}

/// Whether the chain read now (`channel`, `height`) is not the one `saved` last saw: another
/// channel, or a lower height on the same one.
pub fn was_reset(saved: &Value, channel: &str, height: u64) -> bool {
    match saved["channel"].as_str() {
        Some(before) if before != channel => true,
        Some(_) => saved["height"].as_u64().is_some_and(|before| height < before),
        None => false,
    }
}

/// When a read last saw the bridge test network reset (`reset_at` in the state file in `dir`,
/// seconds), if that was less than a day before `now` (seconds; a read tells a reset for
/// `RESET_TOLD_S`). None with no file, an unreadable one, or no reset told: the file is this
/// module's own (written whole, mode 600), and it knows of a reset only once a read has run since.
pub fn reset_seen(dir: &Path, now: u64) -> Option<u64> {
    // A stamp later than now (the clock was set back) isn't taken as a recent reset.
    load_state(dir)["reset_at"].as_u64().filter(|at| *at <= now && now - *at < RESET_TOLD_S)
}

/// Reads the bridge test network at `url` for `accounts`, with the tokens of every bridge in
/// `programs` (at most `MAX_BRIDGES`), remembering in `dir` what it saw (the chain, each
/// bridge's token list). `now` is in seconds. The answer:
/// `{network, version, channel, height, read_at, bridge, bridges: [{program, bridge,
/// token_program, token_problem}], tokens: [{bridge, definition, symbol, decimals, name}],
/// token_problem, reset_at, accounts: [{account, found, native, tokens: [{bridge, definition,
/// symbol, decimals, amount}], other_tokens}]}`; `bridge` is whether any bridge is there, and
/// each token says which bridge listed it. Amounts are decimal text in base units. Any failed
/// request fails the whole read: a balance never read is never shown. Each account is one
/// request whatever the number of bridges (its reply holds every token program's holdings).
pub fn read(url: &str, programs: &[AccountId], accounts: &[AccountId], dir: &Path, now: u64) -> Result<Value, String> {
    let deadline = Deadline::after(READ_BUDGET);
    let channel = call(url, "getChannelId", json!([]), deadline)?;
    let channel = channel.as_str().and_then(crate::zones::channel).ok_or("the bridge test network didn't say which chain it is")?;
    let height = call(url, "getLastBlockId", json!([]), deadline)?.as_u64().ok_or("the bridge test network gave no block height")?;
    let saved = load_state(dir);
    let mut reset = was_reset(&saved, &channel, height);
    let same_chain = !reset && saved["channel"].as_str() == Some(channel.as_str());
    if reset {
        // Nothing kept from before the reset is used: the relayed-call checks read the Configs again.
        forget_configs(url);
    }

    let mut wanted: Vec<AccountId> = Vec::new();
    for program in programs {
        if !wanted.contains(program) && wanted.len() < MAX_BRIDGES {
            wanted.push(*program);
        }
    }
    // Each bridge's token list, kept per chain for an hour, else read from its Config again. A
    // state file from a build that knew one bridge holds it as `tokens`.
    let kept: Vec<Value> = saved["bridges"].as_array().cloned().unwrap_or_else(|| if saved["tokens"].is_object() { vec![saved["tokens"].clone()] } else { Vec::new() });
    struct Bridge {
        program: AccountId,
        present: bool,
        token_program: Option<AccountId>,
        tokens: Vec<Value>,
        problem: Option<String>,
        saved: Value,
    }
    let mut bridges: Vec<Bridge> = Vec::new();
    let wanted_all = wanted.clone();
    // A definition's own name, read once a chain (it never changes once made), at most
    // `NAMES_PER_READ` a read: the first read of a chain stays well under the endpoint's limit,
    // and the names left show from the next reads on. A token whose name was read is marked
    // (`name_read`), so one with none is never asked again.
    let names_left = std::cell::Cell::new(NAMES_PER_READ);
    let name_of = |token: &mut Value, token_program: &AccountId| {
        if token["name_read"] == true || token["name"].is_string() || names_left.get() == 0 {
            return;
        }
        let Some(definition) = token["definition"].as_str().map(str::to_owned) else { return };
        names_left.set(names_left.get() - 1);
        let read = call(url, "getAccount", json!([definition]), deadline);
        if read.is_ok() {
            token["name"] = json!(read.ok().and_then(|name| state_under(&name, token_program).ok()).and_then(|bytes| decode_definition_name(&bytes)));
            token["name_read"] = json!(true);
        }
    };
    for program in wanted {
        let text = registry::account_text(&program);
        let cached = kept.iter().find(|entry| entry["program"] == text.as_str()).cloned().unwrap_or(Value::Null);
        let cache_good = same_chain && cached["read_at"].as_u64().is_some_and(|at| now.saturating_sub(at) < TOKENS_FRESH_S) && cached["bridge"].is_boolean();
        let bridge = if cache_good {
            let token_program = cached["token_program"].as_str().and_then(|id| registry::parse_account(id).ok());
            // A list kept by the one-bridge build has no `bridge` on its rows: say which it is.
            let tokens: Vec<Value> = cached["list"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|mut token| {
                    if token["bridge"].is_null() {
                        token["bridge"] = json!(text);
                    }
                    if token["bridge_label"].is_null() {
                        token["bridge_label"] = json!(bridge_label(&text));
                    }
                    // A name left for a later read (`NAMES_PER_READ`).
                    if let Some(token_program) = &token_program {
                        name_of(&mut token, token_program);
                    }
                    token
                })
                .collect();
            let mut saved = cached.clone();
            saved["list"] = json!(tokens);
            Bridge { program, present: cached["bridge"] == true, token_program, tokens, problem: cached["problem"].as_str().map(str::to_owned), saved }
        } else {
            // The Config as kept for an hour (`config_state`, shared with the relayed-call checks),
            // read again on another chain. A Config that can't be read keeps the last list this
            // chain gave, said as such; one never read on this chain shows no tokens for that bridge,
            // and the read goes on: one bridge's endpoint trouble never hides the balances.
            let config = config_state_by(url, &program, if reset { Duration::ZERO } else { CONFIG_FRESH }, deadline);
            let config = match config {
                Ok(config) => config,
                Err(why) => {
                    let kept_list = cached["list"].as_array().cloned().filter(|_| same_chain && cached["bridge"] == true);
                    let problem = format!("can't read the bridge's token list just now ({}), so its tokens aren't shown", why.replace("can't reach the bridge test network: ", ""));
                    let bridge = match kept_list {
                        Some(list) => {
                            let token_program = cached["token_program"].as_str().and_then(|id| registry::parse_account(id).ok());
                            Bridge { program, present: true, token_program, tokens: list, problem: None, saved: cached }
                        }
                        None => {
                            let saved = json!({ "read_at": 0, "program": text, "bridge": true, "token_program": null, "list": [], "problem": problem });
                            Bridge { program, present: true, token_program: None, tokens: Vec::new(), problem: Some(problem), saved }
                        }
                    };
                    bridges.push(bridge);
                    continue;
                }
            };
            if config.is_none() {
                // The bridge was there on this chain and is gone: a reset by another name.
                if same_chain && cached["bridge"] == true {
                    reset = true;
                }
                let saved = json!({ "read_at": now, "program": text, "bridge": false, "token_program": null, "list": [], "problem": null });
                Bridge { program, present: false, token_program: None, tokens: Vec::new(), problem: None, saved }
            } else {
                match config.as_deref().map_or_else(|| Err("no Config".to_owned()), decode_config) {
                    Ok((token_program, listed)) => {
                        // Kept for good, as a relayed call's check keeps it (S-1): a bridge removed
                        // before any relayed call was checked stays refused as root. Never a bridge
                        // program itself.
                        if !wanted_all.contains(&token_program) {
                            keep_token_programs(dir, &[registry::account_text(&token_program)]);
                        }
                        let mut tokens = Vec::new();
                        // A definition's name never changes once made: one read on this chain is
                        // kept (only a new definition is read), and one that can't be read is shown
                        // without its name.
                        let known_names: Vec<Value> = if same_chain { cached["list"].as_array().cloned().unwrap_or_default() } else { Vec::new() };
                        for token in &listed {
                            let definition = registry::account_text(&token.definition);
                            let mut row = json!({ "bridge": text, "bridge_label": bridge_label(&text), "definition": definition, "eth_token": token.eth_token, "symbol": token.symbol(), "decimals": token.decimals, "name": null });
                            match known_names.iter().find(|known| known["definition"] == definition.as_str() && (known["name"].is_string() || known["name_read"] == true)) {
                                Some(known) => {
                                    row["name"] = known["name"].clone();
                                    row["name_read"] = json!(true);
                                }
                                None => name_of(&mut row, &token_program),
                            }
                            tokens.push(row);
                        }
                        let saved = json!({ "read_at": now, "program": text, "bridge": true, "token_program": registry::account_text(&token_program), "list": tokens, "problem": null });
                        Bridge { program, present: true, token_program: Some(token_program), tokens, problem: None, saved }
                    }
                    Err(_) => {
                        let why = "can't read the bridge's token list, so bridged tokens aren't shown".to_owned();
                        let saved = json!({ "read_at": now, "program": text, "bridge": true, "token_program": null, "list": [], "problem": why });
                        Bridge { program, present: true, token_program: None, tokens: Vec::new(), problem: Some(why), saved }
                    }
                }
            }
        };
        bridges.push(bridge);
    }
    let tokens: Vec<Value> = bridges.iter().flat_map(|bridge| bridge.tokens.iter().cloned()).collect();

    let mut rows = Vec::new();
    for (at, account) in accounts.iter().enumerate() {
        if at > 0 {
            std::thread::sleep(ACCOUNT_PAUSE);
        }
        let text = registry::account_text(account);
        let read = call(url, "getAccount", json!([text]), deadline)?;
        if read.is_null() {
            rows.push(json!({ "account": format!("Public/{text}"), "found": false, "native": "0", "tokens": tokens.iter().map(|t| held_row(t, 0)).collect::<Vec<_>>(), "other_tokens": 0 }));
            continue;
        }
        let native = native_of(&read).map_err(|why| format!("the bridge test network sent an account this version can't read ({why})"))?;
        // What the account holds under each bridge's token program (read once per program).
        let mut held: Vec<(AccountId, BTreeMap<AccountId, u128>)> = Vec::new();
        for token_program in bridges.iter().filter_map(|bridge| bridge.token_program) {
            if held.iter().any(|(known, _)| *known == token_program) {
                continue;
            }
            let holdings = decode_holdings(&state_under(&read, &token_program)?)
                .map_err(|why| format!("the bridge test network sent token holdings this version can't read ({why})"))?;
            held.push((token_program, holdings));
        }
        let holdings_of = |token_program: Option<AccountId>| held.iter().find(|(known, _)| Some(*known) == token_program).map(|(_, map)| map);
        let mut token_rows = Vec::new();
        for bridge in &bridges {
            let map = holdings_of(bridge.token_program);
            for t in &bridge.tokens {
                let amount = t["definition"].as_str().and_then(|d| registry::parse_account(d).ok()).and_then(|d| map.and_then(|m| m.get(&d).copied())).unwrap_or(0);
                token_rows.push(held_row(t, amount));
            }
        }
        // Holdings no configured bridge lists, under any of their token programs.
        let listed: Vec<String> = tokens.iter().filter_map(|t| t["definition"].as_str().map(str::to_owned)).collect();
        let other: usize = held.iter().map(|(_, map)| map.keys().filter(|definition| !listed.contains(&registry::account_text(definition))).count()).sum();
        rows.push(json!({ "account": format!("Public/{text}"), "found": true, "native": native.to_string(), "tokens": token_rows, "other_tokens": other }));
    }

    let reset_at = if reset {
        Some(now)
    } else {
        saved["reset_at"].as_u64().filter(|at| same_chain && now.saturating_sub(*at) < RESET_TOLD_S)
    };
    let kept_now: Vec<Value> = bridges.iter().map(|bridge| bridge.saved.clone()).collect();
    save_state(dir, &json!({ "channel": channel, "height": height, "reset_at": reset_at, "bridges": kept_now }));
    let problems: Vec<&str> = bridges.iter().filter_map(|bridge| bridge.problem.as_deref()).collect();
    Ok(json!({
        "network": NAME,
        "version": VERSION,
        "channel": channel,
        "height": height,
        "read_at": now,
        "bridge": bridges.iter().any(|bridge| bridge.present),
        "bridges": bridges.iter().map(|bridge| json!({
            "program": registry::account_text(&bridge.program),
            "label": bridge_label(&registry::account_text(&bridge.program)),
            "bridge": bridge.present,
            "token_program": bridge.token_program.map(|id| registry::account_text(&id)),
            "token_problem": bridge.problem,
        })).collect::<Vec<_>>(),
        "tokens": tokens,
        "token_problem": problems.first().copied(),
        "reset_at": reset_at,
        "accounts": rows,
    }))
}

fn held_row(token: &Value, amount: u128) -> Value {
    json!({ "bridge": token["bridge"], "bridge_label": token["bridge_label"], "definition": token["definition"], "symbol": token["symbol"], "decimals": token["decimals"], "name": token["name"], "amount": amount.to_string() })
}

/// The balances a read gives, for the History's balance marks: `(scope, [(account, amount)])`
/// per asset, native first (scope: the channel), then each token (scope: channel/definition).
/// None when the read couldn't give every token (its token list wasn't read).
pub fn marks(read: &Value) -> Option<Vec<(String, Vec<(String, u128)>, Option<Value>)>> {
    if !read["token_problem"].is_null() {
        return None;
    }
    let channel = read["channel"].as_str()?;
    // Only the wallet's own accounts go to its History, not the ones it watches.
    let own: Vec<Value> = read["accounts"].as_array()?.iter().filter(|a| a["watched"] != true).cloned().collect();
    let accounts = &own;
    let mut out = vec![(
        channel.to_owned(),
        accounts.iter().filter_map(|a| Some((a["account"].as_str()?.to_owned(), a["native"].as_str()?.parse().ok()?))).collect(),
        None,
    )];
    for token in read["tokens"].as_array()? {
        let definition = token["definition"].as_str()?;
        let rows = accounts
            .iter()
            .filter_map(|a| {
                let held = a["tokens"].as_array()?.iter().find(|t| t["definition"] == definition)?;
                Some((a["account"].as_str()?.to_owned(), held["amount"].as_str()?.parse().ok()?))
            })
            .collect();
        let about = json!({ "token": definition, "symbol": token["symbol"], "decimals": token["decimals"] });
        out.push((format!("{channel}/{definition}"), rows, Some(about)));
    }
    Some(out)
}

/// Notes a read's balances in the History (`activity`), so money seen arriving on the bridge
/// test network is told as such: each rise gets an `arrived` entry with `chain` `lez-0.4`,
/// `network` and, for a token, `token`, `symbol` and `decimals`. The first read of an account
/// on a chain only sets its marks. Returns the entries added.
pub fn note(dir: &Path, read: &Value) -> Vec<String> {
    note_with_native(dir, read, true)
}

/// Like [`note`], with `include_native` controlling whether native LEZ balances are also noted
/// here. When LEZ 0.4 is already the active home network, `note_lez` owns native LEZ balance
/// marks and reconciliation, so setting `include_native` to `false` avoids logging the same
/// native LEZ arrival twice (once under zone Home and once under the bridge test network).
pub fn note_with_native(dir: &Path, read: &Value, include_native: bool) -> Vec<String> {
    let (Some(marks), Some(at)) = (marks(read), read["read_at"].as_u64()) else {
        return Vec::new();
    };
    let mut added = Vec::new();
    for (scope, rows, about) in marks {
        if !include_native && about.is_none() {
            continue;
        }
        for id in crate::activity::note_network_balances(dir, at, CHAIN, &scope, ZONE, &rows) {
            let mut fields = json!({ "network": ZONE, "network_name": NAME });
            if let Some(about) = &about {
                fields["token"] = about["token"].clone();
                fields["symbol"] = about["symbol"].clone();
                fields["decimals"] = about["decimals"].clone();
            }
            crate::activity::update(dir, &id, &fields);
            added.push(id);
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/bridge-net").join(name);
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("fixture JSON")
    }

    fn id(text: &str) -> AccountId {
        registry::parse_account(text).expect("an id")
    }

    const OWNER: &str = "CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk";
    const UNSEEN: &str = "4wBqpZM9xaSheZzJSMawUKKwhdpChKbZ5eu5ky4Vigw";
    const WETH: &str = "DbPGo6FeRLp7NaUWxQGC1WXpJkSQe1jbAqYVMQkCuuPw";
    const DUSDC: &str = "9ANdXnWBPygeh3mJEDqAtk2AHG6uzZATeyUNyfvZBRVJ";
    const TOKEN_PROGRAM: &str = "FiooSkDaMZWNq4bFFpVXnDm2d2uoTB5S3cZ7kWhfUW4K";

    fn config_bytes() -> Vec<u8> {
        bytes_of(&fixture("config.json")["result"]["data"]["actor_states"][DEFAULT_BRIDGE_PROGRAM]).expect("bytes")
    }

    #[test]
    fn the_config_address_is_the_live_one() {
        assert_eq!(registry::account_text(&config_address(&id(DEFAULT_BRIDGE_PROGRAM))), "HugSoWM7Hp1eXMqknfCewWNMoraweLG3HkezfNGtqKf3");
    }

    #[test]
    fn the_live_config_gives_both_tokens_with_their_decimals() {
        let (program, tokens) = decode_config(&config_bytes()).expect("decodes");
        assert_eq!(registry::account_text(&program), TOKEN_PROGRAM);
        let shown: Vec<(String, String, u8)> = tokens.iter().map(|t| (t.symbol(), registry::account_text(&t.definition), t.decimals)).collect();
        assert_eq!(shown, vec![("dUSDC".to_owned(), DUSDC.to_owned(), 6), ("wETH".to_owned(), WETH.to_owned(), 18)]);
    }

    /// The fixed bridge (v3) as our 0.4 holds it (Config 7nQ38yMp…, read 2026-11-14): its token
    /// program, both tokens with their decimals, and the definitions' names. The definition's
    /// bytes after its name differ from v2's (the mint authority is a typed value now); the
    /// wallet reads only the name there, so it reads both.
    #[test]
    fn the_v3_config_and_definitions_read() {
        assert_eq!(registry::account_text(&config_address(&id(BRIDGE_V3_PROGRAM))), "7nQ38yMpJFvWY3tzMGUv4pJyqy5AfFJTHVpN75Yj9b4G");
        let bytes = bytes_of(&fixture("config-v3.json")["result"]["data"]["actor_states"][BRIDGE_V3_PROGRAM]).expect("bytes");
        let (program, tokens) = decode_config(&bytes).expect("decodes");
        assert_eq!(registry::account_text(&program), "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7");
        assert!(KNOWN_TOKEN_PROGRAMS.contains(&registry::account_text(&program).as_str()), "built in");
        let mut shown: Vec<(String, String, u8)> = tokens.iter().map(|t| (t.symbol(), registry::account_text(&t.definition), t.decimals)).collect();
        shown.sort();
        assert_eq!(shown, vec![
            ("dUSDC".to_owned(), "2MjYbRMi9VW81HGM5o6yptnCwvnY9NXfLLtFeqELkNXy".to_owned(), 6),
            ("wETH".to_owned(), "CUsiaS9iH4MYtbHbp6mpfqAPaHCa1py5UT5GN8vQYXzM".to_owned(), 18),
        ]);
        let name = |file: &str| decode_definition_name(&bytes_of(&fixture(file)["result"]["data"]["actor_states"]["5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7"]).unwrap());
        assert_eq!(name("def-weth-v3.json").as_deref(), Some("Wrapped Sepolia ETH (demo v3)"));
        assert_eq!(name("def-dusdc-v3.json").as_deref(), Some("Wrapped dUSDC (demo v3)"));
        assert_eq!((bridge_label(DEFAULT_BRIDGE_PROGRAM), bridge_label(BRIDGE_V3_PROGRAM), bridge_label("x")), (Some("bridge v2"), Some("bridge v3"), None));
        assert_eq!(bridge_label(BRIDGE_V4_PROGRAM), Some("bridge v4"));
        assert_eq!(bridge_label(BRIDGE_V5_PROGRAM), Some("bridge v5"));
        // v5's Config, as deployed (the conductor's ids, 2026-11-15).
        assert_eq!(registry::account_text(&config_address(&id(BRIDGE_V5_PROGRAM))), "FBWWSLgMgEtMs7PUNBfTGXccZdRSAeRHtMRP4nyZsrnW");
        assert!(BURN_BRIDGES.contains(&BRIDGE_V4_PROGRAM) && BURN_BRIDGES.contains(&BRIDGE_V5_PROGRAM) && !BURN_BRIDGES.contains(&BRIDGE_V3_PROGRAM));
        assert!(DEFAULT_BRIDGE_PROGRAMS.len() <= MAX_BRIDGES && DEFAULT_BRIDGE_PROGRAMS.contains(&BRIDGE_V4_PROGRAM));
    }

    /// v6 and v7 as the bridge test network holds them (`config-v6.json`, `config-v7.json`: one
    /// read-only `getAccount` of each Config, 2026-11-18): each fixture is the Config of its own
    /// bridge (the address derived here is the one read, the state sits under that program, the
    /// vault is the deployment's), both light-client Configs (root source 1) decode, share v3 to
    /// v5's token program, and list their own wrapped definitions. Every definition of v4 to v7 is
    /// different from every other: that is what holds a burn to its own bridge.
    #[test]
    fn the_v6_and_v7_configs_read_and_every_burn_bridge_has_its_own_tokens() {
        let cases = [
            (BRIDGE_V4_PROGRAM, "config-v4.json", "HUdVSAHEKE2cQbrzzb9eBi4LSwjvQV64XqxmwH9PXsWX", "c2c6a059eab928966b24fa5549d441a3b17b54ed"),
            (BRIDGE_V5_PROGRAM, "config-v5.json", "FBWWSLgMgEtMs7PUNBfTGXccZdRSAeRHtMRP4nyZsrnW", "afbdf192434e9888bb23dae6f2d15eea6246ed10"),
            (BRIDGE_V6_PROGRAM, "config-v6.json", "9osFF9nkdvEy51wARL7peUF94um3ZBExhAhYSK9ypiNz", "95cea20e75d3b352b49d13cb0cddc268a14c427e"),
            (BRIDGE_V7_PROGRAM, "config-v7.json", "DTHA5tMxD4Vv2BtahwK3cyTFRfLvXE84N7s3AfvxYFg3", "4d7b360cf385d3477f1a67c1aedeb6c17c26d23e"),
        ];
        let mut definitions = Vec::new();
        for (program, file, config, vault) in cases {
            assert_eq!(registry::account_text(&config_address(&id(program))), config, "{file}: the Config read is {program}'s");
            let bytes = config_bytes_in(&fixture(file)["result"], &id(program)).unwrap_or_else(|why| panic!("{file}: {why}"));
            assert_eq!(hex(&bytes[32..52]), vault, "{file}: the deployment's vault");
            let config = decode_bridge_config(&bytes).unwrap_or_else(|why| panic!("{file}: {why}"));
            assert_eq!(registry::account_text(&config.token_program), "5w6nTkftFFDy8mtdpnKpZVetGQJ2WCKdhmpJsAYArbU7", "{file}");
            assert!(!config.burn_frozen && config.tokens.len() == 2 && config.tokens.iter().all(|t| t.registered && !t.burn_frozen && t.denoms.len() == 3), "{file}");
            let mut symbols: Vec<String> = config.tokens.iter().map(Token::symbol).collect();
            symbols.sort();
            assert_eq!(symbols, ["dUSDC", "wETH"], "{file}");
            assert!(BURN_BRIDGES.contains(&program) && DEFAULT_BRIDGE_PROGRAMS.contains(&program), "{file}");
            definitions.extend(config.tokens.iter().map(|t| t.definition));
        }
        let mut distinct = definitions.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!((definitions.len(), distinct.len()), (8, 8), "every bridge's wrapped tokens are its own");
        // What binds each Config to its vault (a burn's opening must name it): v5 on Sepolia.
        let (_, vault5, chain5) = decode_config_identity(&config_bytes_in(&fixture("config-v5.json")["result"], &id(BRIDGE_V5_PROGRAM)).unwrap()).unwrap();
        assert_eq!((hex(&vault5), chain5), ("afbdf192434e9888bb23dae6f2d15eea6246ed10".to_owned(), 11_155_111));
        let v6 = decode_bridge_config(&config_bytes_in(&fixture("config-v6.json")["result"], &id(BRIDGE_V6_PROGRAM)).unwrap()).unwrap();
        assert_eq!(registry::account_text(&v6.tokens.iter().find(|t| t.symbol() == "wETH").unwrap().definition), "5WLrfRYHGY14wnNG7sa8b6Bzy9ZXPCeB4XnHK2qiuP5t");
        let v7 = decode_bridge_config(&config_bytes_in(&fixture("config-v7.json")["result"], &id(BRIDGE_V7_PROGRAM)).unwrap()).unwrap();
        assert_eq!(registry::account_text(&v7.tokens.iter().find(|t| t.symbol() == "wETH").unwrap().definition), "BWDmKjDqkJDrLt1DDXGPeL4DBF8RsGAwB9BQvT7Hzpqg");
        assert_eq!((bridge_label(BRIDGE_V6_PROGRAM), bridge_label(BRIDGE_V7_PROGRAM)), (Some("bridge v6"), Some("bridge v7")));
        assert_eq!((DEFAULT_BRIDGE_PROGRAMS.len(), MAX_BRIDGES, BRIDGE_LABELS.len()), (6, 6, 6));
        assert!(!BURN_BRIDGES.contains(&DEFAULT_BRIDGE_PROGRAM) && !BURN_BRIDGES.contains(&BRIDGE_V3_PROGRAM));
    }

    #[test]
    fn a_config_that_is_short_long_or_unknown_is_refused() {
        let bytes = config_bytes();
        assert!(decode_config(&bytes[..bytes.len() - 1]).is_err(), "short");
        let mut long = bytes.clone();
        long.push(0);
        assert!(decode_config(&long).is_err(), "a byte left over");
        let mut tag = bytes.clone();
        let at = tag.len() - 17; // the root source's tag
        tag[at] = 7;
        assert!(decode_config(&tag).is_err(), "an unknown root source");
        assert!(decode_config(&[]).is_err());
    }

    #[test]
    fn the_owners_account_holds_a_tenth_of_wrapped_eth() {
        let owner = &fixture("owner.json")["result"];
        assert_eq!(native_of(owner).unwrap(), 23_786_858);
        let held = decode_holdings(&state_under(owner, &id(TOKEN_PROGRAM)).unwrap()).unwrap();
        assert_eq!(held.into_iter().collect::<Vec<_>>(), vec![(id(WETH), 100_000_000_000_000_000)]);
    }

    #[test]
    fn holdings_empty_a_definition_or_garbage() {
        assert!(decode_holdings(&[]).unwrap().is_empty());
        let definition = bytes_of(&fixture("def-weth.json")["result"]["data"]["actor_states"][TOKEN_PROGRAM]).unwrap();
        assert!(decode_holdings(&definition).unwrap().is_empty(), "a definition holds nothing");
        assert!(decode_holdings(&[0, 1, 0, 0, 0, 1]).is_err(), "short");
        assert!(decode_holdings(&[9]).is_err(), "unknown tag");
    }

    #[test]
    fn definition_names_are_read_and_cleaned() {
        let name = |file: &str| decode_definition_name(&bytes_of(&fixture(file)["result"]["data"]["actor_states"][TOKEN_PROGRAM]).unwrap());
        assert_eq!(name("def-weth.json").as_deref(), Some("Wrapped Sepolia ETH (demo)"));
        assert_eq!(name("def-dusdc.json").as_deref(), Some("Wrapped dUSDC (demo)"));
        assert_eq!(decode_definition_name(&[0, 0, 0, 0, 0]), None, "holdings aren't a definition");
    }

    #[test]
    fn account_ids_are_32_bytes_and_public() {
        assert_eq!(public_account(&format!("Public/{OWNER}")).unwrap(), id(OWNER));
        assert_eq!(public_account(&format!("  {OWNER} ")).unwrap(), id(OWNER));
        assert!(public_account("abc").is_err());
        assert!(public_account("notbase58!!").is_err());
        assert!(public_account(&format!("Private/{OWNER}")).is_err());
        let (accounts, private) = accounts_asked(&json!([format!("Public/{OWNER}"), OWNER, format!("Private/{UNSEEN}")]).to_string()).unwrap();
        assert_eq!((accounts, private), (vec![id(OWNER)], 1));
        assert!(accounts_asked("[\"Public/abc\"]").is_err());
        assert_eq!(accounts_asked("").unwrap(), (Vec::new(), 0));
    }

    /// A stand-in for the bridge test network answering with the recorded replies, with the
    /// channel and height a test sets.
    fn network(channel: &'static str, height: &'static std::sync::atomic::AtomicU64, config: bool) -> String {
        registry::stub_rpc(move |request| {
            let account = request["params"][0].as_str().unwrap_or_default().to_owned();
            let file = match request["method"].as_str().unwrap_or_default() {
                "getChannelId" => return registry::rpc_ok(json!(channel)),
                "getLastBlockId" => return registry::rpc_ok(json!(height.load(std::sync::atomic::Ordering::SeqCst))),
                "getAccount" if account == "HugSoWM7Hp1eXMqknfCewWNMoraweLG3HkezfNGtqKf3" => if config { "config.json" } else { "unseen.json" },
                "getAccount" if account == WETH => "def-weth.json",
                "getAccount" if account == DUSDC => "def-dusdc.json",
                "getAccount" if account == OWNER => "owner.json",
                "getAccount" => "unseen.json",
                _ => return (403, json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "method not served here" } }).to_string()),
            };
            (200, fixture(file).to_string())
        })
    }

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bridge-net-test-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const CHANNEL: &str = "b4a9c0de162a2a1b0b6ee9840a90ae90e64bdb49414236679a03e1d86bcc99be";

    #[test]
    fn a_read_shows_native_and_bridged_tokens_and_nothing_for_an_unseen_account() {
        static HEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(555);
        let url = network(CHANNEL, &HEIGHT, true);
        let dir = scratch("read");
        let read = read(&url, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER), id(UNSEEN)], &dir, 1_000).expect("reads");
        assert_eq!((read["channel"].as_str(), read["height"].as_u64(), read["bridge"].as_bool()), (Some(CHANNEL), Some(555), Some(true)));
        assert!(read["reset_at"].is_null() && read["token_problem"].is_null());
        let owner = &read["accounts"][0];
        assert_eq!((owner["account"].as_str(), owner["found"].as_bool(), owner["native"].as_str()), (Some(&*format!("Public/{OWNER}")), Some(true), Some("23786858")));
        let tokens: Vec<(&str, &str, u64)> = owner["tokens"].as_array().unwrap().iter().map(|t| (t["symbol"].as_str().unwrap(), t["amount"].as_str().unwrap(), t["decimals"].as_u64().unwrap())).collect();
        assert_eq!(tokens, vec![("dUSDC", "0", 6), ("wETH", "100000000000000000", 18)]);
        assert_eq!(read["tokens"][1]["name"], "Wrapped Sepolia ETH (demo)");
        let unseen = &read["accounts"][1];
        assert_eq!((unseen["found"].as_bool(), unseen["native"].as_str()), (Some(false), Some("0")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_channel_or_a_lower_height_is_a_reset() {
        static HEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(555);
        let dir = scratch("reset");
        let first = network(CHANNEL, &HEIGHT, true);
        assert!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_000).unwrap()["reset_at"].is_null());
        assert_eq!(reset_seen(&dir, 1_000), None, "no reset in the state file");
        // The same chain, later: no reset.
        HEIGHT.store(600, std::sync::atomic::Ordering::SeqCst);
        assert!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_100).unwrap()["reset_at"].is_null());
        // The same channel, a lower height: reset, and told for a day.
        HEIGHT.store(3, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_200).unwrap()["reset_at"].as_u64(), Some(1_200));
        HEIGHT.store(9, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_300).unwrap()["reset_at"].as_u64(), Some(1_200));
        // What a re-sign reads (`reset_seen`): the reset, from the file, for a day after it was seen,
        // even when no read has run since.
        assert_eq!((reset_seen(&dir, 1_300), reset_seen(&dir, 1_200 + RESET_TOLD_S - 1), reset_seen(&dir, 1_200 + RESET_TOLD_S)), (Some(1_200), Some(1_200), None));
        assert!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_200 + RESET_TOLD_S).unwrap()["reset_at"].is_null());
        // Another channel: reset.
        let other = network("ab".repeat(32).leak(), &HEIGHT, true);
        assert_eq!(read(&other, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 200_000).unwrap()["reset_at"].as_u64(), Some(200_000));
        // A reset stamped later than now (the clock was set back): not taken as recent.
        save_state(&dir, &json!({ "channel": CHANNEL, "height": 5, "reset_at": 300_000, "bridges": [] }));
        assert_eq!(reset_seen(&dir, 200_000), None, "a reset_at in the future");
        // A file that isn't the state (written by hand, cut short): no reset known.
        std::fs::write(dir.join(STATE_FILE), "{\"reset_at\": 2").unwrap();
        assert_eq!(reset_seen(&dir, 200_000), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_chain_without_the_bridge_shows_native_only_and_its_loss_is_a_reset() {
        static HEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(10);
        let dir = scratch("nobridge");
        let with = network(CHANNEL, &HEIGHT, true);
        read(&with, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_000).unwrap();
        HEIGHT.store(20, std::sync::atomic::Ordering::SeqCst);
        let without = network(CHANNEL, &HEIGHT, false);
        // The token list is kept for an hour; past it the Config is read again and is gone.
        let read = read(&without, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_000 + TOKENS_FRESH_S).unwrap();
        assert_eq!((read["bridge"].as_bool(), read["reset_at"].as_u64()), (Some(false), Some(1_000 + TOKENS_FRESH_S)));
        assert_eq!(read["accounts"][0]["native"], "23786858");
        assert!(read["accounts"][0]["tokens"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The bridge is a setting: another bridge program reads its own Config, never the token
    /// list kept for the one before (a redeploy with a new id shows its own tokens at once).
    #[test]
    fn another_bridge_program_reads_its_own_config() {
        static HEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(10);
        let dir = scratch("otherbridge");
        let url = network(CHANNEL, &HEIGHT, true);
        let first = read(&url, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_000).unwrap();
        assert_eq!(first["bridge"], true);
        // The stand-in network has no Config for another program: that bridge isn't there.
        let other = read(&url, &[[9; 32]], &[id(OWNER)], &dir, 1_001).unwrap();
        assert_eq!(other["bridge"], false, "{other}");
        assert!(other["accounts"][0]["tokens"].as_array().unwrap().is_empty());
        assert!(other["reset_at"].is_null(), "another bridge setting is not a reset of the chain: {other}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Two bridges (a stand-in second one: the live Config's bytes under another bridge program,
    /// naming another token program, with the owner holding under both): one read shows both
    /// bridges' tokens, each row saying which bridge, still one request per account; a state file
    /// from a build that knew one bridge is still used.
    #[test]
    fn two_bridges_show_their_tokens_side_by_side() {
        let second: AccountId = [7; 32];
        let second_tokens: AccountId = [8; 32];
        let mut bytes = config_bytes();
        bytes[32 + 20 + 8 + 64..32 + 20 + 8 + 64 + 32].copy_from_slice(&second_tokens);
        let second_config = json!({ "jsonrpc": "2.0", "id": 1, "result": { "data": { "actor_states": { registry::account_text(&second): bytes } } } });
        let second_address = registry::account_text(&config_address(&second));
        let mut owner = fixture("owner.json");
        let held = owner["result"]["data"]["actor_states"][TOKEN_PROGRAM].clone();
        owner["result"]["data"]["actor_states"][registry::account_text(&second_tokens)] = held;
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner_reads = asked.clone();
        let url = registry::stub_rpc(move |request| {
            let account = request["params"][0].as_str().unwrap_or_default().to_owned();
            let file = match request["method"].as_str().unwrap_or_default() {
                "getChannelId" => return registry::rpc_ok(json!(CHANNEL)),
                "getLastBlockId" => return registry::rpc_ok(json!(700)),
                "getAccount" if account == second_address => return (200, second_config.to_string()),
                "getAccount" if account == OWNER => {
                    owner_reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return (200, owner.to_string());
                }
                "getAccount" if account == "HugSoWM7Hp1eXMqknfCewWNMoraweLG3HkezfNGtqKf3" => "config.json",
                "getAccount" if account == WETH => "def-weth.json",
                "getAccount" if account == DUSDC => "def-dusdc.json",
                "getAccount" => "unseen.json",
                _ => return (403, json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "method not served here" } }).to_string()),
            };
            (200, fixture(file).to_string())
        });
        let dir = scratch("twobridges");
        // A state file as the one-bridge build left it: the first bridge's list, fresh, its rows
        // without `bridge`.
        save_state(&dir, &json!({ "channel": CHANNEL, "height": 690, "reset_at": null,
            "tokens": { "read_at": 990, "program": DEFAULT_BRIDGE_PROGRAM, "bridge": true, "token_program": TOKEN_PROGRAM,
                        "list": [{ "definition": WETH, "symbol": "wETH", "decimals": 18, "name": null }], "problem": null } }));
        let first = read(&url, &[id(DEFAULT_BRIDGE_PROGRAM), second, id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_000).expect("reads");
        assert!(first["reset_at"].is_null(), "the old state file isn't a reset: {first}");
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1, "one request for the account");
        let bridges: Vec<(&str, bool, &str)> = first["bridges"].as_array().unwrap().iter().map(|b| (b["program"].as_str().unwrap(), b["bridge"] == true, b["token_program"].as_str().unwrap_or(""))).collect();
        let (second_text, second_tokens_text) = (registry::account_text(&second), registry::account_text(&second_tokens));
        assert_eq!(bridges, vec![(DEFAULT_BRIDGE_PROGRAM, true, TOKEN_PROGRAM), (second_text.as_str(), true, second_tokens_text.as_str())], "a repeat is read once");
        // The first bridge's list came from the old state file (its row now says which bridge);
        // the second's from its Config.
        let rows: Vec<(&str, &str, &str)> = first["accounts"][0]["tokens"].as_array().unwrap().iter().map(|t| (t["bridge"].as_str().unwrap_or("NONE"), t["symbol"].as_str().unwrap(), t["amount"].as_str().unwrap())).collect();
        assert_eq!(rows, vec![(DEFAULT_BRIDGE_PROGRAM, "wETH", "100000000000000000"), (second_text.as_str(), "dUSDC", "0"), (second_text.as_str(), "wETH", "100000000000000000")]);
        // An hour later both Configs are read: both bridges' tokens, each held under its own program.
        let later = read(&url, &[id(DEFAULT_BRIDGE_PROGRAM), second], &[id(OWNER)], &dir, 1_000 + TOKENS_FRESH_S).expect("reads");
        let rows: Vec<(String, &str, &str)> = later["accounts"][0]["tokens"].as_array().unwrap().iter().map(|t| (t["bridge"].as_str().unwrap().to_owned(), t["symbol"].as_str().unwrap(), t["amount"].as_str().unwrap())).collect();
        assert_eq!(rows, vec![
            (DEFAULT_BRIDGE_PROGRAM.to_owned(), "dUSDC", "0"), (DEFAULT_BRIDGE_PROGRAM.to_owned(), "wETH", "100000000000000000"),
            (second_text.clone(), "dUSDC", "0"), (second_text.clone(), "wETH", "100000000000000000"),
        ]);
        assert_eq!(later["accounts"][0]["other_tokens"], 0);
        // SF-2: every Config read keeps its token program for good, as a relayed call's check does.
        let kept = seen_token_programs(&dir).unwrap();
        assert!(kept.contains(&TOKEN_PROGRAM.to_owned()) && kept.contains(&second_tokens_text), "{kept:?}");
        assert!(later["token_problem"].is_null());
        // At most MAX_BRIDGES are read.
        let many: Vec<AccountId> = (0..(MAX_BRIDGES as u8 + 2)).map(|n| [n + 30; 32]).collect();
        assert_eq!(read(&url, &many, &[id(OWNER)], &dir, 9_000).unwrap()["bridges"].as_array().unwrap().len(), MAX_BRIDGES);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_kept_token_programs_only_grow_and_are_capped() {
        let dir = scratch("kept");
        assert_eq!(seen_token_programs(&dir), Ok(Vec::new()), "no file: none");
        keep_token_programs(&dir, &[registry::account_text(&[1; 32])]);
        keep_token_programs(&dir, &[]);
        assert_eq!(seen_token_programs(&dir).unwrap(), vec![registry::account_text(&[1; 32])]);
        let many: Vec<String> = (2..40u8).map(|n| registry::account_text(&[n; 32])).collect();
        keep_token_programs(&dir, &many);
        let kept = seen_token_programs(&dir).unwrap();
        assert_eq!(kept.len(), MAX_SEEN_TOKEN_PROGRAMS);
        assert_eq!(kept[0], registry::account_text(&[1; 32]), "never removes one");
        // SF-3: a file that can't be read is an error, and is never written over.
        std::fs::write(dir.join(SEEN_TOKEN_PROGRAMS), "{ not a list").unwrap();
        assert!(seen_token_programs(&dir).is_err());
        keep_token_programs(&dir, &[registry::account_text(&[2; 32])]);
        assert_eq!(std::fs::read_to_string(dir.join(SEEN_TOKEN_PROGRAMS)).unwrap(), "{ not a list", "left as it is");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreachable_network_fails_the_read_in_words() {
        let dir = scratch("down");
        let url = registry::stub_rpc(|_| (502, json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32603, "message": "sequencer unavailable" } }).to_string()));
        let why = read(&url, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1).unwrap_err();
        assert!(why.starts_with("can't reach the bridge test network"), "{why}");
        assert!(!dir.join(STATE_FILE).exists(), "nothing remembered from a failed read");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The faucet: its info and a claim through the endpoint, its 429 and 502 said as refusals
    /// the app shows, its JSON-RPC errors as words; text cleaned, numbers as digits.
    #[test]
    fn the_faucet_is_asked_and_its_answers_kept_plain() {
        let url = registry::stub_rpc(|request| match request["method"].as_str().unwrap_or_default() {
            "bridge_faucetInfo" => registry::rpc_ok(json!({ "amount": "10000000", "per_account_hours": 24, "available": true, "reason": null })),
            "bridge_faucet" if request["params"][0] == "ab".repeat(32) => registry::rpc_ok(json!({ "ok": true, "tx": "cd".repeat(32), "block": 11222, "amount": "10000000", "account": "Public/X" })),
            "bridge_faucet" if request["params"][0] == "ee".repeat(32) => registry::rpc_ok(json!({ "ok": false, "code": "already_today", "reason": "This account had test LEZ today.\u{202E}", "retry": false })),
            "bridge_faucet" if request["params"][0] == "42".repeat(32) => (429, "slow down".to_owned()),
            "bridge_faucet" if request["params"][0] == "50".repeat(32) => (503, "".to_owned()),
            _ => (200, json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "no such method" } }).to_string()),
        });
        let info = faucet_shown(&faucet_call(&url, "bridge_faucetInfo", json!([]), FAUCET_INFO_WAIT).unwrap());
        assert_eq!(info, json!({ "available": true, "amount": "10000000", "per_account_hours": "24" }));
        let got = faucet_shown(&faucet_call(&url, "bridge_faucet", json!(["ab".repeat(32), "Public/X"]), FAUCET_WAIT).unwrap());
        assert_eq!((got["ok"].as_bool(), got["block"].as_str(), got["amount"].as_str()), (Some(true), Some("11222"), Some("10000000")));
        let refused = faucet_shown(&faucet_call(&url, "bridge_faucet", json!(["ee".repeat(32)]), FAUCET_WAIT).unwrap());
        assert_eq!((refused["code"].as_str(), refused["reason"].as_str()), (Some("already_today"), Some("This account had test LEZ today.")));
        let busy = faucet_call(&url, "bridge_faucet", json!(["50".repeat(32)]), FAUCET_WAIT).unwrap();
        assert_eq!((busy["code"].as_str(), busy["retry"].as_bool()), (Some("busy"), Some(true)));
        assert!(faucet_call(&url, "bridge_other", json!([]), FAUCET_WAIT).unwrap_err().contains("no such method"));
        // A 429: said as rate limited, and the endpoint is left alone for its wait (10 s without a
        // Retry-After): the next ask is answered the same at once, without a request.
        assert_eq!(faucet_call(&url, "bridge_faucet", json!(["42".repeat(32)]), FAUCET_WAIT).unwrap()["code"], "rate_limited");
        assert!(registry::slowed_down(&url).is_some_and(|left| left > Duration::from_secs(8)));
        let started = std::time::Instant::now();
        assert_eq!(faucet_call(&url, "bridge_faucetInfo", json!([]), FAUCET_INFO_WAIT).unwrap()["code"], "rate_limited");
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn watched_accounts_are_kept_checked_and_bounded() {
        let dir = scratch("watch");
        assert_eq!(watch(&dir, OWNER, true).unwrap(), vec![format!("Public/{OWNER}")]);
        assert_eq!(watch(&dir, &format!(" Public/{OWNER} "), true).unwrap(), vec![format!("Public/{OWNER}")], "once");
        assert!(watch(&dir, "abc", true).is_err());
        assert!(watch(&dir, &format!("Private/{UNSEEN}"), true).is_err());
        assert_eq!(watch(&dir, UNSEEN, true).unwrap().len(), 2);
        assert_eq!(watch(&dir, OWNER, false).unwrap(), vec![format!("Public/{UNSEEN}")]);
        assert_eq!(watched(&dir), vec![format!("Public/{UNSEEN}")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn watched_rows_stay_out_of_the_history() {
        let read = json!({ "channel": CHANNEL, "read_at": 5, "token_problem": null, "tokens": [],
            "accounts": [{ "account": "Public/A", "native": "1", "tokens": [] }, { "account": "Public/B", "native": "2", "tokens": [], "watched": true }] });
        let marks = marks(&read).unwrap();
        assert_eq!(marks[0].1, vec![("Public/A".to_owned(), 1)]);
    }

    /// The real network, through the real HTTPS client: reads only. Run by hand with
    /// `cargo test live_read -- --ignored`; never in CI (it needs the network).
    #[test]
    #[ignore]
    fn live_read() {
        let dir = scratch("live");
        let read = read(RPC, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER), id(UNSEEN)], &dir, 1).expect("the bridge test network answers");
        println!("{read:#}");
        assert_eq!(read["bridge"], true);
        assert_eq!(read["accounts"][1]["found"], false);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_rise_is_noted_once_as_arriving_on_the_bridge_test_network() {
        static HEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(555);
        let dir = scratch("note");
        let url = network(CHANNEL, &HEIGHT, true);
        let first = read(&url, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_000).unwrap();
        assert!(note(&dir, &first).is_empty(), "the first read only sets the marks");
        // The same balances again: nothing.
        assert!(note(&dir, &first).is_empty());
        // 0.1 more wrapped ETH and 5 more native, read later.
        let mut later = first.clone();
        later["read_at"] = json!(1_030);
        later["accounts"][0]["native"] = json!("23786863");
        later["accounts"][0]["tokens"][1]["amount"] = json!("200000000000000000");
        let added = note(&dir, &later);
        assert_eq!(added.len(), 2, "{added:?}");
        let all = crate::activity::all(&dir);
        let token = all.iter().find(|e| e["token"] == WETH).expect("the token entry");
        assert_eq!((token["kind"].as_str(), token["chain"].as_str(), token["network"].as_str(), token["amount"].as_str(), token["symbol"].as_str(), token["decimals"].as_u64()),
            (Some("arrived"), Some(CHAIN), Some(ZONE), Some("100000000000000000"), Some("wETH"), Some(18)));
        let native = all.iter().find(|e| e["token"].is_null()).expect("the native entry");
        assert_eq!((native["amount"].as_str(), native["to"].as_str()), (Some("5"), Some(&*format!("Public/{OWNER}"))));
        // A LEZ 0.3 entry for the same account is never counted here.
        crate::activity::add(&dir, 1_040, json!({ "kind": "pay_account", "chain": "lez", "from": format!("Public/{OWNER}"), "to": format!("Public/{UNSEEN}"), "amount": "1000", "state": "done" }));
        let mut again = later.clone();
        again["read_at"] = json!(1_060);
        assert!(note(&dir, &again).is_empty(), "a LEZ 0.3 payment isn't taken for a change here");
        // A token list that couldn't be read notes nothing.
        let mut broken = again.clone();
        broken["token_problem"] = json!("can't read");
        assert!(marks(&broken).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
