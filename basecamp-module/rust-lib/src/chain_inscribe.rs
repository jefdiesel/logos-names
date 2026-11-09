//! A testimonial inscribed on the Logos Blockchain (LP-0022), through Basecamp's own
//! node: a Mantle `CHANNEL_INSCRIBE` op, its fee funded from a chain account the person
//! picks.
//!
//! The node (blockchain_module 0.3.4) has no inscribe method, so the transaction is put
//! together here and the node does what only it can:
//! 1. `wallet_fund_tx` takes the unsigned inscription and appends a transfer paying its
//!    fee from the chosen key (change back to it), signed by the node over the funded
//!    transaction's hash. It moves and reserves nothing (two funds in a row pick the
//!    same note).
//! 2. The hash is computed here, as the node does: blake2b-256 of `MANTLE_TXHASH_V1`
//!    and the ops' canonical encoding. The inscription is signed with the account's
//!    Ed25519 inscription key over that hash.
//! 3. `submit_signed_transaction` takes the ops and both proofs; it answers the hash it
//!    computed, which must be the one signed.
//!
//! Each chain account gets its own Ed25519 inscription key, made here and kept in the
//! module's folder (0600), never in a reply or a log. Its testimonial channel is
//! `sha256("lp-0022:testimonial:" || public key)`: the first inscription creates it
//! (parent zero) with that key as its only writer, later ones chain on its tip.

use crate::{chain, node};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Map, Value};
use sha2::Sha256;
use std::path::Path;

/// What the inscription says it is: a testimonial for this submission.
pub const SUBMISSION: &str = "lp-0022:names-wallet";
/// The testimonial channel of a key is the SHA-256 of this and the key.
pub const CHANNEL_DOMAIN: &[u8] = b"lp-0022:testimonial:";
/// At most this much text, as the LEZ testimonial program allows (testimonial/core).
pub const MAX_TEXT: usize = 1024;
/// The fee the node may fund, at most (as a bridge's).
pub const MAX_FEE: u64 = 100_000;
/// Where the inscription keys are kept, in the module's folder.
pub const KEYS_FILE: &str = "chain-inscribe-keys.json";
/// The parent of a channel's first message.
pub const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const OP_TRANSFER: u64 = 0x00;
const OP_INSCRIBE: u64 = 0x11;
const TX_HASH_PREFIX: &[u8] = b"MANTLE_TXHASH_V1";

type Blake2b256 = Blake2b<U32>;

/// Testimonial text as the LEZ testimonial program takes it: 1-1024 bytes, not only
/// spaces, no control characters but line breaks and tabs.
pub fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("a testimonial needs some text".into());
    }
    if text.len() > MAX_TEXT {
        return Err(format!("a testimonial holds at most {MAX_TEXT} bytes of text"));
    }
    if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err("a testimonial has no control characters other than line breaks and tabs".into());
    }
    Ok(())
}

/// The inscription's bytes: `{"submission":…,"text":…}`.
pub fn inscription(text: &str) -> Vec<u8> {
    json!({ "submission": SUBMISSION, "text": text }).to_string().into_bytes()
}

/// The testimonial channel of an inscription key.
pub fn channel_of(signer: &[u8; 32]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CHANNEL_DOMAIN);
    hasher.update(signer);
    hex(&hasher.finalize())
}

/// The unsigned `CHANNEL_INSCRIBE` op, as the node's JSON writes it.
pub fn inscribe_op(channel: &str, inscription: &[u8], parent: &str, signer: &[u8; 32]) -> Value {
    json!({ "opcode": OP_INSCRIBE, "payload": {
        "channel_id": channel, "inscription": hex(inscription), "parent": parent, "signer": hex(signer) } })
}

/// The request `wallet_fund_tx` takes: `op` alone, its fee paid from `from`, change back to it.
pub fn fund_request(op: &Value, from: &str) -> Value {
    json!({
        "tip": null,
        "tx_builder": { "mantle_tx": { "ops": [op] }, "ledger_inputs": [], "pending_transfer": { "inputs": [], "outputs": [] }, "channel_multi_sig_proofs": {} },
        "change_public_key": from,
        "funding_public_keys": [from],
        "max_tx_fee": MAX_FEE,
        "priority_fee_percent": 0,
    })
}

