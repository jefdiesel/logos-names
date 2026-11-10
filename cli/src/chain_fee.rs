//! What a Logos Blockchain send will cost, asked of the node before it is sent (LP-0022).
//!
//! The same method as the Names module's `chain_fee.rs` (the module is its own crate, so
//! this is a copy): the node's `POST /wallet/fund` funds a transaction builder and returns
//! it with its fee. Asked with the key's real notes it RESERVES them (a solo 0.3.4 node
//! refused the send after such a plan), so the builder carries stand-ins: notes of the
//! same values and owner as the ones the node would spend, largest first, under ids no
//! wallet holds, with no funding key. Same shape, same fee: 834 for one input and 866 for
//! two on a solo node, as the sends after them paid.

use serde_json::{json, Value};

const NO_CAP: u64 = 1_000_000_000_000_000;
const MAX_INPUTS: usize = 32;

/// The `/wallet/fund` body for sending `amount` to `to` from stand-ins of `values` owned by `from`.
pub fn transfer_request(from: &str, to: &str, amount: u64, values: &[u128]) -> Value {
    let inputs: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let mut op_id = vec![0u8; 32];
            op_id[0] = 0x4e;
            op_id[1] = i as u8;
            json!({ "op_id": op_id, "output_index": 0, "note": { "value": u64::try_from(*value).unwrap_or(u64::MAX), "pk": from } })
        })
        .collect();
    let ids: Vec<String> = (1..=values.len() as u64).map(|n| format!("{n:064x}")).collect();
    json!({
        "tip": null,
        "tx_builder": {
            "mantle_tx": { "ops": [] },
            "ledger_inputs": inputs,
            "pending_transfer": { "inputs": ids, "outputs": [{ "value": amount, "pk": to }] },
            "channel_multi_sig_proofs": {},
        },
        "change_public_key": from,
        "funding_public_keys": [],
        "max_tx_fee": NO_CAP,
        "priority_fee_percent": 0,
    })
}

/// The fee of a funded stand-in transaction: what the stand-ins hold less what it pays out.
pub fn fee_of(funded: &Value, held: u128) -> Option<u128> {
    let ops = funded["funded_tx"]["ops"].as_array().or(funded["funded_tx"]["mantle_tx"]["ops"].as_array())?;
    let paid: u128 = ops
        .iter()
        .filter(|op| op["opcode"] == 0)
        .flat_map(|op| op["payload"]["outputs"].as_array().into_iter().flatten())
        .map(|output| match &output["value"] {
            Value::Number(n) => n.as_u64().map(u128::from).unwrap_or(0),
            Value::String(s) => s.parse().unwrap_or(0),
            _ => 0,
        })
        .sum();
    held.checked_sub(paid)
}

/// What the node said a send would cost (as the module's `chain_fee::Estimate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estimate {
    /// Its fee, at the node's gas prices now.
    Fee(u128),
    /// The key's notes can't pay the amount and its fee: they hold no more than the
    /// amount, or the node said "not enough" for every set of the largest of them.
    Short,
    /// The node didn't say (not reachable, or refused the request for another reason).
    Unknown,
}

impl Estimate {
    pub fn fee(self) -> Option<u128> {
        match self {
            Self::Fee(fee) => Some(fee),
            _ => None,
        }
    }

    /// The plan's fee line (`names chain send` without --yes), for the key `from` (shortened).
    pub fn line(self, from: &str) -> String {
        match self {
            Self::Fee(fee) => format!("  fee: about {} (the node's own figure at today's gas prices; paid from {from}'s notes)", crate::chain::grouped(fee)),
            Self::Short => format!("  fee: {from}'s notes can't pay this amount and its fee: send a little less"),
            Self::Unknown => "  fee: the node couldn't say in advance; it is shown once sent".to_owned(),
        }
    }
}

