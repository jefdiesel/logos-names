//! A small spell checker for posts (`spell_check`), on the system's word list
//! (`/usr/share/dict/words` on macOS and most Linux systems), so nothing leaves
//! the machine. That list is old and holds few inflections, so a word also
//! passes if a common ending comes off it, and a short list of words the old
//! list lacks is added.

use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::OnceLock;

const WORD_LISTS: &[&str] = &["/usr/share/dict/words", "/usr/dict/words"];

/// Words the system list lacks that posts here use often.
const EXTRA: &[&str] = &[
    "app", "apps", "blockchain", "blog", "bot", "bots", "crypto", "dm", "dms", "email", "emails", "online",
    "website", "websites", "wallet", "wallets", "token", "tokens", "onchain", "testnet", "mainnet", "logos",
    "lez", "lns", "zk", "proof", "proofs", "ok", "okay", "lol", "gm", "gn", "btw", "imo", "tbh", "thanks",
    "hello", "hi", "hey", "yeah", "yep", "nope", "internet", "smartphone", "username", "login", "upload",
    "download", "uploads", "downloads", "repo", "github", "url", "urls", "cid", "cids", "devs", "dev",
    "pubkey", "privacy", "anon", "decentralized", "decentralised", "deploy", "deployed", "rollup", "node",
    "nodes", "mining", "miner", "miners", "bridge", "bridged", "sequencer", "basecamp", "storage", "chat",
    "i'm", "it's", "don't", "can't", "won't", "isn't", "aren't", "didn't", "doesn't", "that's", "there's",
    "you're", "we're", "they're", "i've", "you've", "we've", "i'll", "you'll", "we'll", "let's", "what's",
];

fn words() -> Option<&'static HashSet<String>> {
    static WORDS: OnceLock<Option<HashSet<String>>> = OnceLock::new();
    WORDS
        .get_or_init(|| {
            let text = WORD_LISTS.iter().find_map(|path| std::fs::read_to_string(path).ok())?;
            let mut set: HashSet<String> = text.lines().map(|word| word.trim().to_ascii_lowercase()).collect();
            set.extend(EXTRA.iter().map(|word| (*word).to_owned()));
            Some(set)
        })
        .as_ref()
}

/// Whether `word` (lower case) is spelled right: listed, or listed once a
/// common ending comes off.
fn known(words: &HashSet<String>, word: &str) -> bool {
    if words.contains(word) {
        return true;
    }
    let word = word.strip_suffix("'s").unwrap_or(word);
    if words.contains(word) {
        return true;
    }
    let stems: [(&str, &str); 14] = [
        ("ies", "y"), ("ied", "y"), ("es", ""), ("s", ""), ("ed", ""), ("ed", "e"), ("ing", ""), ("ing", "e"),
        ("ly", ""), ("er", ""), ("ers", ""), ("est", ""), ("ness", ""), ("ment", ""),
    ];
    stems.iter().any(|(ending, back)| {
        word.strip_suffix(ending).is_some_and(|stem| {
            let stem = format!("{stem}{back}");
            stem.len() >= 2 && words.contains(&stem)
                // A doubled last letter before -ed, -ing, -er: "stopped", "running".
                || (stem.len() >= 3 && stem.as_bytes()[stem.len() - 1] == stem.as_bytes()[stem.len() - 2]
                    && words.contains(&stem[..stem.len() - 1]))
        })
    })
}

/// Words one edit from `word` (letters swapped, one changed, dropped or added)
/// that are spelled right, the likeliest typo first.
fn suggestions(words: &HashSet<String>, word: &str) -> Vec<String> {
    let letters: Vec<char> = word.chars().collect();
    let alphabet = "abcdefghijklmnopqrstuvwxyz";
    // Each candidate keeps the kind of edit that made it: swapped letters are
    // the commonest typo, then one letter wrong, then one dropped or added.
    let mut found: std::collections::HashMap<String, u8> = std::collections::HashMap::new();
    let mut add = |candidate: String, kind: u8| {
        if candidate.len() >= 3 && candidate != word && words.contains(&candidate) {
            let best = found.entry(candidate).or_insert(kind);
            *best = (*best).min(kind);
        }
    };
    for i in 0..=letters.len() {
        if i < letters.len() {
            if i + 1 < letters.len() {
                let mut swapped = letters.clone();
                swapped.swap(i, i + 1);
                add(swapped.iter().collect(), 0);
            }
            for c in alphabet.chars() {
                let mut changed = letters.clone();
                changed[i] = c;
                add(changed.iter().collect(), 1);
            }
            let mut dropped = letters.clone();
            dropped.remove(i);
            add(dropped.iter().collect(), 2);
        }
        for c in alphabet.chars() {
            let mut added = letters.clone();
            added.insert(i, c);
            add(added.iter().collect(), 2);
        }
    }
    let first = letters.first().copied();
    let mut list: Vec<(String, u8)> = found.into_iter().collect();
    list.sort_by_key(|(candidate, kind)| (*kind, candidate.chars().next() != first, candidate.clone()));
    list.into_iter().take(3).map(|(candidate, _)| candidate).collect()
}

/// The words in `text` that look misspelled, each once, with up to three
/// suggestions: `[{word, suggestions}]`. Names, links, numbers, CIDs and words
/// in capitals or with digits are left alone. An empty list if there is no word list.
pub fn check(text: &str) -> Value {
    let Some(words) = words() else { return json!([]) };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        if token.contains("://") || token.contains('@') || token.contains('/') || token.starts_with('#') {
            continue;
        }
        let word: String = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_owned();
        if word.len() < 3 || word.len() > 30 || !word.chars().all(|c| c.is_ascii_alphabetic() || c == '\'') {
            continue;
        }
        // A word in capitals is an acronym; one capital after the first letter is a name.
        if word.chars().filter(|c| c.is_ascii_uppercase()).count() > 1 {
            continue;
        }
        let lower = word.to_ascii_lowercase();
        if known(words, &lower) || !seen.insert(lower.clone()) {
            continue;
        }
        out.push(json!({ "word": word, "suggestions": suggestions(words, &lower) }));
        if out.len() >= 12 {
            break;
        }
    }
    Value::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misspellings_are_found_and_fixed_and_good_words_pass() {
        if words().is_none() {
            return; // no word list on this machine
        }
        let found = check("I recieve teh posts, running and stopped happily. Blockchain apps! https://x.y @jef zDvZ1 NASA");
        let words_found: Vec<&str> = found.as_array().unwrap().iter().map(|f| f["word"].as_str().unwrap()).collect();
        assert_eq!(words_found, ["recieve", "teh"]);
        let fixes: Vec<&str> = found[0]["suggestions"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert!(fixes.contains(&"receive"), "{fixes:?}");
        let fixes: Vec<&str> = found[1]["suggestions"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert!(fixes.contains(&"the"), "{fixes:?}");
    }
}