/// The funded transaction's ops, once checked: the inscription first, exactly as asked,
/// then only transfers paying back to `from`. Anything else isn't signed.
pub fn funded_ops(funded: &Value, op: &Value, from: &str) -> Result<Vec<Value>, String> {
    let ops = funded["funded_tx"]["ops"].as_array().cloned().unwrap_or_default();
    if ops.first() != Some(op) {
        return Err("the node funded a different inscription than the one asked for; nothing was sent".into());
    }
    for extra in &ops[1..] {
        let outputs = extra["payload"]["outputs"].as_array().cloned().unwrap_or_default();
        if extra["opcode"].as_u64() != Some(OP_TRANSFER) || outputs.iter().any(|o| o["pk"].as_str() != Some(from)) {
            return Err("the node's funding pays somewhere other than the chosen key; nothing was sent".into());
        }
    }
    Ok(ops)
}

fn bytes32(text: &str, what: &str) -> Result<[u8; 32], String> {
    let bytes = unhex(text).ok_or_else(|| format!("the node wrote a {what} that isn't hex"))?;
    bytes.try_into().map_err(|_| format!("the node wrote a {what} that isn't 32 bytes"))
}

/// One op's canonical encoding (Mantle spec): `Opcode OpPayload`. Only the two ops an
/// inscription's transaction holds are written; any other is refused.
fn encode_op(op: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    let payload = &op["payload"];
    match op["opcode"].as_u64() {
        Some(OP_INSCRIBE) => {
            out.push(OP_INSCRIBE as u8);
            encode_inscription(payload, out)
        }
        Some(OP_TRANSFER) => {
            out.push(OP_TRANSFER as u8);
            let inputs = payload["inputs"].as_array().cloned().unwrap_or_default();
            let outputs = payload["outputs"].as_array().cloned().unwrap_or_default();
            // Inputs and outputs are each at most 255, so their counts are one byte.
            out.push(u8::try_from(inputs.len()).map_err(|_| "a transfer with over 255 inputs")?);
            for input in &inputs {
                out.extend(bytes32(input.as_str().unwrap_or_default(), "note id")?);
            }
            out.push(u8::try_from(outputs.len()).map_err(|_| "a transfer with over 255 outputs")?);
            for output in &outputs {
                let value = match &output["value"] {
                    Value::Number(n) => n.as_u64(),
                    Value::String(s) => s.parse::<u64>().ok(),
                    _ => None,
                }
                .ok_or("the node wrote a note value that isn't a whole number")?;
                out.extend(value.to_le_bytes());
                out.extend(bytes32(output["pk"].as_str().unwrap_or_default(), "public key")?);
            }
            Ok(())
        }
        other => Err(format!("the node's transaction holds an op ({other:?}) an inscription doesn't; nothing was sent")),
    }
}

/// `InscriptionOp`: channel, inscription (u32 length, little-endian), parent, signer.
fn encode_inscription(payload: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    let inscription = unhex(payload["inscription"].as_str().unwrap_or_default()).ok_or("the node wrote an inscription that isn't hex")?;
    out.extend(bytes32(payload["channel_id"].as_str().unwrap_or_default(), "channel id")?);
    out.extend(u32::try_from(inscription.len()).map_err(|_| "an inscription over 4 GiB")?.to_le_bytes());
    out.extend(&inscription);
    out.extend(bytes32(payload["parent"].as_str().unwrap_or_default(), "parent")?);
    out.extend(bytes32(payload["signer"].as_str().unwrap_or_default(), "signer")?);
    Ok(())
}

/// The ops' encoding: a one-byte count (at most 255 ops), then each op.
pub fn encode_ops(ops: &[Value]) -> Result<Vec<u8>, String> {
    let mut out = vec![u8::try_from(ops.len()).map_err(|_| "over 255 ops")?];
    for op in ops {
        encode_op(op, &mut out)?;
    }
    Ok(out)
}

