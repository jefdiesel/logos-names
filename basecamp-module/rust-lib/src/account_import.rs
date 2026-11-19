//! Import account (`cli_account_import`): one PUBLIC account into the wallet in use, from its
//! signing key as a person pastes it or picks it from a file: the bare key (64 hex digits), or the
//! account's key-chain entry as a wallet store keeps it (`{"Public": {account_id, data: {ssk}}}` or
//! `{"ImportedPublic": {account_id, pub_sign_key}}`), or `{private_key, account?}`.
//!
//! The key never goes on a command line (the names tool reads it on standard input,
//! `names module account-import`), never into a log, an error, the history or a reply: every
//! refusal here is a fixed sentence, and the tool's own text is matched, never shown.
//!
//! What is overwritten, best effort: the pasted text, a picked file's text, the key taken out of a
//! parsed key chain, the request line written to the tool and the tool's stdin copy (`cli::feed`'s
//! writer thread). What is NOT: the JSON parser's own temporaries while a key chain is read, and
//! every copy outside this module (the Basecamp host's call arguments, the QML text field). A
//! private account's key chain is refused: importing one isn't in the wallet yet.

use serde_json::{json, Value};

/// What the app says after an import, once, beside the account (rfp-022 IMPORTED-ACCOUNTS-DESIGN.md).
pub const IMPORTED_NOTICE: &str = "Your recovery phrase does not bring this account back: keep its key (or a backup of this wallet) safe. Bridged ETH claimed into it can be moved to an account of your recovery phrase with Move.";
/// The warning before the paste.
pub const WARNING: &str = "Paste a key only from a wallet you own, on this computer. Anyone who sees it can spend from that account. Names keeps it in this wallet's store, like its own keys.";

/// The key and the account it says it is for, from what was pasted. Every refusal is fixed text.
pub fn key_of(text: &str) -> Result<(String, Option<String>), &'static str> {
    let t = text.trim();
    let bare = t.trim_start_matches("0x");
    if bare.len() == 64 && bare.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok((bare.to_ascii_lowercase(), None));
    }
    let mut value: Value = serde_json::from_str(t).map_err(|_| "That isn't a signing key (64 hex digits) or an account's key-chain JSON.")?;
    if value.get("Private").is_some() || value.get("ImportedPrivate").is_some() {
        return Err("That is a private account's key chain. Importing a private account isn't in the wallet yet.");
    }
    let account = |v: &Value| v["account_id"].as_str().or_else(|| v["account"].as_str()).map(|a| a.trim().trim_start_matches("Public/").to_owned());
    // The key's string is taken out of the parsed value (and overwritten once read), never cloned.
    let (slot, owner) = if value.get("Public").is_some() {
        let owner = account(&value["Public"]);
        (value.pointer_mut("/Public/data/ssk"), owner)
    } else if value.get("ImportedPublic").is_some() {
        let owner = account(&value["ImportedPublic"]);
        (value.pointer_mut("/ImportedPublic/pub_sign_key"), owner)
    } else {
        let owner = account(&value);
        (value.get_mut("private_key"), owner)
    };
    let mut taken = match slot.map(Value::take) {
        Some(Value::String(text)) => text,
        _ => String::new(),
    };
    let key = {
        let k = taken.trim().trim_start_matches("0x");
        (k.len() == 64 && k.bytes().all(|b| b.is_ascii_hexdigit())).then(|| {
            let mut out = String::with_capacity(64);
            out.extend(k.chars().map(|c| c.to_ascii_lowercase()));
            out
        })
    };
    wipe(&mut taken);
    key.map(|k| (k, owner)).ok_or("That JSON holds no public account's signing key.")
}

/// What `text` holds when it names a file (an absolute path, or a `file://` URL as a file picker
/// gives it): the file's text, at most 64 KB; else `text` itself. A file that can't be read is a
/// fixed refusal (its path is the person's own, but its content never shows).
pub fn content(text: &str) -> Result<Option<String>, &'static str> {
    let t = text.trim();
    // A file picker's URL is percent-encoded ("My%20Keys"): decoded; a bare path is taken as it is.
    let path = match t.strip_prefix("file://") {
        Some(url) => percent_decoded(url).ok_or("That file can't be read.")?,
        None => t.to_owned(),
    };
    let path = path.as_str();
    if !path.starts_with('/') || path.contains('\n') {
        return Ok(None);
    }
    let meta = std::fs::metadata(path).map_err(|_| "That file can't be read.")?;
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return Err("That isn't a key file: pick the account's key or key-chain JSON (at most 64 KB).");
    }
    std::fs::read_to_string(path).map(Some).map_err(|_| "That file can't be read as text.")
}

/// `%XX` escapes decoded (a file URL's path), or None for a bad escape or text that isn't UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The tool's refusal, in fixed words: matched on its text, never showing it.
pub fn refusal(cli_error: &str) -> &'static str {
    if cli_error.contains(crate::cli::OLDER_CLI) {
        return "This wallet's names tool can't import accounts yet: it needs a newer tool. Nothing was imported.";
    }
    if cli_error.contains("account of this wallet's own recovery phrase") {
        return "That key is an account of this wallet's own recovery phrase: add it with New account instead (it gets the same key). Nothing was imported.";
    }
    if cli_error.contains("has that account already") {
        return "This wallet has that account already. Nothing was imported.";
    }
    if cli_error.contains("for another account") {
        return "That key is for another account than the one in the JSON. Nothing was imported.";
    }
    if cli_error.contains("isn't a signing key") || cli_error.contains("isn't a usable signing key") {
        return "That isn't a usable signing key. Nothing was imported.";
    }
    "The import didn't go through. Nothing was imported."
}

