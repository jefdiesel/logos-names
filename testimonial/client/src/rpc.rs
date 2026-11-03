//! The two sequencer reads listing needs, as plain JSON-RPC over HTTP: `getAccountView` (one
//! actor's state) and `getAccount` (an account's nonce). No wallet and no indexer.

use std::{cell::Cell, collections::BTreeMap, time::Duration};

use anyhow::{Result, anyhow, bail};
use lee_core::account::AccountId;
use serde_json::{Value, json};

pub struct Rpc {
    url: String,
    agent: ureq::Agent,
    calls: Cell<usize>,
}

/// An account as the sequencer returns it: its nonce and each program's state on it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AccountStates {
    pub nonce: u128,
    pub actor_states: BTreeMap<AccountId, Vec<u8>>,
}

impl Rpc {
    #[must_use]
    pub fn new(url: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .max_redirects(0)
            .build();
        Self { url: url.to_owned(), agent: config.into(), calls: Cell::new(0) }
    }

    /// How many calls this client has made.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.get()
    }

    fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.calls.set(self.calls.get() + 1);
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let text = self
            .agent
            .post(&self.url)
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|e| anyhow!("{}: {e}", self.url))?
            .body_mut()
            .read_to_string()
            .map_err(|e| anyhow!("{}: {e}", self.url))?;
        let reply: Value = serde_json::from_str(&text).map_err(|_| anyhow!("{} answered with something other than JSON", self.url))?;
        if let Some(error) = reply.get("error") {
            bail!("{method}: {error}");
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    /// `program`'s state on `account`; empty if none.
    pub fn actor_state(&self, account: &AccountId, program: &AccountId) -> Result<Vec<u8>> {
        let params = json!([{"account_id": account.to_string(), "program_account_id": program.to_string()}]);
        let found = parse_account(&self.call("getAccountView", params)?)?;
        Ok(found.and_then(|mut a| a.actor_states.remove(program)).unwrap_or_default())
    }

    /// The account, or `None` if it doesn't exist.
    pub fn account(&self, account: &AccountId) -> Result<Option<AccountStates>> {
        parse_account(&self.call("getAccount", json!([account.to_string()]))?)
    }
}

/// Parses `{"nonce": n, "data": {"actor_states": {"<base58 program>": [bytes…]}}}`.
pub fn parse_account(v: &Value) -> Result<Option<AccountStates>> {
    if v.is_null() {
        return Ok(None);
    }
    let bad = |what: &str| anyhow!("unexpected account shape: {what}");
    let nonce = match v.get("nonce") {
        Some(Value::Number(n)) => n.as_u64().map(u128::from).ok_or_else(|| bad("nonce"))?,
        Some(Value::String(s)) => s.parse().map_err(|_| bad("nonce"))?,
        _ => 0,
    };
    let mut actor_states = BTreeMap::new();
    if let Some(states) = v.get("data").and_then(|d| d.get("actor_states")).and_then(Value::as_object) {
        for (program, bytes) in states {
            let program: AccountId = program.parse().map_err(|_| bad("program id"))?;
            let bytes = bytes
                .as_array()
                .ok_or_else(|| bad("state bytes"))?
                .iter()
                .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()).ok_or_else(|| bad("state byte")))
                .collect::<Result<Vec<u8>>>()?;
            actor_states.insert(program, bytes);
        }
    }
    Ok(Some(AccountStates { nonce, actor_states }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_rc1_account_shape() {
        // Captured from an rc1 sequencer's getAccount (lez-program-registry sdk/rust/src/rpc.rs).
        let v: Value = serde_json::from_str(
            r#"{"nonce":22,"data":{"actor_states":{"11111111111111111111111111111111":[16,162,160,231,23,9,0,0,0,0,0,0,0,0,0,0]}}}"#,
        )
        .unwrap();
        let a = parse_account(&v).unwrap().unwrap();
        assert_eq!(a.nonce, 22);
        assert_eq!(a.actor_states[&AccountId::new([0; 32])].len(), 16);
        assert_eq!(parse_account(&Value::Null).unwrap(), None);
    }
}
