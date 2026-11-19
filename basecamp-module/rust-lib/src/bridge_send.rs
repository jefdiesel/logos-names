//! Moving wrapped tokens on the bridge test network (LEZ 0.4) between public accounts
//! (`bridge_token_send`; rfp-022 `proposal/WALLET-ROADMAP.md` item 12).
//!
//! A wrapped token (wETH of bridge v5, say) sits in its holder's actor of the bridge's token
//! program, which the bridge's Config names. Moving it is the token program's `Transfer`, signed
//! by the account it leaves; nothing of the bridge runs. The names CLI sends it
//! (`names token send`) with `--token-codec=bridge`: the bridge token program's definitions carry
//! a typed authority the prototype codec can't read (cli/src/tokens.rs `BridgeCodec`). A CLI
//! built before that codec refuses with "no token codec called bridge", said here as a tool too
//! old for this.
//!
//! Who pays: the bridge's relay pays only for claims and burns (rfp-022
//! `public-demo/relayer/relayer.py`: `/relay` and `/relay/burn`), so the account the tokens leave
//! pays the fee in test LEZ, as every public change on LEZ 0.4 does. Before the person approves,
//! the same command runs with `LOGOS_NAMES_DRY_RUN` (a "check"): the CLI simulates the transfer,
//! prices the fee (`fee: about …, at most …`) and checks the payer can cover it, and sends
//! nothing. A check that says the account is short ends there, in plain words, with the faucet as
//! the way out.
//!
//! Everything the call depends on comes from the chain, never from the app: the token program
//! and the token's decimals from the bridge's Config; the definition must be one the Config
//! lists. Only public accounts: a private account's side is not in the wallet on LEZ 0.4 yet.

use crate::bridge_net::{self, BridgeConfig, Token};
use crate::registry::{self, AccountId};
use serde_json::{json, Value};

/// The `--token-codec` value for the bridge's token program.
pub const CODEC: &str = "bridge";
/// The job kinds: the dry run before the sheet, and the transfer.
pub const CHECK_KIND: &str = "bridge_send_check";
pub const SEND_KIND: &str = "bridge_send";
/// What the CLI says when it ran a check and stopped before sending (cli/src/tokens.rs).
const DRY_RUN_MARK: &str = "LOGOS_NAMES_DRY_RUN is set";

/// Where the tokens go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    /// A public account, the wallet's own or anyone's the chain has seen.
    Account(AccountId),
    /// An LNS name: its public account on LEZ 0.4.
    Name(String),
}

impl To {
    /// As the CLI takes it after `--to=`.
    pub fn arg(&self) -> String {
        match self {
            To::Account(id) => format!("Public/{}", registry::account_text(id)),
            To::Name(name) => name.clone(),
        }
    }
}

/// One move, checked: everything the command and the sheet need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub bridge: AccountId,
    pub token_program: AccountId,
    pub definition: AccountId,
    pub symbol: String,
    pub decimals: u8,
    pub amount: u128,
    pub from: AccountId,
    pub to: To,
}

/// An amount as a person writes it in the token's own unit (`1.2` wETH), in base units:
/// digits, at most one `.`, no more decimal places than the token has. Above 0, below 2^128.
pub fn units(text: &str, decimals: u8) -> Result<u128, String> {
    let typed = text.trim();
    if typed.is_empty() {
        return Err("Enter an amount, such as 0.01.".to_owned());
    }
    if typed.starts_with('-') {
        return Err(format!("{typed} is negative. Enter an amount above 0."));
    }
    let (whole, fraction) = typed.split_once('.').unwrap_or((typed, ""));
    let digits_only = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if !digits_only(whole) || !digits_only(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return Err(format!("{typed} isn't an amount. Use digits and one point, such as 0.01."));
    }
    if fraction.len() > usize::from(decimals) {
        return Err(if decimals == 0 {
            format!("{typed} has a fraction, and this token has no smaller unit. Enter a whole number.")
        } else {
            format!("{typed} has more than {decimals} digits after the point, the smallest unit of this token.")
        });
    }
    let padded = format!("{whole}{fraction}{}", "0".repeat(usize::from(decimals) - fraction.len()));
    let digits = padded.trim_start_matches('0');
    if digits.is_empty() {
        return Err("Enter an amount above 0.".to_owned());
    }
    digits.parse().map_err(|_| format!("{typed} is more than an amount can be."))
}

