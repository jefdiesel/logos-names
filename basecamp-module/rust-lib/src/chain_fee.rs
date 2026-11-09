//! What a chain transaction will cost, before it is sent (LP-0022: "display estimated gas
//! usage"), asked of the node itself.
//!
//! The node's `wallet_fund_tx` funds a transaction builder and returns it with its fee:
//! the same funding a send, split, deposit or claim goes through, against the gas prices
//! at the node's tip. Asked with the key's REAL notes, it RESERVES them (blockchain_module
//! 0.3.4 on a solo node: a second plan answered "available=0" and the send after it was
//! refused; the reservation is lifted only once the notes are spent or 10 blocks later
//! become final). So it is never asked with a real note. Instead the builder carries
//! stand-ins: notes of the same values and owner as the ones the node would spend (largest
//! first, as its wallet picks them), under ids no wallet holds, and no funding key, so the
//! node can't pull in a real note. The transaction is then the same shape (op, inputs,
//! outputs, sizes), so its fee is the same, at the prices of the moment: measured on a solo
//! node, 834 for one input and 866 for two, as the sends after them paid. Only the stand-in
//! ids are reserved, and they are the same ids every time.

use serde_json::{json, Value};

/// No cap on the stand-in transaction: the estimate is to be read, not refused.
const NO_CAP: u64 = 1_000_000_000_000_000;

/// A transfer op carries at most this many inputs (the node's bound).
const MAX_INPUTS: usize = 32;

/// The ids under which a stand-in deposit names its note and a stand-in claim its voucher
/// nullifier, after the transfer stand-ins' 1..=32. The node reads these 32 bytes as a
/// little-endian field element, so the last byte must stay small: `..0063` (99) was refused
/// ("did not match any variant of untagged enum OpDe"), `..0007` was read.
const DEPOSIT_NOTE: u64 = 33;
const CLAIM_NULLIFIER: u64 = 34;

/// What is estimated: the ops the transaction has besides its fee-paying transfer.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// `wallet_transfer_funds`: `amount` to `to`, change back (a split is a send to oneself).
    Transfer { to: String, amount: u64 },
    /// `channel_deposit_with_notes`: one whole note into `channel`, `metadata` bytes long.
    Deposit { channel: String, metadata: usize },
    /// `leader_claim`: one LeaderClaim op, its fee paid from the funding key.
    Claim,
}

/// The stand-in note `i`: an op id no transaction has, so no wallet holds it.
fn stand_in_op(i: usize) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(format!("names-fee-estimate-{i}").as_bytes()).to_vec()
}

fn note_id(n: u64) -> String {
    format!("{n:064x}")
}

/// The `wallet_fund_tx` request for `shape` paid by stand-ins of `values` owned by `from`.
pub fn request(shape: &Shape, from: &str, values: &[u128]) -> Value {
    let inputs: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, value)| json!({ "op_id": stand_in_op(i), "output_index": 0, "note": { "value": u64::try_from(*value).unwrap_or(u64::MAX), "pk": from } }))
        .collect();
    let ids: Vec<String> = (1..=values.len() as u64).map(note_id).collect();
    let (ops, outputs) = match shape {
        Shape::Transfer { to, amount } => (json!([]), json!([{ "value": amount, "pk": to }])),
        Shape::Deposit { channel, metadata } => (
            json!([{ "opcode": 0x12, "payload": { "channel_id": channel, "inputs": [note_id(DEPOSIT_NOTE)], "metadata": vec![0u8; *metadata] } }]),
            json!([]),
        ),
        Shape::Claim => (
            json!([{ "opcode": 0x30, "payload": { "pk": from, "rewards_root": "00".repeat(32), "voucher_nullifier": note_id(CLAIM_NULLIFIER) } }]),
            json!([]),
        ),
    };
    json!({
        "tip": null,
        "tx_builder": {
            "mantle_tx": { "ops": ops },
            "ledger_inputs": inputs,
            "pending_transfer": { "inputs": ids, "outputs": outputs },
            "channel_multi_sig_proofs": {},
        },
        "change_public_key": from,
        "funding_public_keys": [],
        "max_tx_fee": NO_CAP,
        "priority_fee_percent": 0,
    })
}

/// The fee of a funded stand-in transaction: what its stand-ins hold less what its
/// transfer pays out (the amount and the change).
pub fn fee_of(funded: &Value, held: u128) -> Option<u128> {
    let ops = funded["funded_tx"]["ops"].as_array().or(funded["funded_tx"]["mantle_tx"]["ops"].as_array())?;
    let paid: u128 = ops
        .iter()
        .filter(|op| op["opcode"] == 0)
        .flat_map(|op| op["payload"]["outputs"].as_array().into_iter().flatten())
        .map(|output| super::chain::amount(&output["value"]))
        .sum();
    held.checked_sub(paid)
}

