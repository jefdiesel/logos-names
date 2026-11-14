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

/// The bridge test network's JSON-RPC endpoint (POST only, behind a filter).
pub const RPC: &str = "https://lez.nameswallet.app/";
/// The bridge program. Its Config lists the bridged tokens, so their definitions (new at every
/// re-genesis) are never written here. Expected to keep this id across a re-genesis (the
/// Sepolia vault's cfgId binds it).
pub const BRIDGE_PROGRAM: &str = "JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7";
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
}

impl Token {
    pub fn symbol(&self) -> String {
        SYMBOLS.iter().find(|(eth, _)| *eth == self.eth_token).map_or_else(|| self.eth_token.clone(), |(_, symbol)| (*symbol).to_owned())
    }
}

/// The bridge Config's token program and tokens (`Config` and `TokenEntry` in
/// `lez-bridge/bridge/src/lib.rs`). Fails on anything that isn't exactly one Config.
pub fn decode_config(bytes: &[u8]) -> Result<(AccountId, Vec<Token>), String> {
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
    r.flag()?; // burn_frozen
    let count = r.u32()?;
    if count > 32 {
        return Err("more tokens than a Config holds".to_owned());
    }
    let mut tokens = Vec::with_capacity(count);
    for _ in 0..count {
        let eth_token = format!("0x{}", hex(r.take(20)?));
        let definition = r.id()?;
        let decimals = r.u8()?;
        r.skip_vec(16)?; // denoms
        r.skip_vec(8)?; // claims
        r.skip_vec(8)?; // burns
        r.u128()?; // claimed_value
        r.u128()?; // burned_value
        r.flag()?; // registered
        r.flag()?; // mint_frozen
        r.flag()?; // burn_frozen
        if decimals > 38 {
            return Err("a token with more decimals than any amount has digits".to_owned());
        }
        tokens.push(Token { eth_token, definition, decimals });
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
    Ok((token_program, tokens))
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

/// Reads the bridge test network at `url` for `accounts`, remembering in `dir` what it saw (the
/// chain, its token list). `now` is in seconds. The answer:
/// `{network, version, channel, height, read_at, bridge, tokens: [{definition, symbol,
/// decimals, name}], token_problem, reset_at, accounts: [{account, found, native, tokens:
/// [{definition, symbol, decimals, amount}], other_tokens}]}`; amounts are decimal text in
/// base units. Any failed request fails the whole read: a balance never read is never shown.
pub fn read(url: &str, accounts: &[AccountId], dir: &Path, now: u64) -> Result<Value, String> {
    let deadline = Deadline::after(READ_BUDGET);
    let channel = call(url, "getChannelId", json!([]), deadline)?;
    let channel = channel.as_str().and_then(crate::zones::channel).ok_or("the bridge test network didn't say which chain it is")?;
    let height = call(url, "getLastBlockId", json!([]), deadline)?.as_u64().ok_or("the bridge test network gave no block height")?;
    let saved = load_state(dir);
    let mut reset = was_reset(&saved, &channel, height);
    let same_chain = !reset && saved["channel"].as_str() == Some(channel.as_str());

    // The token list: kept per chain for an hour, else read from the Config again.
    let program = registry::parse_account(BRIDGE_PROGRAM)?;
    let cached = saved["tokens"].clone();
    let cache_good = same_chain && cached["read_at"].as_u64().is_some_and(|at| now.saturating_sub(at) < TOKENS_FRESH_S) && cached["bridge"].is_boolean();
    let (bridge, token_program, tokens, token_problem, tokens_saved) = if cache_good {
        let token_program = cached["token_program"].as_str().and_then(|id| registry::parse_account(id).ok());
        let list = cached["list"].as_array().cloned().unwrap_or_default();
        (cached["bridge"] == true, token_program, list, cached["problem"].as_str().map(str::to_owned), cached)
    } else {
        let config = call(url, "getAccount", json!([registry::account_text(&config_address(&program))]), deadline)?;
        if config.is_null() {
            // The bridge was there on this chain and is gone: a reset by another name.
            if same_chain && saved["tokens"]["bridge"] == true {
                reset = true;
            }
            let saved_tokens = json!({ "read_at": now, "bridge": false, "token_program": null, "list": [], "problem": null });
            (false, None, Vec::new(), None, saved_tokens)
        } else {
            match state_under(&config, &program).and_then(|bytes| decode_config(&bytes)) {
                Ok((token_program, tokens)) => {
                    let mut list = Vec::new();
                    for token in &tokens {
                        let definition = registry::account_text(&token.definition);
                        let name = call(url, "getAccount", json!([definition]), deadline)?;
                        let name = state_under(&name, &token_program).ok().and_then(|bytes| decode_definition_name(&bytes));
                        list.push(json!({ "definition": definition, "eth_token": token.eth_token, "symbol": token.symbol(), "decimals": token.decimals, "name": name }));
                    }
                    let token_program = Some(token_program);
                    let saved_tokens = json!({ "read_at": now, "bridge": true, "token_program": token_program.map(|id| registry::account_text(&id)), "list": list, "problem": null });
                    (true, token_program, list, None, saved_tokens)
                }
                Err(_) => {
                    let why = "can't read the bridge's token list, so bridged tokens aren't shown".to_owned();
                    let saved_tokens = json!({ "read_at": now, "bridge": true, "token_program": null, "list": [], "problem": why });
                    (true, None, Vec::new(), Some(why), saved_tokens)
                }
            }
        }
    };

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
        let held = match token_program {
            Some(token_program) => decode_holdings(&state_under(&read, &token_program)?)
                .map_err(|why| format!("the bridge test network sent token holdings this version can't read ({why})"))?,
            None => BTreeMap::new(),
        };
        let listed: Vec<String> = tokens.iter().filter_map(|t| t["definition"].as_str().map(str::to_owned)).collect();
        let other = held.keys().filter(|definition| !listed.contains(&registry::account_text(definition))).count();
        let token_rows: Vec<Value> = tokens
            .iter()
            .map(|t| {
                let amount = t["definition"].as_str().and_then(|d| registry::parse_account(d).ok()).and_then(|d| held.get(&d).copied()).unwrap_or(0);
                held_row(t, amount)
            })
            .collect();
        rows.push(json!({ "account": format!("Public/{text}"), "found": true, "native": native.to_string(), "tokens": token_rows, "other_tokens": other }));
    }

    let reset_at = if reset {
        Some(now)
    } else {
        saved["reset_at"].as_u64().filter(|at| same_chain && now.saturating_sub(*at) < RESET_TOLD_S)
    };
    save_state(dir, &json!({ "channel": channel, "height": height, "reset_at": reset_at, "tokens": tokens_saved }));
    Ok(json!({
        "network": NAME,
        "version": VERSION,
        "channel": channel,
        "height": height,
        "read_at": now,
        "bridge": bridge,
        "tokens": tokens,
        "token_problem": token_problem,
        "reset_at": reset_at,
        "accounts": rows,
    }))
}

fn held_row(token: &Value, amount: u128) -> Value {
    json!({ "definition": token["definition"], "symbol": token["symbol"], "decimals": token["decimals"], "name": token["name"], "amount": amount.to_string() })
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
        bytes_of(&fixture("config.json")["result"]["data"]["actor_states"][BRIDGE_PROGRAM]).expect("bytes")
    }

    #[test]
    fn the_config_address_is_the_live_one() {
        assert_eq!(registry::account_text(&config_address(&id(BRIDGE_PROGRAM))), "HugSoWM7Hp1eXMqknfCewWNMoraweLG3HkezfNGtqKf3");
    }

    #[test]
    fn the_live_config_gives_both_tokens_with_their_decimals() {
        let (program, tokens) = decode_config(&config_bytes()).expect("decodes");
        assert_eq!(registry::account_text(&program), TOKEN_PROGRAM);
        let shown: Vec<(String, String, u8)> = tokens.iter().map(|t| (t.symbol(), registry::account_text(&t.definition), t.decimals)).collect();
        assert_eq!(shown, vec![("dUSDC".to_owned(), DUSDC.to_owned(), 6), ("wETH".to_owned(), WETH.to_owned(), 18)]);
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
        let read = read(&url, &[id(OWNER), id(UNSEEN)], &dir, 1_000).expect("reads");
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
        assert!(read(&first, &[id(OWNER)], &dir, 1_000).unwrap()["reset_at"].is_null());
        // The same chain, later: no reset.
        HEIGHT.store(600, std::sync::atomic::Ordering::SeqCst);
        assert!(read(&first, &[id(OWNER)], &dir, 1_100).unwrap()["reset_at"].is_null());
        // The same channel, a lower height: reset, and told for a day.
        HEIGHT.store(3, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(&first, &[id(OWNER)], &dir, 1_200).unwrap()["reset_at"].as_u64(), Some(1_200));
        HEIGHT.store(9, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(read(&first, &[id(OWNER)], &dir, 1_300).unwrap()["reset_at"].as_u64(), Some(1_200));
        assert!(read(&first, &[id(OWNER)], &dir, 1_200 + RESET_TOLD_S).unwrap()["reset_at"].is_null());
        // Another channel: reset.
        let other = network("ab".repeat(32).leak(), &HEIGHT, true);
        assert_eq!(read(&other, &[id(OWNER)], &dir, 200_000).unwrap()["reset_at"].as_u64(), Some(200_000));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_chain_without_the_bridge_shows_native_only_and_its_loss_is_a_reset() {
        static HEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(10);
        let dir = scratch("nobridge");
        let with = network(CHANNEL, &HEIGHT, true);
        read(&with, &[id(OWNER)], &dir, 1_000).unwrap();
        HEIGHT.store(20, std::sync::atomic::Ordering::SeqCst);
        let without = network(CHANNEL, &HEIGHT, false);
        // The token list is kept for an hour; past it the Config is read again and is gone.
        let read = read(&without, &[id(OWNER)], &dir, 1_000 + TOKENS_FRESH_S).unwrap();
        assert_eq!((read["bridge"].as_bool(), read["reset_at"].as_u64()), (Some(false), Some(1_000 + TOKENS_FRESH_S)));
        assert_eq!(read["accounts"][0]["native"], "23786858");
        assert!(read["accounts"][0]["tokens"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreachable_network_fails_the_read_in_words() {
        let dir = scratch("down");
        let url = registry::stub_rpc(|_| (502, json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32603, "message": "sequencer unavailable" } }).to_string()));
        let why = read(&url, &[id(OWNER)], &dir, 1).unwrap_err();
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
        let read = read(RPC, &[id(OWNER), id(UNSEEN)], &dir, 1).expect("the bridge test network answers");
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
        let first = read(&url, &[id(OWNER)], &dir, 1_000).unwrap();
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