/// Base units shown in the token's unit, without trailing zeros: (1_200_000_000_000_000_000, 18) → "1.2".
pub fn shown(amount: u128, decimals: u8) -> String {
    let places = usize::from(decimals);
    let mut digits = amount.to_string();
    if places == 0 {
        return digits;
    }
    if digits.len() <= places {
        digits = format!("{}{digits}", "0".repeat(places + 1 - digits.len()));
    }
    let (whole, fraction) = digits.split_at(digits.len() - places);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() { whole.to_owned() } else { format!("{whole}.{fraction}") }
}

/// The account the tokens leave: one public account, written `Public/<id>` (or bare).
pub fn parse_from(text: &str) -> Result<AccountId, String> {
    let text = text.trim();
    if text.starts_with("Private/") {
        return Err("Moving wrapped tokens from a private account isn't in the wallet yet: choose a public account.".to_owned());
    }
    bridge_net::public_account(text).map_err(|_| "Choose the public account the tokens leave from.".to_owned())
}

/// Where they go: `Public/<id>` (or a bare ID, which on this screen is always a public
/// account), or an LNS name. A private account is refused, as is the account they leave.
pub fn parse_to(text: &str, from: &AccountId) -> Result<To, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Choose where the tokens go: one of your accounts, an account ID or an LNS name.".to_owned());
    }
    if text.starts_with("Private/") {
        return Err("Moving wrapped tokens to a private account isn't in the wallet yet: choose a public account.".to_owned());
    }
    let bare = text.strip_prefix("Public/").unwrap_or(text);
    let to = match registry::bare_account(bare) {
        // A send can't be undone: an ID that was most likely mangled on the way is refused.
        Ok(id) => {
            registry::plausible_holder(bare, &id).map_err(|why| format!("{}{}.", why[..1].to_uppercase(), &why[1..]))?;
            To::Account(id)
        }
        Err(_) if text.starts_with("Public/") => return Err("That isn't an account ID: it's Public/ and then letters and digits.".to_owned()),
        Err(_) => To::Name(registry::parse_name(text).map_err(|_| "That isn't an account ID or an LNS name.".to_owned())?),
    };
    if to == To::Account(*from) {
        return Err("That's the account the tokens leave from. Choose another one.".to_owned());
    }
    Ok(to)
}

/// The token `definition` among the bridge's own: a holding the Config doesn't list is not a
/// bridged token, whatever it is called (rfp-022 `public-demo/WALLET-READ.md`).
pub fn token_of<'a>(config: &'a BridgeConfig, definition: &AccountId) -> Result<&'a Token, String> {
    config
        .tokens
        .iter()
        .find(|token| token.definition == *definition)
        .ok_or_else(|| "That token isn't one this bridge made, so the wallet doesn't move it here.".to_owned())
}

/// Checks everything the person gave against the bridge's Config.
pub fn checked(config: &BridgeConfig, bridge: AccountId, definition: &str, amount: &str, from: &str, to: &str) -> Result<Move, String> {
    let definition = registry::bare_account(definition.trim()).map_err(|_| "That isn't a token: a token is its definition's account ID.".to_owned())?;
    let token = token_of(config, &definition)?;
    let from = parse_from(from)?;
    let to = parse_to(to, &from)?;
    let amount = units(amount, token.decimals)?;
    Ok(Move { bridge, token_program: config.token_program, definition, symbol: token.symbol(), decimals: token.decimals, amount, from, to })
}

/// The CLI's arguments: `names token send` with the bridge's token program and codec, from the
/// chosen account. A name is paid at its public account (`--public`).
pub fn args(it: &Move) -> Vec<String> {
    let mut args = vec![
        format!("--token-program={}", registry::account_text(&it.token_program)),
        format!("--token-codec={CODEC}"),
        "token".to_owned(),
        "send".to_owned(),
        format!("--to={}", it.to.arg()),
        format!("--from=Public/{}", registry::account_text(&it.from)),
    ];
    if matches!(it.to, To::Name(_)) {
        args.push("--public".to_owned());
    }
    args.extend(["--".to_owned(), registry::account_text(&it.definition), it.amount.to_string()]);
    args
}

