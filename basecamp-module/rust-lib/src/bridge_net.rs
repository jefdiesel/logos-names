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
/// The bridges read when none is set: v2, v3 and v4.
pub const DEFAULT_BRIDGE_PROGRAMS: [&str; 3] = [DEFAULT_BRIDGE_PROGRAM, BRIDGE_V3_PROGRAM, BRIDGE_V4_PROGRAM];
/// What each known bridge is called on a balance row, by its program: the names of the
/// deployments (rfp-022's v2, its fixed v3 and v4). Another bridge shows its short program ID.
pub const BRIDGE_LABELS: [(&str, &str); 3] = [(DEFAULT_BRIDGE_PROGRAM, "bridge v2"), (BRIDGE_V3_PROGRAM, "bridge v3"), (BRIDGE_V4_PROGRAM, "bridge v4")];

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
/// once an hour) in every balance read and every relayed-call check.
pub const MAX_BRIDGES: usize = 4;
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
/// all of them (plus 3 to 5 others) stays well under the edge's 50 requests per 10 s per IP.
pub const MAX_ACCOUNTS: usize = 24;
/// The pause between two account reads.
const ACCOUNT_PAUSE: Duration = Duration::from_millis(120);
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

/// The bridges a relayed burn may go to: the ones whose vault releases by proof (v4). Built in,
/// not a setting: v2 and v3 share v4's token program, and a burn to them destroys the tokens for
/// good (their vaults can never release). Never `set_bridge_programs`.
pub const BURN_BRIDGES: [&str; 1] = [BRIDGE_V4_PROGRAM];

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

/// The whole Config of the bridge `program` on the chain at `url`, or `None` when the chain has
/// none. Any failed read is an error (fail closed), as in `token_program_of`.
pub fn config_of(url: &str, program: &AccountId) -> Result<Option<BridgeConfig>, String> {
    let deadline = Deadline::after(Duration::from_secs(8));
    let config = call(url, "getAccount", json!([registry::account_text(&config_address(program))]), deadline)?;
    if config.is_null() {
        return Ok(None);
    }
    let bytes = state_under(&config, program)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    decode_bridge_config(&bytes).map(Some)
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
    let cleaned = crate::text::clean_text(name, 64);
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
pub fn token_program_of(url: &str, program: &AccountId) -> Result<Option<AccountId>, String> {
    let deadline = Deadline::after(Duration::from_secs(8));
    let config = call(url, "getAccount", json!([registry::account_text(&config_address(program))]), deadline)?;
    if config.is_null() {
        return Ok(None);
    }
    let bytes = state_under(&config, program)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    decode_config(&bytes).map(|(token_program, _)| Some(token_program))
}

/// What the state file says, or an empty object.
fn load_state(dir: &Path) -> Value {
    std::fs::read_to_string(dir.join(STATE_FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn save_state(dir: &Path, state: &Value) {
    let _ = std::fs::create_dir_all(dir);
    let _ = crate::cli::write_private(&dir.join(STATE_FILE), state.to_string().as_bytes());
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
    for program in wanted {
        let text = registry::account_text(&program);
        let cached = kept.iter().find(|entry| entry["program"] == text.as_str()).cloned().unwrap_or(Value::Null);
        let cache_good = same_chain && cached["read_at"].as_u64().is_some_and(|at| now.saturating_sub(at) < TOKENS_FRESH_S) && cached["bridge"].is_boolean();
        let bridge = if cache_good {
            let token_program = cached["token_program"].as_str().and_then(|id| registry::parse_account(id).ok());
            // A list kept by the one-bridge build has no `bridge` on its rows: say which it is.
            let tokens = cached["list"]
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
                    token
                })
                .collect();
            Bridge { program, present: cached["bridge"] == true, token_program, tokens, problem: cached["problem"].as_str().map(str::to_owned), saved: cached }
        } else {
            let config = call(url, "getAccount", json!([registry::account_text(&config_address(&program))]), deadline)?;
            if config.is_null() {
                // The bridge was there on this chain and is gone: a reset by another name.
                if same_chain && cached["bridge"] == true {
                    reset = true;
                }
                let saved = json!({ "read_at": now, "program": text, "bridge": false, "token_program": null, "list": [], "problem": null });
                Bridge { program, present: false, token_program: None, tokens: Vec::new(), problem: None, saved }
            } else {
                match state_under(&config, &program).and_then(|bytes| decode_config(&bytes)) {
                    Ok((token_program, listed)) => {
                        // Kept for good, as a relayed call's check keeps it (S-1): a bridge removed
                        // before any relayed call was checked stays refused as root. Never a bridge
                        // program itself.
                        if !wanted_all.contains(&token_program) {
                            keep_token_programs(dir, &[registry::account_text(&token_program)]);
                        }
                        let mut tokens = Vec::new();
                        for token in &listed {
                            let definition = registry::account_text(&token.definition);
                            let name = call(url, "getAccount", json!([definition]), deadline)?;
                            let name = state_under(&name, &token_program).ok().and_then(|bytes| decode_definition_name(&bytes));
                            tokens.push(json!({ "bridge": text, "bridge_label": bridge_label(&text), "definition": definition, "eth_token": token.eth_token, "symbol": token.symbol(), "decimals": token.decimals, "name": name }));
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
    let (Some(marks), Some(at)) = (marks(read), read["read_at"].as_u64()) else {
        return Vec::new();
    };
    let mut added = Vec::new();
    for (scope, rows, about) in marks {
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
        assert!(DEFAULT_BRIDGE_PROGRAMS.len() <= MAX_BRIDGES && DEFAULT_BRIDGE_PROGRAMS.contains(&BRIDGE_V4_PROGRAM));
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
        // The same chain, later: no reset.
        HEIGHT.store(600, std::sync::atomic::Ordering::SeqCst);
        assert!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_100).unwrap()["reset_at"].is_null());
        // The same channel, a lower height: reset, and told for a day.
        HEIGHT.store(3, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_200).unwrap()["reset_at"].as_u64(), Some(1_200));
        HEIGHT.store(9, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_300).unwrap()["reset_at"].as_u64(), Some(1_200));
        assert!(read(&first, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 1_200 + RESET_TOLD_S).unwrap()["reset_at"].is_null());
        // Another channel: reset.
        let other = network("ab".repeat(32).leak(), &HEIGHT, true);
        assert_eq!(read(&other, &[id(DEFAULT_BRIDGE_PROGRAM)], &[id(OWNER)], &dir, 200_000).unwrap()["reset_at"].as_u64(), Some(200_000));
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
        let many: Vec<AccountId> = (0..6u8).map(|n| [n + 30; 32]).collect();
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
