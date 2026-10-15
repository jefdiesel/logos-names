//! Reading a Logos AccountLog (LIP `identity/raw/accountlog`, editor jazzz), to check
//! that an account endorses an LNS name.
//!
//! An account is an Ed25519 key pair whose public key is its address. It publishes
//! a signed, append-only log of entries; anyone holding the address verifies the
//! log and reads off the entries still live. LNS links the two directions: a name's
//! record carries the account address (text entry `logos.account`), and the
//! account's log endorses the name under the context [`NAME_CONTEXT`]. Only when
//! both hold does a name belong to an account.
//!
//! How a log is fetched is out of scope here, as it is in the LIP.

use ed25519_dalek::{Signature, VerifyingKey};

/// The constant prefix of every payload.
pub const DOMAIN: &[u8] = b"logos:accounts:1\0";
/// The AccountLog context under which an account endorses its LNS names: namespace
/// `lns`, label `name`, value the name with its suffix, such as `jef.lez`.
pub const NAME_CONTEXT: &str = "lns.name";
/// The most payload a consumer accepts.
pub const MAX_PAYLOAD: usize = 131_072;

const OP_ADD: u8 = 0x01;
const OP_REMOVE: u8 = 0x02;
const TAG_KEY: u8 = 0x01;
const TAG_TEXT: u8 = 0x02;

/// What a live entry endorses, when this reader understands it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data {
    Ed25519Key([u8; 32]),
    Text(String),
}

/// A live, understood entry: its index in the log, context and data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub index: u32,
    pub context: String,
    pub data: Data,
}

enum Slot {
    Add(Option<(String, Data)>),
    Remove(u32),
}

/// Verifies `signed_log` (64-byte signature, then the payload) under `address` and
/// returns its typed live set: the entries not removed that this reader understands.
/// Any reason the LIP gives to reject a log rejects it here.
pub fn verify(address: &[u8; 32], signed_log: &[u8]) -> Result<Vec<Entry>, String> {
    if signed_log.len() < 64 {
        return Err("too short to hold a signature".into());
    }
    let (signature, payload) = signed_log.split_at(64);
    if payload.len() > MAX_PAYLOAD {
        return Err("payload larger than 131072 bytes".into());
    }
    let key = strict_key(address).ok_or("the address is not a valid Ed25519 key")?;
    let signature = Signature::from_slice(signature).map_err(|_| "malformed signature")?;
    // Strict: cofactorless, canonical S and points, no small-order keys.
    key.verify_strict(payload, &signature)
        .map_err(|_| "the signature does not verify under this address")?;

    let body = payload.strip_prefix(DOMAIN).ok_or("wrong domain")?;
    let mut slots = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        if rest.len() < 3 {
            return Err("truncated entry header".into());
        }
        let opcode = rest[0];
        let len = usize::from(u16::from_le_bytes([rest[1], rest[2]]));
        let entry = rest.get(3..3 + len).ok_or("entry runs past the payload")?;
        rest = &rest[3 + len..];
        slots.push(match opcode {
            OP_REMOVE => {
                let index: [u8; 4] = entry.try_into().map_err(|_| "Remove length is not 4")?;
                Slot::Remove(u32::from_le_bytes(index))
            }
            OP_ADD => Slot::Add(decode_add(entry)?),
            // An unknown opcode is an opaque live slot, never a reason to reject.
            _ => Slot::Add(None),
        });
    }

    let mut dead = vec![false; slots.len()];
    for (position, slot) in slots.iter().enumerate() {
        if let Slot::Remove(target) = slot {
            let target = *target as usize;
            if target >= position {
                return Err("a Remove targets itself or a later entry".into());
            }
            if matches!(slots[target], Slot::Remove(_)) {
                return Err("a Remove targets a Remove".into());
            }
            if dead[target] {
                return Err("a Remove targets an entry already removed".into());
            }
            dead[target] = true;
        }
    }
    Ok(slots
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !dead[*index])
        .filter_map(|(index, slot)| match slot {
            Slot::Add(Some((context, data))) => Some(Entry {
                index: index as u32,
                context,
                data,
            }),
            _ => None,
        })
        .collect())
}

/// Whether the live set endorses `name` (with its `.lez` suffix) under [`NAME_CONTEXT`].
#[must_use]
pub fn endorses_name(entries: &[Entry], name: &str) -> bool {
    entries
        .iter()
        .any(|entry| entry.context == NAME_CONTEXT && entry.data == Data::Text(name.to_owned()))
}