/// What the sheet shows about the move (the fee comes from the check's job).
pub fn plan(it: &Move) -> Value {
    let label = bridge_net::bridge_label(&registry::account_text(&it.bridge));
    json!({
        "bridge": registry::account_text(&it.bridge),
        "bridge_label": label,
        "token_program": registry::account_text(&it.token_program),
        "definition": registry::account_text(&it.definition),
        "symbol": it.symbol,
        "decimals": it.decimals,
        "amount": it.amount.to_string(),
        "amount_shown": shown(it.amount, it.decimals),
        "from": format!("Public/{}", registry::account_text(&it.from)),
        "to": it.to.arg(),
        "to_name": match &it.to { To::Name(name) => Value::from(name.clone()), To::Account(_) => Value::Null },
        // The account the tokens leave pays the fee: the relay pays only for claims and burns.
        "fee_payer": format!("Public/{}", registry::account_text(&it.from)),
        "relay_pays": false,
    })
}

/// A number the CLI wrote, grouped for reading (`7142927` → `7,142,927`).
fn grouped(digits: &str) -> String {
    let digits = digits.trim();
    let mut out = String::new();
    for (at, ch) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// An account as the wallet shows it in a sentence: `CCVd76…pKnk`.
fn short(account: &str) -> String {
    let id = account.trim().trim_start_matches("Public/").trim_start_matches("Private/");
    let chars: Vec<char> = id.chars().collect();
    if chars.len() > 12 { format!("{}…{}", chars[..6].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>()) } else { id.to_owned() }
}

/// The first `Error:` line of a CLI run, without the prefix.
fn error_line(log: &str) -> Option<&str> {
    log.lines().map(str::trim).find_map(|line| line.strip_prefix("Error:")).map(str::trim)
}

/// Why a check or a move didn't go through, in the wallet's words, from what the CLI printed.
/// The CLI's own text is shown only when none of the known cases fits. A move that was SENT (its
/// log has `submitted <hash>`) but not seen applied is never said as "didn't go through": it was
/// refused on chain with its fee paid, or it may still land.
pub fn problem(log: &str) -> String {
    let sent = log.lines().any(|l| l.trim().starts_with("submitted "));
    let applied = log.lines().any(|l| l.trim().starts_with("applied in block "));
    if sent && !applied {
        return if log.contains("but reverted") {
            "The network refused the move: nothing moved, but its fee was paid from the account it left.".to_owned()
        } else {
            "The move was sent but isn't confirmed yet. It may still go through: check the balance on the card before moving again.".to_owned()
        };
    }
    let error = error_line(log).unwrap_or_default();
    let says = |mark: &str| log.contains(mark);
    if says("no token codec called bridge") {
        return "This wallet's LEZ 0.4 names tool can't move wrapped tokens yet: it needs a newer tool. Nothing was sent.".to_owned();
    }
    // check_payer: "Public/X holds B; about R is held for the fee while this runs" (or "this takes S, and about R more …").
    if error.contains("is held for the fee") {
        let held = error.split(" holds ").nth(1).and_then(|rest| rest.split(';').next()).unwrap_or("").trim();
        let fee = error.split("about ").nth(1).and_then(|rest| rest.split_whitespace().next()).unwrap_or("").trim();
        let who = error.split_whitespace().next().map_or_else(|| "This account".to_owned(), short);
        return if held.is_empty() || fee.is_empty() {
            format!("{who} hasn't enough test LEZ for the fee. Get test LEZ for it first. Nothing was sent.")
        } else if held == "0" {
            format!("{who} has no test LEZ, and the fee needs about {} test LEZ. Get test LEZ for it first (Get test LEZ, on the bridge test network card). Nothing was sent.", grouped(fee))
        } else {
            format!("{who} holds {} test LEZ, and the fee needs about {}. Get test LEZ for it first (Get test LEZ, on the bridge test network card). Nothing was sent.", grouped(held), grouped(fee))
        };
    }
    if error.contains("fees are too high") {
        return "The bridge test network's fees are too high right now. Nothing was sent; try again later.".to_owned();
    }
    // check_spend: "Public/X holds N <token name>, less than M".
    if error.contains(", less than ") {
        let who = error.split_whitespace().next().map_or_else(|| "This account".to_owned(), short);
        return format!("{who} doesn't hold that much of this token. Nothing was sent.");
    }
    if error.contains("would be sending to itself") {
        return "That's the account the tokens leave from. Choose another one. Nothing was sent.".to_owned();
    }
    if error.contains("does not hold the key") {
        return "This wallet doesn't hold the key of the account the tokens leave from. Nothing was sent.".to_owned();
    }
    if says("UnadmittedPublicActor") || says("has not been seen") {
        return "That account has never been used on the bridge test network and isn't this wallet's, so it can't be paid yet. Nothing was sent.".to_owned();
    }
    if error.contains("is not registered") || error.contains("no public account") {
        return "That LNS name has no public account on LEZ 0.4 to send to. Nothing was sent.".to_owned();
    }
    format!("The move didn't go through: {}", crate::cli::failure_cause(log))
}

/// A check's ended job, as the app reads it: `ready` when the CLI priced it and stopped only
/// because it was a check; otherwise `problem`. A check is a run that ends in an error by
/// design, so `state` says how the CHECK went, not the CLI's exit.
pub fn check_outcome(log: &str, running: bool) -> Value {
    if running {
        return json!({});
    }
    if log.contains(DRY_RUN_MARK) {
        // The account the move pays, as the check resolved it (an LNS name: its public account, after
        // its forward if it has one), so the send can go to exactly that account.
        json!({ "state": "done", "ready": true, "problem": Value::Null, "to_account": paid_account(log), "forwarded_to": forwarded_to(log) })
    } else {
        json!({ "state": "failed", "ready": false, "problem": problem(log) })
    }
}

/// The account a token send's log says it pays: the `sending … to <who>` line's `Public/<id>`
/// (`<name> (Public/<id>)` for a name, `Public/<id>` for an account), or None.
pub fn paid_account(log: &str) -> Option<String> {
    let line = log.lines().map(str::trim).find(|l| l.starts_with("sending ") && l.contains(" to "))?;
    let to = line.rsplit_once(" to ")?.1.trim();
    let id = to.rsplit_once("(Public/").map(|(_, rest)| rest.trim_end_matches(')')).or_else(|| to.strip_prefix("Public/"))?;
    registry::bare_account(id).ok().map(|account| format!("Public/{}", registry::account_text(&account)))
}

/// The name an LNS name forwards to, from the CLI's `<name> → <target> (it forwards there)` line.
pub fn forwarded_to(log: &str) -> Option<String> {
    log.lines().map(str::trim).find_map(|l| l.strip_suffix(" (it forwards there)")?.split_once(" → ").map(|(_, target)| crate::text::clean_text(target, 64)))
}

// ── One send per check (review 2026-11-19: a send whose answer came late was approved again) ──
//
// The module keeps, per move (bridge, token, amount, from), when it was last checked and when it
// was last sent. A send needs a check newer than the last send of the same move, and at most
// `CHECK_FRESH` old: the same move is never sent twice from one review, whatever the app does. The
// recipient is not in the key: a check of a name is sent to the account it resolved to.

/// How long a check stays good for its send.
pub const CHECK_FRESH: std::time::Duration = std::time::Duration::from_secs(15 * 60);

fn ledger() -> std::sync::MutexGuard<'static, Vec<(String, Option<std::time::Instant>, Option<std::time::Instant>)>> {
    static LEDGER: std::sync::Mutex<Vec<(String, Option<std::time::Instant>, Option<std::time::Instant>)>> = std::sync::Mutex::new(Vec::new());
    LEDGER.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The move's key in the ledger.
pub fn move_key(it: &Move) -> String {
    format!("{}|{}|{}|{}", registry::account_text(&it.bridge), registry::account_text(&it.definition), it.amount, registry::account_text(&it.from))
}

/// A check of the move started now.
pub fn checked_now(key: &str, now: std::time::Instant) {
    let mut l = ledger();
    match l.iter_mut().find(|(k, _, _)| k == key) {
        Some(entry) => entry.1 = Some(now),
        None => {
            if l.len() >= 64 {
                l.remove(0);
            }
            l.push((key.to_owned(), Some(now), None));
        }
    }
}

/// Whether the move may be sent now, and if so records the send: only after a check of it within
/// `CHECK_FRESH`, newer than its last send.
pub fn may_send(key: &str, now: std::time::Instant) -> Result<(), String> {
    let mut l = ledger();
    let entry = l.iter_mut().find(|(k, _, _)| k == key);
    let Some(entry) = entry else {
        return Err("Review the move first: the wallet sends only a move it has just checked. Nothing was sent.".to_owned());
    };
    match (entry.1, entry.2) {
        (Some(checked), sent) if sent.is_none_or(|sent| checked > sent) && now.duration_since(checked) <= CHECK_FRESH => {
            entry.2 = Some(now);
            Ok(())
        }
        (_, Some(_)) => Err("This move was sent already from this review. Check the balance on the card, then review it again to send another. Nothing was sent.".to_owned()),
        _ => Err("This review is too old. Review the move again. Nothing was sent.".to_owned()),
    }
}

/// A move's ended job: its transaction and block (as for every token send), and, if it failed,
/// why in plain words.
pub fn send_outcome(log: &str, running: bool, done: bool) -> Value {
    if running {
        return json!({});
    }
    let mut out = crate::cli::token_outcome(log);
    if !done {
        out["problem"] = Value::from(problem(log));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> AccountId {
        [byte; 32]
    }

    fn config() -> BridgeConfig {
        BridgeConfig {
            token_program: id(9),
            burn_frozen: false,
            tokens: vec![Token {
                eth_token: "0x0000000000000000000000000000000000000000".to_owned(),
                definition: id(5),
                decimals: 18,
                denoms: vec![10_000_000_000_000_000],
                registered: true,
                burn_frozen: false,
            }],
        }
    }

    #[test]
    fn amounts_are_read_in_the_tokens_unit_without_floating_point() {
        assert_eq!(units("1.2", 18), Ok(1_200_000_000_000_000_000));
        assert_eq!(units("0.01", 18), Ok(10_000_000_000_000_000));
        assert_eq!(units(" 1 ", 18), Ok(1_000_000_000_000_000_000));
        assert_eq!(units(".5", 18), Ok(500_000_000_000_000_000));
        assert_eq!(units("5.", 18), Ok(5_000_000_000_000_000_000));
        assert_eq!(units("0.000000000000000001", 18), Ok(1));
        assert!(units("0.0000000000000000001", 18).unwrap_err().contains("more than 18 digits"));
        assert_eq!(units("0", 18), Err("Enter an amount above 0.".to_owned()));
        assert_eq!(units("0.000", 18), Err("Enter an amount above 0.".to_owned()));
        assert!(units("", 18).is_err());
        assert!(units("-1", 18).unwrap_err().contains("negative"));
        assert!(units("1,5", 18).unwrap_err().contains("isn't an amount"));
        assert!(units("1.2.3", 18).is_err());
        assert!(units(".", 18).is_err());
        assert!(units("1e3", 18).is_err());
        assert!(units("0.5", 0).unwrap_err().contains("no smaller unit"));
        assert_eq!(units("7", 0), Ok(7));
        assert!(units("999999999999999999999", 18).unwrap_err().contains("more than an amount"));
    }

    #[test]
    fn amounts_come_back_as_written() {
        assert_eq!(shown(1_200_000_000_000_000_000, 18), "1.2");
        assert_eq!(shown(10_000_000_000_000_000, 18), "0.01");
        assert_eq!(shown(1, 18), "0.000000000000000001");
        assert_eq!(shown(2_000_000_000_000_000_000, 18), "2");
        assert_eq!(shown(7, 0), "7");
        for text in ["1.2", "0.01", "3", "0.123456789012345678"] {
            assert_eq!(shown(units(text, 18).unwrap(), 18), text);
        }
    }

    #[test]
    fn only_public_accounts_and_names_and_never_the_same_account() {
        let from = id(1);
        let other = registry::account_text(&id(2));
        assert_eq!(parse_to(&format!("Public/{other}"), &from), Ok(To::Account(id(2))));
        assert_eq!(parse_to(&other, &from), Ok(To::Account(id(2))));
        assert_eq!(parse_to("alice", &from), Ok(To::Name("alice".to_owned())));
        assert!(parse_to(&format!("Private/{other}"), &from).unwrap_err().contains("private account isn't in the wallet yet"));
        assert!(parse_to(&format!("Public/{}", registry::account_text(&from)), &from).unwrap_err().contains("leave from"));
        assert!(parse_to("", &from).is_err());
        assert!(parse_to("Public/nope", &from).is_err());
        assert!(parse_from(&format!("Private/{other}")).unwrap_err().contains("private account"));
        assert_eq!(parse_from(&format!("Public/{other}")), Ok(id(2)));
    }

    #[test]
    fn the_token_must_be_one_the_bridge_lists_and_its_decimals_come_from_the_config() {
        let config = config();
        let from = format!("Public/{}", registry::account_text(&id(1)));
        let to = format!("Public/{}", registry::account_text(&id(2)));
        let it = checked(&config, id(7), &registry::account_text(&id(5)), "1.2", &from, &to).unwrap();
        assert_eq!(it.amount, 1_200_000_000_000_000_000);
        assert_eq!(it.symbol, "wETH");
        assert_eq!(it.token_program, id(9), "the token program is the Config's, never the app's");
        let stranger = checked(&config, id(7), &registry::account_text(&id(6)), "1", &from, &to).unwrap_err();
        assert!(stranger.contains("isn't one this bridge made"));
        assert!(checked(&config, id(7), "nope", "1", &from, &to).unwrap_err().contains("isn't a token"));
    }

    #[test]
    fn the_command_is_the_bridge_codec_from_the_chosen_account() {
        let config = config();
        let from = format!("Public/{}", registry::account_text(&id(1)));
        let to = format!("Public/{}", registry::account_text(&id(2)));
        let it = checked(&config, id(7), &registry::account_text(&id(5)), "0.5", &from, &to).unwrap();
        assert_eq!(
            args(&it),
            vec![
                format!("--token-program={}", registry::account_text(&id(9))),
                "--token-codec=bridge".to_owned(),
                "token".to_owned(),
                "send".to_owned(),
                format!("--to={to}"),
                format!("--from={from}"),
                "--".to_owned(),
                registry::account_text(&id(5)),
                "500000000000000000".to_owned(),
            ]
        );
        let named = checked(&config, id(7), &registry::account_text(&id(5)), "0.5", &from, "alice").unwrap();
        let named = args(&named);
        assert!(named.contains(&"--to=alice".to_owned()) && named.contains(&"--public".to_owned()));
        let shown = plan(&it);
        assert_eq!(shown["fee_payer"], from.as_str());
        assert_eq!(shown["relay_pays"], false);
        assert_eq!(shown["amount_shown"], "0.5");
    }

    #[test]
    fn a_check_that_stopped_before_sending_is_ready() {
        let log = "fee: about 2140, at most 3000 (gas: 100 cycles, limit 200)\nfee reserve: 3000 from Public/A, plus 0 the call moves\nError: LOGOS_NAMES_DRY_RUN is set, so nothing was sent\n";
        assert_eq!(check_outcome(log, false), json!({ "state": "done", "ready": true, "problem": null, "to_account": null, "forwarded_to": null }));
        assert_eq!(crate::cli::parse_fee(log)["about"], "2140");
        assert_eq!(check_outcome(log, true), json!({}));
    }

    #[test]
    fn a_sent_move_is_never_said_as_failed_plainly() {
        let reverted = "sending 5 wETH from Public/A to Public/B\nsubmitted ab12\nError: the transaction was not applied\n\nCaused by:\n    ab12 was included in block 9 but reverted: its fee was charged and nothing else applied\n";
        assert!(problem(reverted).starts_with("The network refused the move: nothing moved, but its fee was paid"));
        for later in [
            "submitted ab12\nError: the transaction was not applied\n\nCaused by:\n    ab12 was not applied: it was not in a block after 120 s; a slow chain may still apply it\n",
            "submitted ab12\nError: the transaction was not applied\n\nCaused by:\n    0: couldn't poll\n    1: Connection refused (os error 61)\n",
            "submitted ab12\nError: the transaction was not applied\n\nCaused by:\n    block 5 is missing\n",
        ] {
            let said = problem(later);
            assert!(said.starts_with("The move was sent but isn't confirmed yet"), "{said}");
            assert!(!said.contains("didn't go through") && !said.contains("try again"), "{said}");
        }
        let out = send_outcome(reverted, false, false);
        assert_eq!(out["tx"], "ab12", "the hash stays with the outcome");
        // Not sent at all: the plain cases still apply.
        assert!(problem("Error: Public/A would be sending to itself").contains("leave from"));
    }

    #[test]
    fn the_check_says_which_account_a_name_pays_and_its_forward() {
        let other = registry::account_text(&id(2));
        let named = format!("fee: about 2140, at most 3000 (gas)\nalice → bob (it forwards there)\nsending 500 Wrapped Sepolia ETH (v5) from Public/A to alice (Public/{other})\nError: LOGOS_NAMES_DRY_RUN is set, so nothing was sent\n");
        let c = check_outcome(&named, false);
        assert_eq!((c["to_account"].as_str(), c["forwarded_to"].as_str()), (Some(format!("Public/{other}").as_str()), Some("bob")));
        let plain = format!("sending 500 wETH from Public/A to Public/{other}\nError: LOGOS_NAMES_DRY_RUN is set, so nothing was sent\n");
        assert_eq!(check_outcome(&plain, false)["to_account"], format!("Public/{other}"));
        assert!(check_outcome(&plain, false)["forwarded_to"].is_null());
        assert!(check_outcome("Error: LOGOS_NAMES_DRY_RUN is set, so nothing was sent", false)["to_account"].is_null());
        assert_eq!(paid_account("sending 5 x from Public/A to alice (Public/nope)"), None);
    }

    #[test]
    fn one_send_per_check() {
        let t0 = std::time::Instant::now();
        let key = "test-move|one-send-per-check";
        assert!(may_send(key, t0).unwrap_err().contains("Review the move first"));
        checked_now(key, t0);
        assert_eq!(may_send(key, t0 + std::time::Duration::from_secs(5)), Ok(()));
        // Pressed again (a late answer, a second press): refused until a new review.
        assert!(may_send(key, t0 + std::time::Duration::from_secs(40)).unwrap_err().contains("sent already"));
        checked_now(key, t0 + std::time::Duration::from_secs(60));
        assert_eq!(may_send(key, t0 + std::time::Duration::from_secs(61)), Ok(()));
        // A review older than CHECK_FRESH doesn't send.
        let other = "test-move|old-review";
        checked_now(other, t0);
        assert!(may_send(other, t0 + CHECK_FRESH + std::time::Duration::from_secs(1)).unwrap_err().contains("too old"));
    }

    #[test]
    fn refusals_are_said_plainly() {
        let short = "fee: about 2140, at most 3000 (gas)\nError: Public/AbC holds 0; about 3000 is held for the fee while this runs\n";
        let said = check_outcome(short, false);
        assert_eq!(said["ready"], false);
        assert_eq!(said["problem"], "AbC has no test LEZ, and the fee needs about 3,000 test LEZ. Get test LEZ for it first (Get test LEZ, on the bridge test network card). Nothing was sent.");
        let some = "Error: Public/AbC holds 1200; about 3000 is held for the fee while this runs";
        assert!(problem(some).starts_with("AbC holds 1,200 test LEZ, and the fee needs about 3,000."));
        assert_eq!(problem("Error: no token codec called bridge; known: prototype"), "This wallet's LEZ 0.4 names tool can't move wrapped tokens yet: it needs a newer tool. Nothing was sent.");
        assert!(problem("Error: Public/AbC holds 5 Wrapped Sepolia ETH (v5), less than 9").contains("doesn't hold that much"));
        assert!(problem("Error: Public/A would be sending to itself").contains("leave from"));
        assert!(problem("Error: this wallet does not hold the key of AbC").contains("doesn't hold the key"));
        assert!(problem("Error: the sequencer did not accept the transaction\n  0: UnadmittedPublicActor").contains("never been used"));
        assert!(problem("Error: Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk holds 0; about 3000 is held for the fee while this runs").starts_with("CCVd76…pKnk has no test LEZ"));
        assert!(problem("Error: something new\n").starts_with("The move didn't go through: something new"));
        // A move that failed says why; one that went through has its block.
        let failed = send_outcome("Error: no token codec called bridge; known: prototype", false, false);
        assert!(failed["problem"].as_str().unwrap().contains("newer tool"));
        let sent = send_outcome("sending 5 wETH from Public/A to Public/B\nsubmitted ab12\napplied in block 28500\n", false, true);
        assert_eq!(sent["tx"], "ab12");
        assert_eq!(sent["block"], 28500);
        assert!(sent["problem"].is_null());
    }
}
