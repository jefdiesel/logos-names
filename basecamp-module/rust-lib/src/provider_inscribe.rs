//! The Wallet Provider SDK's inscription request (LP-0022): an app asks for a testimonial
//! to be inscribed on the Logos Blockchain; the person picks which of the node's chain
//! accounts inscribes it (and pays its fee) and approves it in the Names app, which then
//! runs `chain_inscribe` with that account.
//!
//! An inscription request names no account and reads nothing of the person's, so unlike a
//! LEZ transfer or call it doesn't need the app to have been given LEZ accounts first:
//! the person's approval, with the account chosen there, is the consent. The store's
//! limits (requests waiting per app and in all, requests per app per hour) still apply.

use crate::{chain, chain_inscribe, provider, registry, Names};
use serde_json::{json, Value};

/// What the request asks the person, in the sheet's words.
pub const SAYS: &str = "inscribes on the Logos blockchain";

/// A new inscription request from `app` for `text`, checked as the inscription will be.
pub fn request(app: &str, text: &str, now: u64) -> Result<Value, String> {
    chain_inscribe::validate_text(text)?;
    Ok(json!({
        "id": registry::hex(&registry::fresh_bytes::<16>()?),
        "app": app,
        "kind": provider::INSCRIBE,
        "state": "pending",
        "created": now,
        "expires": now + provider::PENDING_FOR_MS,
        "chain": "logos",
        "says": SAYS,
        "submission": chain_inscribe::SUBMISSION,
        "text": text,
    }))
}

impl Names {
    /// See `NamesModule::propose_inscription`.
    pub(crate) fn inscription_proposed(&mut self, text: &str) -> Result<Value, String> {
        let app = crate::asking_app()?;
        let now = provider::now_ms();
        let request = request(&app, text, now)?;
        let view = provider::app_view(&request);
        let store = self.store();
        store.add(request, now)?;
        store.save();
        Ok(view)
    }

    /// See `NamesModule::pending_inscriptions`.
    pub(crate) fn inscriptions_pending(&mut self) -> Result<Value, String> {
        let pending: Vec<Value> =
            self.store().requests.iter().filter(|r| r["state"] == "pending" && r["kind"] == provider::INSCRIBE).cloned().collect();
        if pending.is_empty() {
            return Ok(json!([]));
        }
        // Without the node (stopped, say) the requests are still listed, with no choices and
        // why, so the person can see and turn them down (pending-inscriptions-fails-node-stopped).
        let (choices, node) = match chain_choices() {
            Ok(choices) => (choices, None),
            Err(error) => (json!([]), Some(error)),
        };
        let mut views = Vec::new();
        for mut request in pending {
            let app = request["app"].as_str().unwrap_or_default().to_owned();
            request["publisher"] = self.publisher_of(&app);
            request["choices"] = choices.clone();
            if let Some(node) = &node {
                request["node"] = Value::from(node.as_str());
            }
            views.push(request);
        }
        Ok(Value::from(views))
    }

    /// See `NamesModule::approve_inscription`.
    pub(crate) fn inscription_approved(&mut self, id: &str, from_key: &str) -> Result<Value, String> {
        let request = self.store().get(id).cloned().ok_or("there is no such request")?;
        if request["kind"] != provider::INSCRIBE {
            return Err("that request isn't an inscription".to_owned());
        }
        if request["state"] != "pending" {
            return Err(format!("that request is {} already", request["state"].as_str().unwrap_or("decided")));
        }
        let from = chain::parse_key(from_key).ok_or("choose the chain account that inscribes it")?;
        // Checked against the node now, not a list read earlier.
        if !chain_choices()?.as_array().into_iter().flatten().any(|choice| choice["key"] == from.as_str()) {
            return Err(format!("{} isn't an account your node's wallet holds", provider::short(&from)));
        }
        let text = request["text"].as_str().unwrap_or_default().to_owned();
        let now = provider::now_ms();
        let outcome = match chain_inscribe::run(&from, &text, true) {
            Ok(sent) => json!({ "state": "done", "decided": now, "account": from, "tx": sent["hash"], "fee": sent["fee"],
                "channel": sent["channel"], "msg_id": sent["msg_id"] }),
            // Refused before it was sent: nothing happened, the person may choose again.
            Err(error) if !error.contains("signed as") => return Err(error),
            // Taken by the node under another hash: it may land, so never offered again.
            Err(error) => json!({ "state": "failed", "decided": now, "account": from, "error": "the inscription may not have been sent", "problem": error }),
        };
        let store = self.store();
        let stored = store.get_mut(id).ok_or("there is no such request")?;
        for (key, value) in outcome.as_object().into_iter().flatten() {
            stored[key.as_str()] = value.clone();
        }
        let view = provider::app_view(stored);
        store.save();
        Ok(view)
    }
}