/// The bytes of an `Add(Text)` entry endorsing `name` under [`NAME_CONTEXT`], for an
/// account owner to append to their log and sign.
#[must_use]
pub fn name_entry(name: &str) -> Vec<u8> {
    let mut body = vec![NAME_CONTEXT.len() as u8];
    body.extend_from_slice(NAME_CONTEXT.as_bytes());
    body.push(TAG_TEXT);
    body.extend_from_slice(name.as_bytes());
    let mut entry = vec![OP_ADD];
    entry.extend_from_slice(&(body.len() as u16).to_le_bytes());
    entry.extend_from_slice(&body);
    entry
}

/// Parses an address written as the LIP requires: 64 lowercase hex characters.
pub fn parse_address(text: &str) -> Result<[u8; 32], String> {
    let valid = text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !valid {
        return Err("an account address is 64 lowercase hex characters".into());
    }
    let mut address = [0; 32];
    for (i, byte) in address.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(address)
}

fn decode_add(entry: &[u8]) -> Result<Option<(String, Data)>, String> {
    let ctx_len = usize::from(*entry.first().ok_or("empty Add")?);
    if ctx_len == 0 || 1 + ctx_len >= entry.len() {
        return Err("an Add's context is empty or leaves no room for its data".into());
    }
    let context = &entry[1..1 + ctx_len];
    check_context(context)?;
    let context = String::from_utf8(context.to_vec()).map_err(|_| "context is not ASCII")?;
    let data = &entry[2 + ctx_len..];
    Ok(match entry[1 + ctx_len] {
        TAG_KEY => {
            let key: [u8; 32] = data.try_into().map_err(|_| "an Ed25519Key is not 32 bytes")?;
            strict_key(&key).ok_or("an endorsed key is not a valid Ed25519 key")?;
            Some((context, Data::Ed25519Key(key)))
        }
        TAG_TEXT => {
            let text = String::from_utf8(data.to_vec()).map_err(|_| "a Text is not UTF-8")?;
            Some((context, Data::Text(text)))
        }
        // An unknown data variant is an opaque live slot.
        _ => None,
    })
}

/// `<namespace>.<label>`: the namespace 1-16 of `a-z0-9-`, the label 1-64 of
/// `a-z0-9-.`, each starting with a letter. A context without a full stop has no
/// namespace boundary and is invalid.
fn check_context(context: &[u8]) -> Result<(), String> {
    let dot = context.iter().position(|&b| b == b'.').ok_or("context has no namespace")?;
    let (namespace, label) = (&context[..dot], &context[dot + 1..]);
    let part = |part: &[u8], max: usize, extra: &[u8]| {
        !part.is_empty()
            && part.len() <= max
            && part[0].is_ascii_lowercase()
            && part.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || extra.contains(&b))
    };
    if part(namespace, 16, b"") && part(label, 64, b".") {
        Ok(())
    } else {
        Err("invalid context".into())
    }
}