/// The transaction's hash, which every proof signs: blake2b-256 of `MANTLE_TXHASH_V1`
/// and the ops' encoding.
pub fn tx_hash(ops: &[Value]) -> Result<[u8; 32], String> {
    let mut hasher = Blake2b256::new();
    hasher.update(TX_HASH_PREFIX);
    hasher.update(encode_ops(ops)?);
    Ok(hasher.finalize().into())
}

/// The id the channel gives an inscription (its tip once it lands): blake2b-256 of the
/// op's payload encoding.
pub fn msg_id(op: &Value) -> Result<String, String> {
    let mut payload = Vec::new();
    encode_inscription(&op["payload"], &mut payload)?;
    Ok(hex(&Blake2b256::digest(&payload)))
}

/// The signed transaction `submit_signed_transaction` takes: the ops and their proofs in
/// op order, the inscription's signature first, then the node's proof of its fee transfer.
pub fn signed_tx(ops: &[Value], signature: &[u8; 64], transfer_proof: &Value) -> Value {
    let mut proofs = vec![json!({ "Ed25519Sig": hex(signature) })];
    if !transfer_proof.is_null() {
        proofs.push(transfer_proof.clone());
    }
    json!({ "mantle_tx": { "ops": ops }, "ops_proofs": proofs })
}

