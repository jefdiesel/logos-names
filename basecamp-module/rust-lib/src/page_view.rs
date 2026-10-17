//! A name's page made safe to show inside an app (`page_view`).
//!
//! A page is HTML its holder wrote, so nothing from it may act in the app that
//! shows it. This rewrites it into the few tags Qt's rich text draws, against
//! an allowlist: headings, paragraphs, lists, tables, emphasis, code and links.
//! Every other tag is dropped (its text kept), and `script`, `style`, `title`
//! and the other raw-text elements are dropped with their content. The only
//! attribute that survives is `href` on `a`, and only for an absolute `http:`
//! or `https:` URL, so nothing points at a script, a `data:` URL or a local
//! file. Text is decoded and escaped again, every open tag is closed, and the
//! nesting depth, input and output are capped.
//!
//! This is a small tokenizer, not a full HTML parser: where it and a browser
//! would read malformed markup differently, the result is still only allowed
//! tags and escaped text, which is all an app ever sees.

use serde_json::{json, Value};

/// Only this much of a page is read; the rest is cut (on a character boundary).
pub const MAX_INPUT: usize = 64 * 1024;
/// Output stops growing here, then every open tag is closed.
pub const MAX_OUTPUT: usize = 128 * 1024;
/// Tags nested deeper than this are dropped, their text kept.
const MAX_DEPTH: usize = 32;
const MAX_TITLE: usize = 200;
const MAX_HREF: usize = 2048;

/// Tags kept, besides the void `br` and `hr`.
const ALLOWED: [&str; 26] = [
    "h1", "h2", "h3", "h4", "p", "ul", "ol", "li", "b", "strong", "i", "em", "code", "pre", "blockquote",
    "table", "thead", "tbody", "tr", "th", "td", "span", "div", "a", "u", "small",
];
const VOID: [&str; 2] = ["br", "hr"];
/// Elements whose content is not markup: it is skipped up to the closing tag
/// (a `title` is kept aside as the page's title).
const RAW_TEXT: [&str; 10] =
    ["script", "style", "title", "textarea", "xmp", "iframe", "noembed", "noframes", "noscript", "plaintext"];

/// A page made safe to show.
#[derive(Debug, PartialEq)]
pub struct Sanitized {
    pub html: String,
    pub title: Option<String>,
    /// The page was longer than `MAX_INPUT`, or its output reached `MAX_OUTPUT`.
    pub truncated: bool,
}

/// What `page_view` answers for `page` (`{content_type, bytes, text}`, as the
/// registry reads it): the same fields, plus `kind` (`html`, `text` or
/// `file`), `title`, `html` (the sanitized markup, for an HTML page only) and
/// `truncated`. `text` stays the page's own source, for showing as plain text.
pub fn view(page: &Value) -> Value {
    let content_type = page["content_type"].as_str().unwrap_or("");
    let text = page["text"].as_str();
    let lower = content_type.to_ascii_lowercase();
    let kind = if lower.contains("html") {
        "html"
    } else if lower.starts_with("text/") {
        "text"
    } else {
        "file"
    };
    let sanitized = match (kind, text) {
        ("html", Some(text)) => Some(sanitize(text)),
        _ => None,
    };
    json!({
        "content_type": content_type,
        "bytes": page["bytes"].clone(),
        "text": text,
        "kind": kind,
        "title": sanitized.as_ref().and_then(|s| s.title.clone()),
        "html": sanitized.as_ref().map(|s| s.html.clone()),
        "truncated": sanitized.as_ref().is_some_and(|s| s.truncated) || (kind == "text" && text.is_some_and(|t| t.len() > MAX_INPUT)),
    })
}

