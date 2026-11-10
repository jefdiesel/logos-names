//! Text checks the Names module and the `names` CLI share: the CLI includes this file by
//! path (like `deposit.rs` and `zones.rs`), so both read a sequencer's address and a
//! typed name the same way. It needs nothing but the standard library.

/// A sequencer URL the module and the CLI's zones will talk to: `https://`, or `http://`
/// only to this machine (a local dev chain). Plain http elsewhere would let anyone on the
/// network swap name records and payment keys.
pub fn is_http_url(url: &str) -> bool {
    // Plain URL characters only, and no `user@` part: a backslash, a space or an `@` is
    // read one way here and another by the HTTP library, which let `http://` reach any
    // host (SF-2). A sequencer address never needs them.
    let plain = |c: char| c.is_ascii_alphanumeric() || "-._~:/?#[]!$&'()*+,;=%".contains(c);
    if url.is_empty() || !url.chars().all(plain) {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return rest.split(['/', '?', '#']).next().is_some_and(|host| !host.is_empty());
    }
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(bracketed) = host.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Text an app wrote, made safe to put in front of the person: control
/// characters and the Unicode marks that reorder or hide text (bidi
/// overrides, zero-width characters) become spaces, runs of space collapse,
/// and it is cut to `max` characters with an ellipsis.
pub fn clean_text(text: &str, max: usize) -> String {
    let hidden = |c: char| {
        c.is_control()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}' | '\u{061C}')
    };
    let mut out = String::new();
    let mut space = false;
    for c in text.chars() {
        let c = if hidden(c) || c.is_whitespace() { ' ' } else { c };
        if c == ' ' {
            if !space && !out.is_empty() {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    let out = out.trim_end().to_owned();
    if out.chars().count() > max {
        let cut: String = out.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    } else {
        out
    }
}