/// What the node said a transaction would cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estimate {
    /// Its fee, at the node's gas prices now.
    Fee(u128),
    /// The key's notes can't pay it and its fee (the node said so for every set of them).
    Short,
    /// The node didn't say (not running, or refused the request).
    Unknown,
}

impl Estimate {
    pub fn fee(self) -> Option<u128> {
        match self {
            Self::Fee(fee) => Some(fee),
            _ => None,
        }
    }
}

/// The node's fee for `shape` paid from `from`, whose spendable notes hold `notes`: the
/// fewest largest notes that cover it, as the node's wallet picks them. `ask` sends one
/// `wallet_fund_tx` request (its JSON text) and gives the node's reply.
pub fn estimate(shape: &Shape, from: &str, notes: &[u128], mut ask: impl FnMut(String) -> Result<Value, String>) -> Estimate {
    let mut values = notes.to_vec();
    values.sort_unstable_by(|a, b| b.cmp(a));
    let floor = match shape {
        Shape::Transfer { amount, .. } => u128::from(*amount),
        _ => 0,
    };
    let short = super::chain::problem("wallet_fund_tx", "not enough");
    for count in 1..=values.len().min(MAX_INPUTS) {
        let held: u128 = values[..count].iter().sum();
        if held <= floor {
            continue;
        }
        match ask(request(shape, from, &values[..count]).to_string()) {
            Ok(funded) => return fee_of(&funded, held).map_or(Estimate::Unknown, Estimate::Fee),
            // These notes can't pay it with its fee: one more, as the wallet would.
            Err(error) if error == short => continue,
            Err(_) => return Estimate::Unknown,
        }
    }
    Estimate::Short
}

/// The key and note a claim's stand-in is made of: the largest note any key holds, so it
/// pays the claim's fee in one input as the leader funding key's note does. The first key
/// with notes was used before: a key whose largest note can't pay a claim's fee (the
/// fixture's B holds 100) leaves the plan with no estimate, as a solo node's did while
/// 1,161 vouchers waited.
pub fn claim_stand_in(held: &[(String, Vec<u128>)]) -> Option<(String, u128)> {
    held.iter()
        .filter_map(|(key, notes)| notes.iter().max().map(|largest| (key.clone(), *largest)))
        .max_by_key(|(_, largest)| *largest)
}