/// The inscription key of chain account `account`, made the first time it is asked for
/// and kept (0600) in `dir`.
pub fn signing_key(dir: &Path, account: &str) -> Result<SigningKey, String> {
    let path = dir.join(KEYS_FILE);
    let mut keys: Map<String, Value> = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| format!("{} can't be read; it was left as it is", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(error) => return Err(format!("{} can't be read: {error}", path.display())),
    };
    if let Some(seed) = keys.get(account).and_then(Value::as_str) {
        let seed: [u8; 32] = unhex(seed).and_then(|s| s.try_into().ok()).ok_or_else(|| format!("{} holds a damaged key", path.display()))?;
        return Ok(SigningKey::from_bytes(&seed));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|error| format!("no randomness for a new inscription key: {error}"))?;
    keys.insert(account.to_owned(), Value::from(hex(&seed)));
    let bytes = serde_json::to_vec(&keys).map_err(|error| error.to_string())?;
    crate::cli::write_private(&path, &bytes).map_err(|error| format!("the inscription key couldn't be kept in {}: {error}", path.display()))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Inscribes `text` as a testimonial from the node's key `from_key`; see `chain_inscribe`.
pub fn run(from_key: &str, text: &str, send: bool) -> Result<Value, String> {
    let from = chain::parse_key(from_key).ok_or("choose the key to inscribe from")?;
    validate_text(text)?;
    let dir = crate::config_dir().ok_or("there is no folder to keep the inscription key in")?;
    // The node's own view of the key first: a key it can't read or doesn't hold is refused
    // before any key is made for it.
    let balance = chain::balance_of(&node::call("wallet_get_balance", json!([from]), node::QUICK)?);
    let key = signing_key(&dir, &from)?;
    let signer = key.verifying_key().to_bytes();
    let channel = channel_of(&signer);
    // Only "no such channel" means a first inscription; any other failure is not a guess.
    let (parent, first) = match node::call("get_channel_state", json!([channel]), node::QUICK) {
        Ok(state) => {
            let writers = state["accredited_keys"].as_array().cloned().unwrap_or_default();
            if !writers.iter().any(|k| k.as_str() == Some(hex(&signer).as_str())) {
                return Err(format!("channel {channel} is written by another key; nothing was sent"));
            }
            let tip = state["tip_message"].as_str().and_then(chain::parse_key).ok_or("the node didn't say where the channel ends")?;
            (tip, false)
        }
        Err(error) if error.contains("No channel found") => (ZERO.to_owned(), true),
        Err(error) => return Err(error),
    };
    let bytes = inscription(text);
    let op = inscribe_op(&channel, &bytes, &parent, &signer);
    let notes = chain::notes_of(&node::call("wallet_get_notes", json!([from, ""]), node::QUICK)?);
    // An inscription of this channel still waiting to land would be the parent of this one.
    let at = crate::now();
    if send && crate::chain_sent().values().any(|sent| {
        sent["channel"] == channel.as_str()
            && sent["msg_id"].as_str() != Some(parent.as_str())
            && at.saturating_sub(sent["at"].as_u64().unwrap_or(0)) < crate::NOTES_HELD_S
            && sent["inputs"].as_array().is_some_and(|inputs| inputs.iter().any(|i| i.as_str().is_some_and(|i| notes.contains_key(i))))
    }) {
        return Err("Your last inscription from this key is still waiting to land. Try again in a minute. Nothing was sent.".into());
    }
    let funded = node::call("wallet_fund_tx", json!([fund_request(&op, &from).to_string()]), node::PROVING)?;
    let ops = funded_ops(&funded, &op, &from)?;
    let fee = chain::transfer_fee(&json!({ "mantle_tx": { "ops": ops } }), &notes);
    let mut plan = json!({
        "from": from, "balance": balance.to_string(), "signer": hex(&signer), "channel": channel, "parent": parent, "first": first,
        "submission": SUBMISSION, "text": text, "bytes": bytes.len(), "msg_id": msg_id(&op)?,
        "fee_estimate": fee.map(|f| f.to_string()), "hash": null, "fee": null,
    });
    if !send {
        return Ok(plan);
    }
    let busy = crate::chain_notes_in_use(&from);
    let (inputs, _) = chain::transfer_parts(&json!({ "mantle_tx": { "ops": ops } }));
    if inputs.iter().any(|input| busy.contains(input)) {
        return Err(crate::IN_FLIGHT.to_owned());
    }
    let hash = tx_hash(&ops)?;
    let signature = key.sign(&hash).to_bytes();
    let signed = signed_tx(&ops, &signature, &funded["transfer_proof"]);
    let submitted = node::call("submit_signed_transaction", json!([signed.to_string()]), node::PROVING)?;
    // The node hashes what it took; the signature is only good for the hash signed here.
    let ours = hex(&hash);
    if chain::hash_of(&submitted).as_deref() != Some(ours.as_str()) {
        return Err(format!("the node took the inscription as {submitted} but it was signed as {ours}; check it with chain_tx"));
    }
    crate::chain_sent_add(&ours, json!({ "from": from, "inputs": inputs, "fee": fee.map(|f| f.to_string()), "at": at,
        "channel": channel, "msg_id": plan["msg_id"] }));
    crate::activity::add(&dir, at, json!({ "kind": "inscribe", "chain": "logos", "from": from, "channel": channel,
        "submission": SUBMISSION, "text": text, "hash": ours, "fee": fee.map(|f| f.to_string()) }));
    plan["hash"] = Value::from(ours);
    plan["fee"] = fee.map_or(Value::Null, |f| Value::from(f.to_string()));
    Ok(plan)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// logos-blockchain 0.3.4, core/src/mantle/transactions/codec.rs
    /// `test_decode_signed_mantle_tx_with_inscribe`: key `[4; 32]`, one inscription.
    #[test]
    fn encodes_hashes_and_signs_as_the_node_does_upstream_vector() {
        let key = SigningKey::from_bytes(&[4u8; 32]);
        let signer = key.verifying_key().to_bytes();
        assert_eq!(hex(&signer), "ca93ac1705187071d67b83c7ff0efe8108e8ec4530575d7726879333dbdabe7c");
        let op = inscribe_op(&"aa".repeat(32), b"hello", &"bb".repeat(32), &signer);
        let encoded = hex(&encode_ops(std::slice::from_ref(&op)).unwrap());
        let want = format!("0111{}0500000068656c6c6f{}{}", "aa".repeat(32), "bb".repeat(32), hex(&signer));
        assert_eq!(encoded, want);
        let signature = key.sign(&tx_hash(&[op]).unwrap()).to_bytes();
        assert_eq!(
            hex(&signature),
            "4ec789fc67b7f7bfba02f8cc7f3f671a107225faefbe60ca0b8e9e7e8e43e8db835075aed539fac37e0fdc03acc2aba873e43eef8a835476c4c6bdaaba866901"
        );
    }

    /// A real inscription with its fee transfer, taken by a solo node (0.3.4) on
    /// 2026-11-09: the node answered hash c5086d5f… and made 6ffcd125… its channel's tip.
    #[test]
    fn hash_and_message_id_match_a_transaction_the_node_took() {
        let ops: Vec<Value> = serde_json::from_str(r#"[{"opcode":17,"payload":{"channel_id":"ff771bf83f84c0ee1a11b5a9179de08c02e60ad5262f9c52da6bd60de8577fac","inscription":"7b227375626d697373696f6e223a226c702d303032323a6e616d65732d77616c6c6574222c2274657874223a227370696b653a20666972737420696e736372697074696f6e227d","parent":"0000000000000000000000000000000000000000000000000000000000000000","signer":"c73dabc9096c01114f1c9775b2c2c3101308cdb631397e67eb969efa29b391d2"}},{"opcode":0,"payload":{"inputs":["f08aaa6388c8aa77e71fbc3a2a2b3edea5684e3b80bd7e778b8755bc893c0b0f"],"outputs":[{"value":98914,"pk":"e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07"}]}}]"#).unwrap();
        assert_eq!(hex(&tx_hash(&ops).unwrap()), "c5086d5f12dbbf7075af31689e80a9ec33f22c502ba878486f5a3a0b17cce606");
        assert_eq!(msg_id(&ops[0]).unwrap(), "6ffcd125bd8e9b7abc6d60af55c3050c38340d4fd7192f76713995e337af2fee");
        // The channel is the signer's, and the inscription the testimonial's JSON.
        let signer = bytes32("c73dabc9096c01114f1c9775b2c2c3101308cdb631397e67eb969efa29b391d2", "signer").unwrap();
        assert_eq!(channel_of(&signer), "ff771bf83f84c0ee1a11b5a9179de08c02e60ad5262f9c52da6bd60de8577fac");
        assert_eq!(hex(&inscription("spike: first inscription")), ops[0]["payload"]["inscription"].as_str().unwrap());
    }

    #[test]
    fn refuses_to_sign_what_it_did_not_ask_for() {
        let from = "e3".repeat(32);
        let op = inscribe_op(&"aa".repeat(32), b"x", ZERO, &[1; 32]);
        let fee = json!({ "opcode": 0, "payload": { "inputs": ["01".repeat(32)], "outputs": [{ "value": 5, "pk": from }] } });
        assert!(funded_ops(&json!({ "funded_tx": { "ops": [op, fee] } }), &op, &from).is_ok());
        let other = inscribe_op(&"aa".repeat(32), b"y", ZERO, &[1; 32]);
        assert!(funded_ops(&json!({ "funded_tx": { "ops": [other, fee] } }), &op, &from).is_err(), "a changed inscription");
        let away = json!({ "opcode": 0, "payload": { "inputs": [], "outputs": [{ "value": 5, "pk": "ab".repeat(32) }] } });
        assert!(funded_ops(&json!({ "funded_tx": { "ops": [op, away] } }), &op, &from).is_err(), "change to another key");
        let deposit = json!({ "opcode": 18, "payload": {} });
        assert!(funded_ops(&json!({ "funded_tx": { "ops": [op, deposit] } }), &op, &from).is_err(), "an op it didn't ask for");
        assert!(encode_ops(&[deposit]).is_err());
    }

    const FROM: &str = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07";
    const NOTE: &str = "f08aaa6388c8aa77e71fbc3a2a2b3edea5684e3b80bd7e778b8755bc893c0b0f";

    fn answer(method: &str, value: Value) {
        let value = if value.is_string() { value } else { Value::from(value.to_string()) };
        node::ANSWERS.with(|a| a.borrow_mut().insert(method.to_owned(), json!({ "success": true, "value": value, "error": null })));
    }

    fn refuse(method: &str, error: &str) {
        node::ANSWERS.with(|a| a.borrow_mut().insert(method.to_owned(), json!({ "success": false, "value": null, "error": error })));
    }

    fn asked() -> Vec<String> {
        node::ASKED.with(|asked| asked.borrow().iter().map(|(m, _)| m.clone()).collect())
    }

    /// A node with FROM holding one note of 100,000 and no channel yet, whose funding
    /// pays 1,086 from that note: what the solo node did. Gives the op it funds.
    fn solo_node(text: &str) -> (Value, Vec<Value>) {
        node::ANSWERS.with(|a| a.borrow_mut().clear());
        node::ASKED.with(|a| a.borrow_mut().clear());
        let _ = std::fs::remove_dir_all(crate::config_dir().unwrap());
        let key = signing_key(&crate::config_dir().unwrap(), FROM).unwrap();
        let signer = key.verifying_key().to_bytes();
        let op = inscribe_op(&channel_of(&signer), &inscription(text), ZERO, &signer);
        let fee = json!({ "opcode": 0, "payload": { "inputs": [NOTE], "outputs": [{ "value": 98914, "pk": FROM }] } });
        let ops = vec![op.clone(), fee];
        answer("wallet_get_balance", json!("100000"));
        refuse("get_channel_state", "No channel found for id ChannelId([255, 119])");
        answer("wallet_get_notes", json!({ "notes": [{ "id": NOTE, "value": "100000" }] }));
        answer("wallet_fund_tx", json!({ "tip": "2b".repeat(32), "funded_tx": { "ops": ops }, "transfer_proof": { "ZkSig": { "pi_a": "6f" } } }));
        (op, ops)
    }

    #[test]
    fn plans_then_inscribes_and_follows_it_like_a_send() {
        let (op, ops) = solo_node("good wallet");
        let plan = run(FROM, "good wallet", false).unwrap();
        assert_eq!(plan["first"], true, "{plan}");
        assert_eq!(plan["parent"], ZERO);
        assert_eq!(plan["fee_estimate"], "1086", "100,000 in, 98,914 back");
        assert_eq!(plan["hash"], Value::Null, "a plan sends nothing");
        assert_eq!(plan["msg_id"], msg_id(&op).unwrap());
        assert!(!asked().contains(&"submit_signed_transaction".to_owned()));
        let hash = hex(&tx_hash(&ops).unwrap());
        answer("submit_signed_transaction", json!(hash));
        let sent = run(FROM, "good wallet", true).unwrap();
        assert_eq!(sent["hash"], hash.as_str(), "{sent}");
        assert_eq!(sent["fee"], "1086");
        // What was submitted: the funded ops, the inscription's signature over the hash, the node's proof.
        let submitted = node::ASKED.with(|a| a.borrow().iter().rev().find(|(m, _)| m == "submit_signed_transaction").map(|(_, args)| args.clone())).unwrap();
        let signed: Value = serde_json::from_str(submitted[0].as_str().unwrap()).unwrap();
        assert_eq!(signed["mantle_tx"]["ops"], json!(ops));
        assert_eq!(signed["ops_proofs"][1], json!({ "ZkSig": { "pi_a": "6f" } }));
        let key = signing_key(&crate::config_dir().unwrap(), FROM).unwrap();
        let sig: [u8; 64] = unhex(signed["ops_proofs"][0]["Ed25519Sig"].as_str().unwrap()).unwrap().try_into().unwrap();
        key.verifying_key().verify_strict(&tx_hash(&ops).unwrap(), &ed25519_dalek::Signature::from_bytes(&sig)).unwrap();
        // Followed as a send is: held notes, and chain_tx knows it after a reload (on disk).
        let kept = crate::chain_sent();
        assert_eq!(kept[&hash]["inputs"], json!([NOTE]), "{kept:?}");
        assert_eq!(kept[&hash]["msg_id"], msg_id(&op).unwrap());
        // A second one before the first lands would chain on the wrong parent.
        let again = run(FROM, "good wallet", true).unwrap_err();
        assert!(again.contains("still waiting to land"), "{again}");
    }

    #[test]
    fn a_later_inscription_chains_on_the_channels_tip() {
        let (_, _) = solo_node("x");
        let key = signing_key(&crate::config_dir().unwrap(), FROM).unwrap();
        let signer = hex(&key.verifying_key().to_bytes());
        answer("get_channel_state", json!({ "accredited_keys": [signer], "tip_message": "6f".repeat(32) }));
        // The funded op must be the one asked for, now with the tip as parent.
        let op = inscribe_op(&channel_of(&key.verifying_key().to_bytes()), &inscription("x"), &"6f".repeat(32), &key.verifying_key().to_bytes());
        let fee = json!({ "opcode": 0, "payload": { "inputs": [NOTE], "outputs": [{ "value": 98914, "pk": FROM }] } });
        answer("wallet_fund_tx", json!({ "funded_tx": { "ops": [op, fee] }, "transfer_proof": null }));
        let plan = run(FROM, "x", false).unwrap();
        assert_eq!((plan["first"].clone(), plan["parent"].clone()), (json!(false), json!("6f".repeat(32))), "{plan}");
        // A channel some other key writes is refused.
        answer("get_channel_state", json!({ "accredited_keys": ["ab".repeat(32)], "tip_message": "6f".repeat(32) }));
        assert!(run(FROM, "x", false).unwrap_err().contains("written by another key"));
    }

    #[test]
    fn refusals_say_why_and_send_nothing() {
        let (_, ops) = solo_node("x");
        // The node took it under another hash: not reported as sent.
        answer("submit_signed_transaction", json!("cd".repeat(32)));
        assert!(run(FROM, "x", true).unwrap_err().contains("signed as"));
        assert!(!crate::chain_sent().contains_key(&hex(&tx_hash(&ops).unwrap())));
        // Only "no such channel" is a first inscription: another failure isn't guessed at.
        solo_node("x");
        refuse("get_channel_state", "node is not running");
        let down = run(FROM, "x", false).unwrap_err();
        assert_eq!(down, "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app.");
        assert!(!asked().contains(&"wallet_fund_tx".to_owned()));
        // A key the node can't read: no inscription key is made for it.
        solo_node("x");
        refuse("wallet_get_balance", "Invalid wallet address: FrFromBytesError");
        let bad = "ab".repeat(32);
        assert!(run(&bad, "x", false).unwrap_err().contains("isn't a Logos Blockchain public key"));
        let kept: Map<String, Value> = serde_json::from_slice(&std::fs::read(crate::config_dir().unwrap().join(KEYS_FILE)).unwrap()).unwrap();
        assert!(!kept.contains_key(&bad));
        assert!(run("nothex", "x", false).unwrap_err().contains("choose the key"));
        assert!(run(FROM, " ", false).unwrap_err().contains("needs some text"));
        // The funding's own refusal reaches the person in plain words.
        solo_node("x");
        refuse("wallet_fund_tx", "Failed to fund tx: Wallet does not have enough funds, available=10");
        assert_eq!(run(FROM, "x", false).unwrap_err(), "That key doesn't hold enough for this and its fee.");
    }

    #[test]
    fn only_the_names_app_may_inscribe() {
        use crate::NamesModule as _;
        solo_node("x");
        crate::TEST_CALLER.with(|c| *c.borrow_mut() = logos_rust_sdk::LogosCaller::Module { name: "evil_ui".into(), instance: None });
        let refused: Value = serde_json::from_str(&crate::Names::default().chain_inscribe(FROM.into(), "x".into(), true)).unwrap();
        crate::TEST_CALLER.with(|c| *c.borrow_mut() = logos_rust_sdk::LogosCaller::HostAnchor);
        assert_eq!(refused["success"], false);
        assert!(refused["error"].as_str().unwrap().contains("only the Names app"), "{refused}");
        assert!(asked().is_empty(), "the node wasn't asked anything");
    }

    #[test]
    fn text_is_checked_as_the_testimonial_program_checks_it() {
        assert!(validate_text("good wallet").is_ok());
        assert!(validate_text(&"x".repeat(MAX_TEXT)).is_ok());
        for bad in ["", "  \n", "bell\u{7}", &"x".repeat(MAX_TEXT + 1)] {
            assert!(validate_text(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_accounts_key_is_made_once_kept_private_and_reused() {
        let dir = std::env::temp_dir().join(format!("names-inscribe-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = signing_key(&dir, &"aa".repeat(32)).unwrap();
        let again = signing_key(&dir, &"aa".repeat(32)).unwrap();
        let other = signing_key(&dir, &"bb".repeat(32)).unwrap();
        assert_eq!(first.to_bytes(), again.to_bytes());
        assert_ne!(first.to_bytes(), other.to_bytes());
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(KEYS_FILE)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