/// The chain accounts an inscription can come from: the keys the node's wallet holds,
/// `[{key, balance, label?, title?}]`, as `chain_accounts` lists them: with the label the person
/// gave the account, and without an account they hid while it holds nothing.
fn chain_choices() -> Result<Value, String> {
    let keys = crate::node::call("wallet_get_known_addresses", json!([]), crate::node::QUICK)?;
    let records = crate::config_dir().map(|dir| crate::activity::load(&crate::chain_keys::records_path(&dir))).unwrap_or_default();
    let mut held = Vec::new();
    for key in keys.as_array().into_iter().flatten().filter_map(|k| k.as_str().and_then(chain::parse_key)).take(crate::MAX_WALLET_ACCOUNTS) {
        let balance = match crate::node::call("wallet_get_balance", json!([key]), crate::node::QUICK) {
            Ok(balance) => chain::balance_of(&balance),
            Err(error) if chain::not_held(&error) => continue,
            Err(error) => return Err(error),
        };
        if balance == 0 && crate::chain_keys::hidden(&records, &key) {
            continue;
        }
        let mut choice = json!({ "key": key, "balance": balance.to_string() });
        // As `chain_accounts` names it: the person's label, and the keystore title of an
        // account made in the Names app.
        for field in ["label", "title"] {
            if let Some(text) = records.get(&key).and_then(|record| record[field].as_str()).filter(|text| !text.is_empty()) {
                choice[field] = Value::from(text);
            }
        }
        held.push(choice);
    }
    Ok(Value::from(held))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NamesModule as _;

    const FROM: &str = "e3635f207984ae779cf76b5f20714b514373f61ff96260879fe0a6d71f2dce07";
    const NOTE: &str = "f08aaa6388c8aa77e71fbc3a2a2b3edea5684e3b80bd7e778b8755bc893c0b0f";

    fn answer(method: &str, value: Value) {
        let value = if value.is_string() { value } else { Value::from(value.to_string()) };
        crate::node::ANSWERS.with(|a| a.borrow_mut().insert(method.to_owned(), json!({ "success": true, "value": value, "error": null })));
    }

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn as_app(name: &str) {
        let caller = if name == "host" {
            logos_rust_sdk::LogosCaller::HostAnchor
        } else {
            logos_rust_sdk::LogosCaller::Module { name: name.into(), instance: None }
        };
        crate::TEST_CALLER.with(|c| *c.borrow_mut() = caller);
    }

    /// A node holding FROM (one note of 100,000), with no channel yet, funding at 1,086.
    fn solo_node(text: &str) -> Vec<Value> {
        crate::node::ANSWERS.with(|a| a.borrow_mut().clear());
        let _ = std::fs::remove_dir_all(crate::config_dir().unwrap());
        let key = chain_inscribe::signing_key(&crate::config_dir().unwrap(), FROM).unwrap();
        let signer = key.verifying_key().to_bytes();
        let op = chain_inscribe::inscribe_op(&chain_inscribe::channel_of(&signer), &chain_inscribe::inscription(text), chain_inscribe::ZERO, &signer);
        let ops = vec![op, json!({ "opcode": 0, "payload": { "inputs": [NOTE], "outputs": [{ "value": 98914, "pk": FROM }] } })];
        answer("wallet_get_known_addresses", json!([FROM, "ab".repeat(32)]));
        crate::node::ANSWERS.with(|a| a.borrow_mut().insert("wallet_get_balance".into(), json!({ "success": true, "value": "100000", "error": null })));
        crate::node::ANSWERS.with(|a| {
            a.borrow_mut().insert("get_channel_state".into(), json!({ "success": false, "value": null, "error": "No channel found for id ChannelId([1])" }))
        });
        answer("wallet_get_notes", json!({ "notes": [{ "id": NOTE, "value": "100000" }] }));
        answer("wallet_fund_tx", json!({ "funded_tx": { "ops": ops }, "transfer_proof": { "ZkSig": { "pi_a": "6f" } } }));
        answer("submit_signed_transaction", json!(chain_inscribe::hex(&chain_inscribe::tx_hash(&ops).unwrap())));
        ops
    }

    #[test]
    fn an_app_asks_the_person_picks_a_chain_account_and_approves_and_it_is_inscribed() {
        let ops = solo_node("good wallet");
        let mut names = Names::default();
        as_app("testimonial_ui");
        let asked = parsed(&names.propose_inscription("good wallet".into()));
        assert_eq!(asked["value"]["state"], "pending", "{asked}");
        assert_eq!(asked["value"]["kind"], "inscribe");
        let id = asked["value"]["request"].as_str().unwrap().to_owned();
        // The app can't decide for itself.
        assert_eq!(parsed(&names.approve_inscription(id.clone(), FROM.into()))["success"], false);
        assert_eq!(parsed(&names.pending_inscriptions())["success"], false);
        as_app("host");
        let pending = parsed(&names.pending_inscriptions());
        let shown = &pending["value"][0];
        assert_eq!(shown["app"], "testimonial_ui", "{pending}");
        assert_eq!(shown["says"], SAYS);
        assert_eq!(shown["text"], "good wallet");
        assert_eq!(shown["choices"], json!([{ "key": FROM, "balance": "100000" }, { "key": "ab".repeat(32), "balance": "100000" }]));
        // The sheet's fee: the plan for the chosen account.
        assert_eq!(parsed(&names.chain_inscribe(FROM.into(), "good wallet".into(), false))["value"]["fee_estimate"], "1086");
        // A key the node doesn't hold is refused and the request stays pending.
        let stray = parsed(&names.approve_inscription(id.clone(), "cd".repeat(32)));
        assert!(stray["error"].as_str().unwrap().contains("isn't an account your node's wallet holds"), "{stray}");
        let done = parsed(&names.approve_inscription(id.clone(), FROM.into()));
        let hash = chain_inscribe::hex(&chain_inscribe::tx_hash(&ops).unwrap());
        assert_eq!(done["value"]["state"], "done", "{done}");
        assert_eq!(done["value"]["tx"], hash.as_str());
        assert_eq!(done["value"]["account"], FROM);
        assert_eq!(done["value"]["fee"], "1086");
        // The app follows its request, kept on disk (a reload reads it back).
        as_app("testimonial_ui");
        let status = parsed(&Names::default().request_status(id.clone()));
        assert_eq!((status["value"]["state"].clone(), status["value"]["tx"].clone()), (json!("done"), json!(hash)), "{status}");
        assert_eq!(status["value"]["text"], "good wallet");
        as_app("host");
        assert!(parsed(&names.approve_inscription(id, FROM.into()))["error"].as_str().unwrap().contains("done already"));
    }

    /// pending-inscriptions-fails-node-stopped: with the node stopped, pending_inscriptions
    /// failed outright, so the sheet couldn't list (or reject) a waiting request.
    #[test]
    fn waiting_requests_are_listed_while_the_node_is_stopped() {
        solo_node("x");
        let mut names = Names::default();
        as_app("testimonial_ui");
        let id = parsed(&names.propose_inscription("still waiting".into()))["value"]["request"].as_str().unwrap().to_owned();
        as_app("host");
        crate::node::ANSWERS.with(|a| {
            a.borrow_mut().insert("wallet_get_known_addresses".into(), json!({ "success": false, "value": null, "error": "node is not running" }))
        });
        let pending = parsed(&names.pending_inscriptions());
        assert_eq!(pending["success"], true, "{pending}");
        let shown = &pending["value"][0];
        assert_eq!((shown["id"].as_str(), shown["text"].as_str()), (Some(id.as_str()), Some("still waiting")), "{pending}");
        assert_eq!(shown["choices"], json!([]));
        assert_eq!(shown["node"], "Your Logos Blockchain node isn't running. Start it in Basecamp's Blockchain app.");
        // The person can still turn it down.
        assert_eq!(parsed(&names.reject_request(id.clone()))["value"]["state"], "rejected");
        assert_eq!(parsed(&names.pending_inscriptions())["value"], json!([]));
        // With the node running, choices again and no node field.
        as_app("testimonial_ui");
        parsed(&names.propose_inscription("again".into()));
        as_app("host");
        answer("wallet_get_known_addresses", json!([FROM]));
        let pending = parsed(&names.pending_inscriptions());
        assert_eq!(pending["value"][0]["choices"], json!([{ "key": FROM, "balance": "100000" }]), "{pending}");
        assert!(pending["value"][0].get("node").is_none(), "{pending}");
    }

    /// inscribe-in-generic-sheet: an inscription request was listed by `pending_requests`
    /// with the wallet's LEZ accounts as its choices, none of which can approve it. And the
    /// chain accounts offered carry the person's labels, less those hidden and empty.
    #[test]
    fn an_inscription_is_offered_chain_accounts_only_named_as_the_person_named_them() {
        solo_node("x");
        let other = "ab".repeat(32);
        let records = json!({ FROM: { "label": "Savings" }, other.as_str(): { "title": "names-1", "label": "", "hidden": true } });
        crate::save_object(&crate::chain_keys::records_path(&crate::config_dir().unwrap()), records.as_object().unwrap());
        let mut names = Names::default();
        as_app("testimonial_ui");
        let id = parsed(&names.propose_inscription("x".into()))["value"]["request"].as_str().unwrap().to_owned();
        as_app("host");
        assert_eq!(parsed(&names.pending_requests())["value"], json!([]), "not in the sheet that offers LEZ accounts");
        let lez = parsed(&names.approve_request(id.clone(), "Public/CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk".into()));
        assert_eq!(lez["success"], false, "{lez}");
        let pending = parsed(&names.pending_inscriptions());
        assert_eq!(pending["value"][0]["choices"], json!([{ "key": FROM, "balance": "100000", "label": "Savings" }, { "key": other, "balance": "100000", "title": "names-1" }]), "hidden but holding: shown");
        // Hidden and holding nothing: not offered.
        crate::node::ANSWERS.with(|a| a.borrow_mut().insert("wallet_get_balance".into(), json!({ "success": true, "value": "0", "error": null })));
        let pending = parsed(&names.pending_inscriptions());
        assert_eq!(pending["value"][0]["choices"], json!([{ "key": FROM, "balance": "0", "label": "Savings" }]), "{pending}");
        assert_eq!(parsed(&names.reject_request(id))["value"]["state"], "rejected");
    }

    #[test]
    fn a_refused_inscription_sends_nothing_and_stays_pending() {
        solo_node("x");
        let mut names = Names::default();
        as_app("testimonial_ui");
        let id = parsed(&names.propose_inscription("x".into()))["value"]["request"].as_str().unwrap().to_owned();
        assert_eq!(parsed(&names.propose_inscription(" ".into()))["error"], "a testimonial needs some text");
        as_app("host");
        crate::node::ANSWERS.with(|a| {
            a.borrow_mut().insert("wallet_fund_tx".into(), json!({ "success": false, "value": null, "error": "Wallet does not have enough funds, available=10" }))
        });
        let refused = parsed(&names.approve_inscription(id.clone(), FROM.into()));
        assert_eq!(refused["error"], "That key doesn't hold enough for this and its fee.");
        as_app("testimonial_ui");
        assert_eq!(parsed(&names.request_status(id.clone()))["value"]["state"], "pending");
        // The app may withdraw it, as any request.
        assert_eq!(parsed(&names.cancel_request(id))["value"]["state"], "cancelled");
        as_app("host");
    }
}