/// Rewrites untrusted HTML into the allowlisted subset (see the module docs).
pub fn sanitize(source: &str) -> Sanitized {
    let mut cut = source.len().min(MAX_INPUT);
    while !source.is_char_boundary(cut) {
        cut -= 1;
    }
    let input = &source[..cut];
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut open: Vec<&'static str> = Vec::new();
    let mut title = None;
    let mut truncated = cut < source.len();
    let mut i = 0;
    while i < bytes.len() {
        if out.len() >= MAX_OUTPUT {
            truncated = true;
            break;
        }
        if bytes[i] != b'<' {
            let end = find_byte(bytes, i, b'<').unwrap_or(bytes.len());
            push_text(&mut out, &input[i..end]);
            i = end;
            continue;
        }
        let next = bytes.get(i + 1).copied();
        if input[i..].starts_with("<!--") {
            i = find(bytes, i + 4, b"-->").map_or(bytes.len(), |at| at + 3);
        } else if matches!(next, Some(b'!' | b'?')) {
            i = find_byte(bytes, i, b'>').map_or(bytes.len(), |at| at + 1);
        } else if next == Some(b'/') && bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
            let (name, after) = tag_name(bytes, i + 2);
            i = find_byte(bytes, after, b'>').map_or(bytes.len(), |at| at + 1);
            close(&mut out, &mut open, &name);
        } else if next.is_some_and(|b| b.is_ascii_alphabetic()) {
            let (name, attributes, after) = start_tag(input, i + 1);
            i = after;
            if let Some(raw) = RAW_TEXT.iter().find(|raw| **raw == name) {
                let (content, after) = raw_text(input, i, raw);
                if *raw == "title" && title.is_none() {
                    title = clean_title(content);
                }
                i = after;
            } else {
                start(&mut out, &mut open, &name, &attributes);
            }
        } else {
            out.push_str("&lt;");
            i += 1;
        }
    }
    while let Some(tag) = open.pop() {
        push_close(&mut out, tag);
    }
    Sanitized { html: out.trim().to_owned(), title, truncated }
}

fn find_byte(bytes: &[u8], from: usize, byte: u8) -> Option<usize> {
    bytes.get(from..)?.iter().position(|b| *b == byte).map(|at| from + at)
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|at| from + at)
}

/// The lowercased tag name at `from`, and where it ends.
fn tag_name(bytes: &[u8], from: usize) -> (String, usize) {
    let mut end = from;
    while end < bytes.len() && !matches!(bytes[end], b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' | b'/' | b'>') {
        end += 1;
    }
    (String::from_utf8_lossy(&bytes[from..end]).to_ascii_lowercase(), end)
}

/// A start tag from just after its `<`: its name, its attributes (names
/// lowercased, values raw) and the index just past its `>`.
fn start_tag(input: &str, from: usize) -> (String, Vec<(String, String)>, usize) {
    let bytes = input.as_bytes();
    let (name, mut i) = tag_name(bytes, from);
    let mut attributes = Vec::new();
    let space = |b: u8| matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ');
    loop {
        while i < bytes.len() && (space(bytes[i]) || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            return (name, attributes, i);
        }
        if bytes[i] == b'>' {
            return (name, attributes, i + 1);
        }
        let start = i;
        i += 1; // an attribute name may start with `=`
        while i < bytes.len() && !space(bytes[i]) && !matches!(bytes[i], b'/' | b'>' | b'=') {
            i += 1;
        }
        let key = input[start..i].to_ascii_lowercase();
        while i < bytes.len() && space(bytes[i]) {
            i += 1;
        }
        let mut value = String::new();
        if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < bytes.len() && space(bytes[i]) {
                i += 1;
            }
            match bytes.get(i) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let end = find_byte(bytes, i + 1, quote).unwrap_or(bytes.len());
                    value = input[i + 1..end].to_owned();
                    i = (end + 1).min(bytes.len());
                }
                _ => {
                    let start = i;
                    while i < bytes.len() && !space(bytes[i]) && bytes[i] != b'>' {
                        i += 1;
                    }
                    value = input[start..i].to_owned();
                }
            }
        }
        attributes.push((key, value));
    }
}

/// The content of a raw-text element starting at `from`, and the index just
/// past its closing tag (the end of the input if it is never closed).
fn raw_text<'a>(input: &'a str, from: usize, tag: &str) -> (&'a str, usize) {
    let bytes = input.as_bytes();
    if tag == "plaintext" {
        return (&input[from..], bytes.len());
    }
    let mut i = from;
    while let Some(at) = find(bytes, i, b"</") {
        let end = at + 2 + tag.len();
        let matches = bytes.get(at + 2..end).is_some_and(|name| name.eq_ignore_ascii_case(tag.as_bytes()));
        if matches && bytes.get(end).map_or(true, |b| matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' | b'/' | b'>')) {
            let after = find_byte(bytes, end, b'>').map_or(bytes.len(), |gt| gt + 1);
            return (&input[from..at], after);
        }
        i = at + 2;
    }
    (&input[from..], bytes.len())
}

