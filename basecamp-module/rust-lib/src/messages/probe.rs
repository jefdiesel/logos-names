//! The harness's receiving end (test only, not part of the module): opens what `delivery_module` delivered on
//! this key's inbox topic, as `logosctl watch delivery_module` logged it, with the Rust port of the format.
//!
//!   MSG_PROBE_KEY=<key file {secret, public}> MSG_PROBE_EVENTS=<watch log> cargo test --lib messages::probe -- --ignored --nocapture

use super::crypto;

/// base64url without padding, as the module host writes `_bytes` (also takes the standard alphabet and padding).
fn b64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[test]
#[ignore]
fn open_what_the_harness_received() {
    let key: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::env::var("MSG_PROBE_KEY").unwrap()).unwrap()).unwrap();
    let secret: [u8; 32] = crypto::unhex(key["secret"].as_str().unwrap()).unwrap().try_into().unwrap();
    let topic = crypto::inbox_topic(&crypto::public_key(&secret));
    let log = std::fs::read_to_string(std::env::var("MSG_PROBE_EVENTS").unwrap()).unwrap();
    let mut opened = 0;
    for line in log.lines() {
        let Ok(e) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if e["event"] != "messageReceived" || e["data"]["arg1"] != topic.as_str() {
            continue;
        }
        let payload = b64(e["data"]["arg2"]["_bytes"].as_str().unwrap()).unwrap();
        match crypto::open(&secret, &payload) {
            Some(m) => {
                opened += 1;
                println!("opened {} bytes on {topic} ({}): from {} key {}… authentic {} ts {} text {:?}",
                    payload.len(), e["data"]["arg3"], m.from, &m.from_key[..16], m.authentic, m.ts, m.text);
            }
            None => println!("a {}-byte message on {topic} didn't open", payload.len()),
        }
    }
    assert!(opened > 0, "nothing opened");
}

/// The harness's reply: seals `MSG_PROBE_TEXT` from this key (no name: `from` is msg-<16 hex>) to `MSG_PROBE_TO`,
/// padded as the phone does, and writes it as the module host's `_bytes` (base64url) to `MSG_PROBE_OUT`, for
/// `logosctl call delivery_module send str:<topic> json:{"_bytes":…}`.
#[test]
#[ignore]
fn seal_a_reply() {
    let key: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::env::var("MSG_PROBE_KEY").unwrap()).unwrap()).unwrap();
    let secret: [u8; 32] = crypto::unhex(key["secret"].as_str().unwrap()).unwrap().try_into().unwrap();
    let to = crypto::parse_msg_key(&std::env::var("MSG_PROBE_TO").unwrap()).unwrap();
    let from = crypto::nameless(&crypto::public_key(&secret));
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let payload = crypto::seal(&to, &from, &secret, &std::env::var("MSG_PROBE_TEXT").unwrap(), ts, true, crypto::Fresh::random().unwrap()).unwrap();
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in payload.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| (n << 8) | *b as u32) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    std::fs::write(std::env::var("MSG_PROBE_OUT").unwrap(), &out).unwrap();
    println!("sealed {} bytes from {from} to {}", payload.len(), crypto::inbox_topic(&to));
}