/// The node's fee for sending `amount` to `to` from notes holding `notes`: the fewest
/// largest that cover it. `ask` posts one body to `/wallet/fund`.
pub fn estimate(from: &str, to: &str, amount: u64, notes: &[u128], mut ask: impl FnMut(&Value) -> anyhow::Result<Value>) -> Estimate {
    let mut values = notes.to_vec();
    values.sort_unstable_by(|a, b| b.cmp(a));
    for count in 1..=values.len().min(MAX_INPUTS) {
        let held: u128 = values[..count].iter().sum();
        if held <= u128::from(amount) {
            continue;
        }
        match ask(&transfer_request(from, to, amount, &values[..count])) {
            Ok(funded) => return fee_of(&funded, held).map_or(Estimate::Unknown, Estimate::Fee),
            Err(error) if error.to_string().contains("not have enough funds") => continue,
            Err(_) => return Estimate::Unknown,
        }
    }
    Estimate::Short
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07";
    const B: &str = "39e16b432574571a6bcd8ee36e370589641bd9f35367f6f97a972453c46c3225";

    #[test]
    fn a_fee_request_names_no_real_note_and_no_funding_key() {
        let body = transfer_request(A, B, 3000, &[100_000]);
        assert_eq!(body["funding_public_keys"], json!([]));
        assert_eq!(body["tx_builder"]["ledger_inputs"][0]["note"], json!({ "value": 100_000, "pk": A }));
        assert_eq!(body["tx_builder"]["ledger_inputs"][0]["op_id"].as_array().unwrap().len(), 32);
        assert_eq!(body["tx_builder"]["pending_transfer"]["outputs"], json!([{ "value": 3000, "pk": B }]));
        assert_eq!(body, transfer_request(A, B, 3000, &[100_000]), "the same ids every time");
    }

    /// The solo node's replies (exp2): one input 834, two inputs 866.
    #[test]
    fn the_estimate_is_the_nodes_fee_for_the_same_shape() {
        let funded = |outs: &[u64]| json!({ "funded_tx": { "ops": [{ "opcode": 0, "payload": { "inputs": ["01"], "outputs": outs.iter().map(|v| json!({ "value": v })).collect::<Vec<_>>() } }] } });
        assert_eq!(estimate(A, B, 3000, &[100_000], |_| Ok(funded(&[3000, 96166]))), Estimate::Fee(834));
        let mut asked = Vec::new();
        let fee = estimate(A, B, 94_198, &[1234, 94_098], |body| {
            asked.push(body["tx_builder"]["ledger_inputs"].as_array().unwrap().len());
            Ok(funded(&[94_198, 268]))
        });
        assert_eq!((fee, asked), (Estimate::Fee(866), vec![2]));
    }

    /// adv.py H7: B holds one note of 100, a plan to send 0. The node says "not enough"
    /// for it, and the plan says the key can't pay, apart from a node that didn't say.
    #[test]
    fn a_key_that_cant_pay_is_told_apart_from_a_node_that_didnt_say() {
        let short = || anyhow::anyhow!("the node refused /wallet/fund (HTTP 500): Failed to fund tx: Wallet does not have enough funds, available=0");
        assert_eq!(estimate(B, A, 0, &[100], |_| Err(short())), Estimate::Short);
        assert_eq!(estimate(A, B, 3000, &[3500], |_| Err(short())), Estimate::Short);
        assert_eq!(estimate(A, B, 3000, &[2000], |_| unreachable!()), Estimate::Short, "less than the amount, nothing asked");
        assert_eq!(estimate(A, B, 3000, &[100_000], |_| Err(anyhow::anyhow!("cannot reach your node"))), Estimate::Unknown);
        assert_eq!(Estimate::Short.line("39e16b43…3225"), "  fee: 39e16b43…3225's notes can't pay this amount and its fee: send a little less");
        assert_eq!(Estimate::Unknown.line("x"), "  fee: the node couldn't say in advance; it is shown once sent");
        assert_eq!(Estimate::Fee(834).line("x"), "  fee: about 834 (the node's own figure at today's gas prices; paid from x's notes)");
    }
}