fn clean_title(raw: &str) -> Option<String> {
    let decoded = decode(raw);
    let words: Vec<&str> = decoded.split_whitespace().collect();
    let joined: String = words.join(" ").chars().filter(|c| !c.is_control()).take(MAX_TITLE).collect();
    (!joined.is_empty()).then_some(joined)
}

fn start(out: &mut String, open: &mut Vec<&'static str>, name: &str, attributes: &[(String, String)]) {
    if let Some(void) = VOID.iter().find(|tag| **tag == name) {
        out.push('<');
        out.push_str(void);
        out.push('>');
        return;
    }
    let Some(&tag) = ALLOWED.iter().find(|tag| **tag == name) else {
        return;
    };
    // Links do not nest: a new one ends the one before, as in a browser.
    if tag == "a" && open.contains(&"a") {
        close(out, open, "a");
    }
    // The end tags HTML lets a page leave out: a list item ends the one before
    // it, a row the row before, a cell the cell before, and a block ends an
    // open paragraph.
    let implied: &[(&str, &[&str])] = match tag {
        "li" => &[("li", &["ul", "ol"])],
        "tr" => &[("tr", &["table"])],
        "td" | "th" => &[("td", &["tr", "table"]), ("th", &["tr", "table"])],
        "p" | "h1" | "h2" | "h3" | "h4" | "ul" | "ol" | "pre" | "blockquote" | "table" | "div" => {
            &[("p", &["div", "li", "td", "th", "blockquote"])]
        }
        _ => &[],
    };
    for (ends, scope) in implied {
        let inner = open.iter().rposition(|open| open == ends || scope.contains(open));
        if inner.is_some_and(|at| open[at] == *ends) {
            close(out, open, ends);
        }
    }
    if open.len() >= MAX_DEPTH {
        return;
    }
    out.push('<');
    out.push_str(tag);
    if tag == "a" {
        if let Some(href) = attributes.iter().find(|(key, _)| key == "href").and_then(|(_, value)| safe_href(value)) {
            out.push_str(" href=\"");
            out.push_str(&href.replace('&', "&amp;"));
            out.push('"');
        }
    }
    out.push('>');
    open.push(tag);
}

/// Closes `name` and anything opened inside it; a closing tag for something
/// that is not open, or not allowed, is dropped.
fn close(out: &mut String, open: &mut Vec<&'static str>, name: &str) {
    let Some(at) = open.iter().rposition(|tag| *tag == name) else {
        return;
    };
    while open.len() > at {
        let tag = open.pop().expect("an open tag");
        push_close(out, tag);
    }
}