fn strict_key(bytes: &[u8; 32]) -> Option<VerifyingKey> {
    let key = VerifyingKey::from_bytes(bytes).ok()?;
    // Reject small-order points and non-canonical encodings.
    (!key.is_weak() && key.to_bytes() == *bytes).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const KEY1: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
    const KEY2: &str = "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025";
    const D: &str = "6c6f676f733a6163636f756e74733a3100";
    const ADD1: &str = "0130000e636861742e6d6573736167696e6701";

    fn log(signature: &str, payload: &str) -> Vec<u8> {
        hex::decode(format!("{signature}{payload}")).unwrap()
    }
    fn key(hex_key: &str) -> [u8; 32] {
        hex::decode(hex_key).unwrap().try_into().unwrap()
    }
    fn address() -> [u8; 32] {
        parse_address(ADDRESS).unwrap()
    }

    // Vectors V1-V8 of the AccountLog LIP.
    #[test]
    fn v1_empty_log() {
        let signed = log("d58b9c39e92232bfa686a6b60b445168162a11ea8a47730289085f51ac161f4bb57bda68f65eb0f7e7c5dd8345fb6fb5375a65d4086385da76a383213800940e", D);
        assert_eq!(verify(&address(), &signed).unwrap(), vec![]);
    }

    #[test]
    fn v4_revoked_key_drops_out() {
        let payload = format!("{D}{ADD1}{KEY1}{ADD1}{KEY2}02040000000000");
        let signed = log("060ec48ee5c7d245417c5af6a433b7451cd325ab80547bd377b070b54871a5f68e3b207e3ea678408ff9ace88caa1cfd774cb2bdb45f27b3091fd02387fc4501", &payload);
        let live = verify(&address(), &signed).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].index, 1);
        assert_eq!(live[0].data, Data::Ed25519Key(key(KEY2)));
    }

    #[test]
    fn v5_unknown_opcode_is_counted_but_not_surfaced() {
        let payload = format!("{D}{ADD1}{KEY1}0f0400deadbeef02040000000000");
        let signed = log("f8e9e2a71ea777f111f3ccd84f6429493101de77d65b4baa3450bb1d45c591e52e100bdc7ef6b9ba8d035e464c656d21dccbfee97f52a66f9cb94c9b2cf05d00", &payload);
        assert_eq!(verify(&address(), &signed).unwrap(), vec![]);
    }

    #[test]
    fn v6_text_entries() {
        let payload = format!("{D}{ADD1}{KEY1}011a001370726f66696c652e646973706c61796e616d6502616c696365011c001370726f66696c652e646973706c61796e616d6502616c696365206a");
        let signed = log("744f0eef44732a307ea89c784d04fa34d43f0c6314c2ec5222e5c7db81a4380ba5d3326c4c25f4163432d4a267f40a9e4c2071e90308a5976f6ec897587a5f08", &payload);
        let texts: Vec<_> = verify(&address(), &signed)
            .unwrap()
            .into_iter()
            .filter(|e| e.context == "profile.displayname")
            .map(|e| e.data)
            .collect();
        assert_eq!(texts, vec![Data::Text("alice".into()), Data::Text("alice j".into())]);
    }

    #[test]
    fn v8_unknown_data_tag_is_skipped() {
        let payload = format!("{D}{ADD1}{KEY1}0114000e636861742e6d6573736167696e677fdeadbeef");
        let signed = log("8d7535a8595fdeb8c391043ecc1a2481afe9efe02dc6072dd1bea2ad58de916e2d501be2c97a93f6375652f384c50b7aa507a5960caee2ea488cafd10bf1670c", &payload);
        let live = verify(&address(), &signed).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].data, Data::Ed25519Key(key(KEY1)));
    }

    #[test]
    fn rejects_a_log_under_another_address_or_tampered() {
        let signed = log("d58b9c39e92232bfa686a6b60b445168162a11ea8a47730289085f51ac161f4bb57bda68f65eb0f7e7c5dd8345fb6fb5375a65d4086385da76a383213800940e", D);
        assert!(verify(&key(KEY1), &signed).is_err());
        let mut tampered = signed.clone();
        tampered.push(0);
        assert!(verify(&address(), &tampered).is_err());
    }

    #[test]
    fn a_name_entry_round_trips_through_a_signed_log() {
        use ed25519_dalek::{Signer as _, SigningKey};
        let signing = SigningKey::from_bytes(&key("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"));
        let mut payload = DOMAIN.to_vec();
        payload.extend(name_entry("jef.lez"));
        let mut signed = signing.sign(&payload).to_bytes().to_vec();
        signed.extend(&payload);
        let live = verify(&address(), &signed).unwrap();
        assert!(endorses_name(&live, "jef.lez"));
        assert!(!endorses_name(&live, "logos.lez"));
    }

    // Rejection vectors N1-N14 of the AccountLog LIP: each must be refused, and not
    // for its signature, which verifies.
    const REJECTED: &[(&str, &str, &str)] = &[
        ("N1 mismatched domain", "66001bafa87f11b844ef6d10f6f685d5cbb87e052c2a1c93eea806b60ef2c250f1f8c177eac8a650d65479470b9b87e67b03c326302d4eb641674e48cc462d00", "6c6f676f733a6163636f756e74733a32000130000e636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
        ("N2 small-order `Ed25519Key`", "e486e78cf17eb20d67ebc3a54db0d6dde9980a0903b5c38b05bf5420e0c8e5dd417ceaef61bf31b1e578f2a93830c1ab6fea319382b986dcfabf81cd4e74ad04", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573736167696e67010100000000000000000000000000000000000000000000000000000000000000"),
        ("N3 trailing bytes after last entry", "ab5501ace7945c63ff25df8680a64977d37af34910c781177a82acf49dceb7d732454118a603f31a649ce9d8410c356239eddc08ee9abfb821287cdf132f8d0e", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660cffff"),
        ("N4 entry len runs past payload", "c9ce34c9ae7a6448277db4f864e5fef89125b47e9a2bd9b72ffc25b967a06009d07c34650fa5f34aa5509e317bae668b9db7cebb11373ee42525f1fcf736b109", "6c6f676f733a6163636f756e74733a310001ffff00000000"),
        ("N5 Remove with wrong len", "a4ca08ef15de1ed782b32e2cf6fd50cb9e2e59158513f714f81bc16e848cb3589ed6cb168a2eb6c1d4d3afd164a77b407c821802cd6495025103d36538690b01", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c0205000000000000"),
        ("N6 Add ctx_len zero", "a60390e07cd2b1fc01197e207647c665586eed63c849b2ad42bb8ac129382ddf79057d418e00d845a354157f88287153a82a32e8ed3f0a117569c0b421218b0e", "6c6f676f733a6163636f756e74733a310001220000013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
        ("N7 context leaves no room for data_tag", "e4abccb6d6570e237fd465d2cff39ca06c7364f428518331aa8d633a46a0fc9acdeb571b09fc9ef7636e379461c179ee2be88fa342bc6a4689ce2ae5c955cc0a", "6c6f676f733a6163636f756e74733a3100010f000e636861742e6d6573736167696e67"),
        ("N8 context has illegal byte", "9bc59f4e2928a474af39caa45b379486554fa94a158ca78e27ace1b97a5727aa6c66fd954b01d956ee501af2d463226977b98b9499496fd7fabb939a1cbf600d", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573734067696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
        ("N9 context has no namespace boundary", "09457e5c68e846f1362343e4eaa7dd5371d48359328ec9987ffc80ad5d7d3a18ae46232bc4515d3584f541af36c27dfab077b1192030ce9f425e6792064c4b09", "6c6f676f733a6163636f756e74733a3100012b00096d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
        ("N10 namespace does not begin with a letter", "b3c3b71783b1da9686f485a323b455c0a4eaf78ff3a596b5e3c5af60b6a0c4b8b97a37c0f2dd0e6dad55590cfb22a91969fa08bd2b06b37d0afde0adcceabf0e", "6c6f676f733a6163636f756e74733a31000131000f31636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
        ("N11 invalid UTF-8 in Text", "d6c35f646506187cb3e1fce7fc36eb226be953ea2ec9b22b12249e67bdcd3d0331992d19347ad07c9bd0b38668d8420b93814fa4d6b8e03702e419c80cedec0b", "6c6f676f733a6163636f756e74733a31000117001370726f66696c652e646973706c61796e616d6502fffe"),
        ("N12 Remove index at its own position", "cc625f45f42f9cce8fa0fa53997fb67e5a844e3af9b8fa43804fe22a57fb2cb1075a541f815062ad85787279a0ae9b442c7968bce84da42cf1c598b0cf87260b", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c02040001000000"),
        ("N13 Remove of an already-removed entry", "aae22448caae0f4f0fcbbe0557efe2bdfbf2e1f636cbbbcfbd75bf2a7a534d7f499e3f04f36204c5f127b4b3114554c7da0c5548573eb4866f3563554db8250f", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c0204000000000002040000000000"),
        ("N14 Remove targeting a Remove", "1904b4216f8eee78035951706be188bbdd8d3250d37f2015cb81e9bc60dfa8261b152ea351d672108e22c14703fd4d3f0e3439ad68c093c5e7e69764749c5200", "6c6f676f733a6163636f756e74733a31000130000e636861742e6d6573736167696e67013d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c0204000000000002040001000000"),
    ];

    #[test]
    fn rejects_every_rejection_vector() {
        for (what, signature, payload) in REJECTED {
            let result = verify(&address(), &log(signature, payload));
            assert!(result.is_err(), "{what} was accepted");
            assert_ne!(
                result.unwrap_err(),
                "the signature does not verify under this address",
                "{what} was rejected for its signature"
            );
        }
    }

    #[test]
    fn parses_addresses_strictly() {
        assert!(parse_address(ADDRESS).is_ok());
        assert!(parse_address(&ADDRESS.to_uppercase()).is_err());
        assert!(parse_address(&format!("0x{}", &ADDRESS[2..])).is_err());
    }
}