/// The tool's request on standard input, written straight into one buffer sized up front (no JSON
/// value holds the key, and no growth leaves a partial copy behind). `key` is 64 hex digits.
pub fn request(key: &str, account: Option<&str>, label: &str) -> String {
    let account = serde_json::to_string(&account.map(|a| format!("Public/{a}"))).unwrap_or_else(|_| "null".to_owned());
    let label = serde_json::to_string(label).unwrap_or_else(|_| "\"\"".to_owned());
    let mut out = String::with_capacity(64 + key.len() + account.len() + label.len());
    out.push_str("{\"private_key\":\"");
    out.push_str(key);
    out.push_str("\",\"account\":");
    out.push_str(&account);
    out.push_str(",\"label\":");
    out.push_str(&label);
    out.push('}');
    out
}

/// Overwrites a string's bytes (best effort) and empties it.
pub fn wipe(text: &mut String) {
    // SAFETY: zero bytes are valid UTF-8, and the string is emptied right after.
    unsafe { text.as_bytes_mut() }.fill(0);
    text.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "5ec2e7e1c0ffee5ec2e7e1c0ffee5ec2e7e1c0ffee5ec2e7e1c0ffee5ec2e7e1";

    #[test]
    fn a_key_is_read_from_every_form_and_nothing_else() {
        assert_eq!(key_of(KEY), Ok((KEY.to_owned(), None)));
        assert_eq!(key_of(&format!(" 0x{} ", KEY.to_uppercase())), Ok((KEY.to_owned(), None)));
        let public = json!({ "Public": { "account_id": "AbC", "data": { "ssk": KEY } } }).to_string();
        assert_eq!(key_of(&public), Ok((KEY.to_owned(), Some("AbC".to_owned()))));
        let imported = json!({ "ImportedPublic": { "account_id": "AbC", "pub_sign_key": KEY } }).to_string();
        assert_eq!(key_of(&imported), Ok((KEY.to_owned(), Some("AbC".to_owned()))));
        assert_eq!(key_of(&json!({ "private_key": KEY, "account": "Public/AbC" }).to_string()), Ok((KEY.to_owned(), Some("AbC".to_owned()))));
        assert!(key_of(&json!({ "Private": {} }).to_string()).unwrap_err().contains("private account"));
        assert!(key_of(&json!({ "Public": { "data": {} } }).to_string()).is_err());
        assert!(key_of(&KEY[1..]).is_err());
        assert!(key_of("hello").is_err());
        // No refusal carries what was pasted.
        for bad in [&KEY[1..], "{\"Public\":{\"data\":{\"ssk\":\"5ec2e7\"}}}", "{"] {
            let said = key_of(bad).unwrap_err();
            assert!(!said.contains("5ec2e7"), "{said}");
        }
    }

    #[test]
    fn the_request_line_is_the_json_the_tool_reads() {
        let line = request(KEY, Some("AbC"), "Old \"wallet\"");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v, json!({ "private_key": KEY, "account": "Public/AbC", "label": "Old \"wallet\"" }));
        assert_eq!(serde_json::from_str::<Value>(&request(KEY, None, "")).unwrap()["account"], Value::Null);
        // Sized up front: never grown (a growth copies the key and frees the old buffer unwiped).
        assert!(line.capacity() >= line.len() && line.capacity() <= 64 + KEY.len() + 64);
    }

    #[test]
    fn the_tools_refusals_are_said_in_fixed_words() {
        assert!(refusal(&format!("x {} y", crate::cli::OLDER_CLI)).contains("newer tool"));
        assert!(refusal("the names CLI failed: this wallet has that account already; nothing was imported").contains("already"));
        assert!(refusal("the names CLI failed: that key is for another account than the one given").contains("another account"));
        assert!(refusal("the names CLI failed: that key is an account of this wallet's own recovery phrase; add it with New account instead").contains("New account"));
        assert_eq!(refusal(&format!("the names CLI failed: something {KEY}")), "The import didn't go through. Nothing was imported.");
    }

    #[test]
    fn a_picked_file_is_read_and_nothing_else_is_a_path() {
        let dir = std::env::temp_dir().join(format!("names-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("my key.json");
        std::fs::write(&file, json!({ "ImportedPublic": { "account_id": "AbC", "pub_sign_key": KEY } }).to_string()).unwrap();
        let read = content(&format!("file://{}", file.display().to_string().replace(' ', "%20"))).unwrap().unwrap();
        assert_eq!(percent_decoded("a%2Fb%zz"), None);
        assert_eq!(key_of(&read), Ok((KEY.to_owned(), Some("AbC".to_owned()))));
        assert_eq!(content(&file.display().to_string()).unwrap().as_deref(), Some(read.as_str()));
        assert_eq!(content(KEY), Ok(None));
        assert!(content("/no/such/file").is_err());
        assert!(content(&dir.display().to_string()).is_err(), "a folder isn't a key file");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wiped_string_is_empty_and_its_bytes_were_zeroed() {
        let mut s = KEY.to_owned();
        let at = s.as_ptr();
        let capacity = s.capacity();
        wipe(&mut s);
        assert!(s.is_empty());
        // The same buffer, now zeros (read within its capacity).
        let bytes = unsafe { std::slice::from_raw_parts(at, capacity.min(64)) };
        assert!(bytes.iter().all(|b| *b == 0));
    }
}