/// A claim's estimate beside the reward it cashes: a warning when it costs more.
pub fn claim_warning(fee: Option<u128>, reward: u128) -> Option<String> {
    let fee = fee?;
    (fee > reward).then(|| {
        format!("A claim cashes one reward of {reward} and pays a fee of about {fee}: it costs more than it brings. The node's claim has no fee cap you can set.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07";
    const B: &str = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225";

    /// The node's reply to a stand-in plan on a solo 0.3.4 node (exp2 si1): 100,000 in,
    /// 3,000 + 96,166 out, so 834, what the send after it paid.
    fn funded(outputs: &[u64]) -> Value {
        let outs: Vec<Value> = outputs.iter().map(|v| json!({ "pk": A, "value": v })).collect();
        json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": [note_id(1)], "outputs": outs } }] }, "tip": "b3", "transfer_proof": {} })
    }

    #[test]
    fn the_fee_is_what_the_stand_ins_hold_less_what_is_paid_out() {
        assert_eq!(fee_of(&funded(&[3000, 96166]), 100_000), Some(834));
        assert_eq!(fee_of(&json!({ "funded_tx": { "mantle_tx": { "ops": [] } } }), 5), Some(5));
        assert_eq!(fee_of(&json!({ "error": "x" }), 5), None);
        assert_eq!(fee_of(&funded(&[10]), 5), None, "paid out more than held is no fee");
    }

    /// No real note and no funding key: the node can only spend the stand-ins.
    #[test]
    fn a_request_names_no_real_note_and_no_funding_key() {
        let shown = request(&Shape::Transfer { to: B.into(), amount: 3000 }, A, &[100_000, 1234]);
        assert_eq!(shown["funding_public_keys"], json!([]));
        assert_eq!(shown["change_public_key"], A);
        assert_eq!(shown["priority_fee_percent"], 0, "the node's own sends fund with 0");
        assert_eq!(shown["tx_builder"]["pending_transfer"]["outputs"], json!([{ "value": 3000, "pk": B }]));
        assert_eq!(shown["tx_builder"]["pending_transfer"]["inputs"], json!([note_id(1), note_id(2)]));
        let inputs = shown["tx_builder"]["ledger_inputs"].as_array().unwrap();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0]["note"], json!({ "value": 100_000, "pk": A }));
        assert_eq!(inputs[0]["op_id"].as_array().unwrap().len(), 32, "the node reads an op id as 32 bytes");
        // The same ids every time, so the node's reservations don't grow with each plan.
        assert_eq!(request(&Shape::Claim, A, &[5]), request(&Shape::Claim, A, &[5]));
        let deposit = request(&Shape::Deposit { channel: "01".repeat(32), metadata: 32 }, A, &[5]);
        assert_eq!(deposit["tx_builder"]["mantle_tx"]["ops"][0]["opcode"], 0x12);
        // As the node stored a real deposit (spike 5): metadata a list of bytes.
        assert_eq!(deposit["tx_builder"]["mantle_tx"]["ops"][0]["payload"]["metadata"], json!(vec![0; 32]));
        assert_eq!(request(&Shape::Claim, A, &[5])["tx_builder"]["mantle_tx"]["ops"][0]["opcode"], 0x30);
    }

    /// Largest notes first, one more each time the node says they can't pay it.
    #[test]
    fn the_estimate_takes_the_fewest_largest_notes_as_the_wallet_does() {
        let short = crate::chain::problem("wallet_fund_tx", "Failed to fund tx: Wallet does not have enough funds, available=0");
        let mut asked = Vec::new();
        let fee = estimate(&Shape::Transfer { to: B.into(), amount: 94_198 }, A, &[1234, 94_098], |request| {
            let request: Value = serde_json::from_str(&request).unwrap();
            let count = request["tx_builder"]["ledger_inputs"].as_array().unwrap().len();
            asked.push(count);
            // As the solo node answered (exp2 si-2in): two inputs, 94,198 + 268 out.
            if count < 2 { Err(short.clone()) } else { Ok(funded(&[94_198, 268])) }
        });
        assert_eq!(fee, Estimate::Fee(866));
        assert_eq!(asked, [2], "one note of 94,098 can't pay 94,198, so it isn't asked");
        let mut asked = 0;
        let fee = estimate(&Shape::Transfer { to: B.into(), amount: 3000 }, A, &[3500, 100], |_| {
            asked += 1;
            Err(short.clone())
        });
        assert_eq!((fee, asked), (Estimate::Short, 2), "nothing covers it with its fee");
        // The proof run's case: 1,268 in two notes can't send 1,000 and a fee of 866.
        assert_eq!(estimate(&Shape::Transfer { to: B.into(), amount: 1000 }, A, &[1000, 268], |_| Err(short.clone())), Estimate::Short);
        assert_eq!(estimate(&Shape::Transfer { to: B.into(), amount: 1000 }, A, &[900], |_| unreachable!()), Estimate::Short, "less than the amount");
        let down = estimate(&Shape::Claim, A, &[50_000], |_| Err("Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app.".into()));
        assert_eq!(down, Estimate::Unknown, "a node that didn't say is not a short key");
        assert_eq!(estimate(&Shape::Claim, A, &[], |_| unreachable!()), Estimate::Short, "no notes, nothing asked");
    }

    /// Every stand-in id read as a little-endian field element stays below the BN254
    /// modulus (its top byte, the last one written, at most 48).
    #[test]
    fn stand_in_ids_are_field_elements_the_node_can_read() {
        let mut ids: Vec<String> = (1..=MAX_INPUTS as u64).map(note_id).collect();
        ids.extend([note_id(DEPOSIT_NOTE), note_id(CLAIM_NULLIFIER)]);
        for id in &ids {
            assert!(u8::from_str_radix(&id[62..], 16).unwrap() <= 48 && id[..62].bytes().all(|b| b == b'0'), "{id}");
        }
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "the deposit's and the claim's stand-ins are not a transfer's");
    }

    /// The solo node's keys at the claim run: B with 100, the leader funding key with
    /// 5,000, A with 94,166, in any order. The stand-in is A's note.
    #[test]
    fn a_claim_stand_in_is_the_largest_note_any_key_holds() {
        let held = vec![(B.to_owned(), vec![100]), ("63f1".repeat(16), vec![5000]), (A.to_owned(), vec![94_166]), ("0d".repeat(32), vec![])];
        assert_eq!(claim_stand_in(&held), Some((A.to_owned(), 94_166)));
        assert_eq!(claim_stand_in(&[(B.to_owned(), vec![])]), None);
    }

    #[test]
    fn a_claim_warns_when_its_fee_is_more_than_its_reward() {
        assert!(claim_warning(Some(1599), 38).unwrap().contains("reward of 38 and pays a fee of about 1599"));
        assert_eq!(claim_warning(Some(10), 38), None);
        assert_eq!(claim_warning(None, 38), None);
    }
}