fn push_close(out: &mut String, tag: &str) {
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

/// `value` as a link target if it is an absolute `http:` or `https:` URL.
fn safe_href(value: &str) -> Option<String> {
    // Browsers drop tabs and newlines inside a URL and spaces around it, so
    // they are removed before the scheme is checked, never after.
    let url: String = decode(value).chars().filter(|c| !c.is_ascii_whitespace() && !c.is_control()).collect();
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"))?;
    let printable = url.bytes().all(|b| (0x21..0x7f).contains(&b) && !matches!(b, b'"' | b'\'' | b'<' | b'>' | b'\\' | b'`'));
    (printable && !rest.is_empty() && !rest.starts_with('/') && url.len() <= MAX_HREF).then_some(url)
}

/// Appends `text` decoded, then escaped again, up to `MAX_OUTPUT`.
fn push_text(out: &mut String, text: &str) {
    for c in decode(text).chars() {
        if out.len() >= MAX_OUTPUT {
            return;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
}

/// Decodes character references: numeric ones and the few named ones pages
/// use. An unknown or unterminated one stays as it was written.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let reference = rest[1..].find(';').filter(|end| *end <= 10).map(|end| &rest[1..1 + end]);
        let decoded = reference.and_then(|name| {
            let c = if let Some(number) = name.strip_prefix('#') {
                let code = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => number.parse::<u32>().ok()?,
                };
                char::from_u32(code).filter(|c| *c != '\0').unwrap_or('\u{fffd}')
            } else {
                match name {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    "nbsp" => '\u{a0}',
                    "copy" => '©',
                    "mdash" => '—',
                    "ndash" => '–',
                    "hellip" => '…',
                    "middot" => '·',
                    "rarr" => '→',
                    _ => return None,
                }
            };
            Some((c, name.len() + 2))
        });
        match decoded {
            Some((c, length)) => {
                out.push(c);
                rest = &rest[length..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const JEFDIESEL: &str = include_str!("../testdata/jefdiesel.html");

    fn clean(html: &str) -> String {
        sanitize(html).html
    }

    /// Every tag in `html` is one the allowlist keeps, with no attribute but a
    /// link's `href`.
    fn only_allowed_tags(html: &str) {
        let mut rest = html;
        while let Some(at) = rest.find('<') {
            let end = rest[at..].find('>').expect("a closed tag") + at;
            let tag = &rest[at + 1..end];
            let name = tag.trim_start_matches('/').split(' ').next().unwrap();
            assert!(ALLOWED.contains(&name) || VOID.contains(&name), "tag {tag:?} in {html}");
            if let Some((_, attribute)) = tag.split_once(' ') {
                assert!(name == "a" && attribute.starts_with("href=\"http"), "attribute {attribute:?}");
            }
            rest = &rest[end + 1..];
        }
    }

    #[test]
    fn jefdiesels_page_keeps_its_words_and_loses_its_style() {
        let page = sanitize(JEFDIESEL);
        assert_eq!(page.title.as_deref(), Some("jefdiesel.lez"));
        assert!(!page.truncated);
        let html = &page.html;
        only_allowed_tags(html);
        for gone in ["style", "--bg", "data:", "img", "target", "rel=", "meta", "viewport", "<title"] {
            assert!(!html.contains(gone), "{gone} survived: {html}");
        }
        assert!(html.contains("<h1>Jef</h1>"));
        assert!(html.contains("<li><span>pay me</span><code>names pay jefdiesel.lez &lt;amount&gt;</code></li>"));
        assert!(html.contains("<a href=\"https://github.com/jefdiesel\">github.com/jefdiesel</a>"));
        assert!(html.contains("nothing between you and the chain."));
        if std::env::var_os("PRINT_JEF").is_some() {
            println!("JEF_PAGE_HTML={}", serde_json::to_string(html).unwrap());
        }
    }

    #[test]
    fn scripts_handlers_and_hostile_links_are_dropped() {
        let html = clean(concat!(
            "<script>alert(1)</script><SCRIPT type=x>alert(2)</SCRIPT >",
            "<img src=x onerror=alert(3)><p onclick=\"alert(4)\" style=\"color:red\">hi</p>",
            "<a href=\"javascript:alert(5)\">a</a><a href=\" JaVa&#x09;ScRiPt:alert(6)\">b</a>",
            "<a href='data:text/html,<b>x</b>'>c</a><a href=/local>d</a><a href=\"//evil.example\">e</a>",
            "<a href=\"https:/evil\">f</a><a href=\"https://ok.example/?a=1&amp;b=2\" onmouseover=x>g</a>",
            "<a href=\"&#104;ttps://enc.example\">h</a><a href=https://x.example\"onclick=y>i</a>",
            "<iframe src=https://x>frame text</iframe><form action=x><input name=y>field</form>",
            "<svg onload=alert(7)><text>svg</text></svg><style>p{}</style><link rel=stylesheet href=x>",
        ));
        only_allowed_tags(&html);
        for gone in ["alert", "onerror", "onclick", "style", "javascript", "data:", "local", "evil", "frame text", "<img", "<form", "<input"] {
            assert!(!html.to_ascii_lowercase().contains(gone), "{gone} survived: {html}");
        }
        assert!(html.contains("<p>hi</p>"));
        assert!(html.contains("<a>a</a><a>b</a><a>c</a><a>d</a><a>e</a><a>f</a>"));
        assert!(html.contains("<a href=\"https://ok.example/?a=1&amp;b=2\">g</a>"));
        assert!(html.contains("<a href=\"https://enc.example\">h</a>"));
        assert!(html.contains("<a>i</a>"), "{html}");
        assert!(html.contains("field") && html.contains("svg"));
    }

    #[test]
    fn malformed_markup_comes_out_balanced() {
        assert_eq!(clean("<b><i>x</b>y</i>"), "<b><i>x</i></b>y");
        assert_eq!(clean("</div></p>text</a>"), "text");
        assert_eq!(clean("<a href=https://a.example>one<a href=https://b.example>two"), "<a href=\"https://a.example\">one</a><a href=\"https://b.example\">two</a>");
        assert_eq!(clean("<<script>script>alert(1)</script>"), "&lt;");
        assert_eq!(clean("<style>p{}</sty</style>after"), "after");
        assert_eq!(clean("<script>never closed <b>bold</b>"), "");
        assert_eq!(clean("<!-- <b>hidden</b> -->shown<!-- open"), "shown");
        assert_eq!(clean("<!doctype html><?xml x?>a < b && c > d"), "a &lt; b &amp;&amp; c &gt; d");
        assert_eq!(clean("<p title=\"a > b\">q</p>"), "<p>q</p>");
        assert_eq!(clean("<p unterminated=\"x"), "<p></p>");
        assert_eq!(clean("<ul><li>one<li>two</ul>"), "<ul><li>one</li><li>two</li></ul>");
        assert_eq!(clean("<p>a<p>b<div>c</div>"), "<p>a</p><p>b</p><div>c</div>");
        assert_eq!(clean("<table><tr><td>1<td>2<tr><th>3</table>"), "<table><tr><td>1</td><td>2</td></tr><tr><th>3</th></tr></table>");
        assert_eq!(clean("<ul><li>a<ul><li>b</ul></ul>"), "<ul><li>a<ul><li>b</li></ul></li></ul>");
        assert_eq!(clean("&lt;b&gt; &amp;amp; &#0; &#xD800; &bogus; &#169;"), "&lt;b&gt; &amp;amp; \u{fffd} \u{fffd} &amp;bogus; ©");
        assert_eq!(clean("<br/><hr><br></br>"), "<br><hr><br>");
        assert_eq!(clean("<H1 CLASS=x>Big</H1><h5>small</h5>"), "<h1>Big</h1>small");
    }

    #[test]
    fn the_title_is_plain_text() {
        let page = sanitize("<title> A &amp; <b>B</b>\n  C </title><p>body</p>");
        assert_eq!(page.title.as_deref(), Some("A & <b>B</b> C"));
        assert_eq!(page.html, "<p>body</p>");
        assert_eq!(sanitize("<p>no title</p>").title, None);
        assert_eq!(sanitize(&format!("<title>{}</title>", "x".repeat(1000))).title.unwrap().len(), MAX_TITLE);
    }

    #[test]
    fn deep_and_huge_pages_are_capped() {
        let deep = "<div>".repeat(5_000) + "core" + &"</div>".repeat(5_000);
        let html = clean(&deep);
        assert_eq!(html, "<div>".repeat(MAX_DEPTH) + "core" + &"</div>".repeat(MAX_DEPTH));

        let huge = "<p>".to_owned() + &"a<b>&lt;</b>".repeat(200_000);
        let page = sanitize(&huge);
        assert!(page.truncated);
        assert!(page.html.len() <= MAX_OUTPUT + MAX_DEPTH * 16);
        only_allowed_tags(&page.html);
        assert!(page.html.ends_with("</p>"));

        let escapes = "<".repeat(MAX_INPUT * 2);
        let page = sanitize(&escapes);
        assert!(page.truncated && page.html.len() <= MAX_OUTPUT + 8);

        // A cut never splits a character.
        let wide = "é".repeat(MAX_INPUT);
        assert!(sanitize(&wide).html.chars().all(|c| c == 'é'));
    }

    #[test]
    fn view_answers_by_kind() {
        let html = view(&json!({ "content_type": "text/html; charset=utf-8", "bytes": 10, "text": "<title>t</title><b>x</b>" }));
        assert_eq!((html["kind"].as_str(), html["title"].as_str(), html["html"].as_str()), (Some("html"), Some("t"), Some("<b>x</b>")));
        assert_eq!(html["text"], "<title>t</title><b>x</b>");
        let plain = view(&json!({ "content_type": "text/plain", "bytes": 3, "text": "<b>" }));
        assert_eq!((plain["kind"].as_str(), plain["html"].is_null(), plain["text"].as_str()), (Some("text"), true, Some("<b>")));
        let file = view(&json!({ "content_type": "image/png", "bytes": 3, "text": null }));
        assert_eq!((file["kind"].as_str(), file["html"].is_null(), file["text"].is_null()), (Some("file"), true, true));
        let binary_html = view(&json!({ "content_type": "text/html", "bytes": 3, "text": null }));
        assert!(binary_html["html"].is_null());
    }
}
