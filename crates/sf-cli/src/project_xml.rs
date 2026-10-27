//! Port of `plugins/cli/project-xml.js`: offset-based edits leave the original XML (including
//! unknown metadata) intact. Only manifests go through this small tree; large mesh files use
//! `sf_io::threemf`.
//!
//! JS offsets are UTF-16 string indices, here they are byte offsets into the same (UTF-8) string;
//! they are only ever used against the string they came from.

use indexmap::IndexMap;

/// One element: `{ tag, attrs, start, open, close, end, selfClosing, children }`
/// (project-xml.js:33-37). `start..open` is the opening tag, `close..end` the closing one (for a
/// self-closing element `close` is the `/>`'s `/`... as in JS, `end - 2`).
#[derive(Clone, Debug)]
pub struct Node {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub start: usize,
    pub open: usize,
    pub close: usize,
    pub end: usize,
    pub self_closing: bool,
    pub children: Vec<Node>,
}

impl Node {
    /// `node.attrs[name]`
    pub fn attr_exact(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// `local(name)` (project-xml.js:4): `name.split(':').pop()`.
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// `escapeXML` (project-xml.js:5-6).
pub fn escape_xml(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// `decode` (project-xml.js:7-9): `s.replace(/&(#x[\da-f]+|#\d+|amp|lt|gt|quot|apos);/gi, ..)`.
/// A named entity in another case (`&AMP;`) looks its name up in a lower-case table and yields the
/// text "undefined", as the JS does.
fn decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' {
            let rest = &s[i + 1..];
            // the entity name up to the first ';'
            if let Some(semi) = rest.find(';') {
                let body = &rest[..semi];
                let lower = body.to_ascii_lowercase();
                let hex = lower
                    .strip_prefix("#x")
                    .filter(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_hexdigit()));
                let dec = body
                    .strip_prefix('#')
                    .filter(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()));
                let named = ["amp", "lt", "gt", "quot", "apos"].contains(&lower.as_str());
                if hex.is_some() || dec.is_some() || named {
                    if let Some(h) = hex {
                        // `c[0] === '#'`: the second character decides hex vs decimal; a leading
                        // 'X' is hex too (`c[1].toLowerCase() === 'x'`)
                        let n = u64::from_str_radix(h, 16).unwrap_or(u64::MAX);
                        out.push(code_point(n)?);
                    } else if let Some(d) = dec {
                        let n: u64 = d.parse().unwrap_or(u64::MAX);
                        out.push(code_point(n)?);
                    } else {
                        out.push_str(match body {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            _ => "undefined",
                        });
                    }
                    i += 1 + semi + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// `String.fromCodePoint(n)`: a surrogate is a valid JS code point but not a Rust `char`; it
/// becomes U+FFFD as the readers do for a lone surrogate.
fn code_point(n: u64) -> Result<char, String> {
    if n > 0x10FFFF {
        return Err(format!("Invalid code point {n}"));
    }
    Ok(char::from_u32(n as u32).unwrap_or('\u{FFFD}'))
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.' | b'-')
}

/// The next match of the tags regex at or after `from`:
/// `/<!--[\s\S]*?-->|<!\[CDATA\[[\s\S]*?\]\]>|<\?[\s\S]*?\?>|<\/?[A-Za-z_][\w:.-]*(?:[^"'<>]|"[^"]*"|'[^']*')*>/g`.
fn next_tag(xml: &str, from: usize) -> Option<(usize, usize)> {
    let b = xml.as_bytes();
    let mut i = from;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &xml[i..];
        for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")] {
            if rest.starts_with(open)
                && let Some(p) = rest[open.len()..].find(close)
            {
                return Some((i, i + open.len() + p + close.len()));
            }
        }
        // an element tag
        let mut j = i + 1;
        if b.get(j) == Some(&b'/') {
            j += 1;
        }
        if b.get(j).is_some_and(|&c| is_name_start(c)) {
            j += 1;
            while b.get(j).is_some_and(|&c| is_name_char(c)) {
                j += 1;
            }
            // (?:[^"'<>]|"[^"]*"|'[^']*')*>
            let mut ok = false;
            while j < b.len() {
                match b[j] {
                    b'>' => {
                        ok = true;
                        j += 1;
                        break;
                    }
                    b'<' => break,
                    q @ (b'"' | b'\'') => match xml[j + 1..].find(q as char) {
                        Some(p) => j += p + 2,
                        None => break,
                    },
                    _ => j += 1,
                }
            }
            if ok {
                return Some((i, j));
            }
        }
        i += 1;
    }
    None
}

/// The attributes of a tag: `raw.matchAll(/([^\s=<>/]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)`.
fn attributes(raw: &str) -> Result<Vec<(String, String)>, String> {
    let b = raw.as_bytes();
    let is_space = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let mut out: Vec<(String, String)> = Vec::new();
    let mut p = 0;
    while p < b.len() {
        // the name run
        let mut q = p;
        while q < b.len() && !is_space(b[q]) && !matches!(b[q], b'=' | b'<' | b'>' | b'/') {
            q += 1;
        }
        let attempt = (|| -> Option<(String, String, usize)> {
            if q == p {
                return None;
            }
            let mut r = q;
            while r < b.len() && is_space(b[r]) {
                r += 1;
            }
            if b.get(r) != Some(&b'=') {
                return None;
            }
            r += 1;
            while r < b.len() && is_space(b[r]) {
                r += 1;
            }
            let quote = *b.get(r)?;
            if quote != b'"' && quote != b'\'' {
                return None;
            }
            let close = raw[r + 1..].find(quote as char)?;
            Some((
                raw[p..q].to_string(),
                raw[r + 1..r + 1 + close].to_string(),
                r + 1 + close + 1,
            ))
        })();
        match attempt {
            Some((name, value, next)) => {
                if out.iter().any(|(k, _)| *k == name) {
                    return Err("duplicate XML attribute".to_string());
                }
                out.push((name, decode(&value)?));
                p = next;
            }
            None => {
                p += raw[p..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    Ok(out)
}

/// Port of `xmlTree` (project-xml.js:11-40): the root element of `xml`, or an error for
/// declarations with entities, unbalanced tags, duplicate attributes and anything but one root.
pub fn xml_tree(xml: &str) -> Result<Node, String> {
    let lower = xml.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err("XML declarations with entities are not supported".to_string());
    }
    // the open elements, root last; `top` collects the roots
    struct Open {
        node: Node,
    }
    let mut top: Vec<Node> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    let mut pos = 0;
    while let Some((s, e)) = next_tag(xml, pos) {
        pos = e;
        let raw = &xml[s..e];
        if (raw.starts_with("<!") || raw.starts_with("<?")) && !raw.starts_with("</") {
            continue;
        }
        let name_end = raw[1 + usize::from(raw.starts_with("</"))..]
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .map_or(raw.len(), |p| p + 1 + usize::from(raw.starts_with("</")));
        let tag = &raw[1 + usize::from(raw.starts_with("</"))..name_end];
        if raw.starts_with("</") {
            let Some(Open { mut node }) = stack.pop() else {
                return Err("unbalanced project XML".to_string());
            };
            if node.tag != tag {
                return Err("unbalanced project XML".to_string());
            }
            node.close = s;
            node.end = e;
            match stack.last_mut() {
                Some(parent) => parent.node.children.push(node),
                None => top.push(node),
            }
        } else {
            let attrs = attributes(raw)?;
            let self_closing = raw.ends_with("/>");
            let node = Node {
                tag: tag.to_string(),
                attrs,
                start: s,
                open: e,
                close: if self_closing { e - 2 } else { 0 },
                end: if self_closing { e } else { 0 },
                self_closing,
                children: Vec::new(),
            };
            if self_closing {
                match stack.last_mut() {
                    Some(parent) => parent.node.children.push(node),
                    None => top.push(node),
                }
            } else {
                stack.push(Open { node });
            }
        }
    }
    if !stack.is_empty() || top.len() != 1 {
        return Err("invalid project XML".to_string());
    }
    Ok(top.remove(0))
}

/// `children(node, tag)` (project-xml.js:42).
pub fn children<'a>(node: &'a Node, tag: &str) -> Vec<&'a Node> {
    node.children
        .iter()
        .filter(|n| local(&n.tag) == tag)
        .collect()
}

/// `child(node, tag)` (project-xml.js:43).
pub fn child<'a>(node: &'a Node, tag: &str) -> Option<&'a Node> {
    children(node, tag).into_iter().next()
}

/// `metadata(node)` (project-xml.js:44-45): `{ key: value }` of the `<metadata key= value=>`
/// children that have a (non-empty) key; a later one with the same key replaces the value in
/// place. `None` is a `value` attribute that is absent.
pub fn metadata(node: &Node) -> IndexMap<String, Option<String>> {
    let mut m = IndexMap::new();
    for n in children(node, "metadata") {
        if let Some(k) = n.attr_exact("key").filter(|k| !k.is_empty()) {
            m.insert(k.to_string(), n.attr_exact("value").map(str::to_string));
        }
    }
    m
}

/// `meta(key, value)` (project-xml.js:46).
pub fn meta(key: &str, value: &str) -> String {
    format!(
        "<metadata key=\"{}\" value=\"{}\"/>",
        escape_xml(key),
        escape_xml(value)
    )
}

/// `attr(node, name)` (project-xml.js:47): the attribute whose local name is `name`.
pub fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|(k, _)| local(k) == name)
        .map(|(_, v)| v.as_str())
}

/// One edit: `[start, end, text]`.
pub type Edit = (usize, usize, String);

/// `insert(node, text)` (project-xml.js:49-52): an edit that adds `text` as the last content of
/// `node` (a self-closing element is opened up).
pub fn insert(node: &Node, text: &str) -> Edit {
    if !node.self_closing {
        return (node.close, node.close, text.to_string());
    }
    (node.close, node.end, format!(">{text}</{}>", node.tag))
}

/// `editXML(xml, edits)` (project-xml.js:54-60): the edits applied last to first; they must not
/// overlap.
pub fn edit_xml(xml: &str, mut edits: Vec<Edit>) -> Result<String, String> {
    let mut out = xml.to_string();
    let mut last = out.len();
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (start, end, text) in edits {
        if end > last || start > end {
            return Err("overlapping project edits".to_string());
        }
        out = format!("{}{}{}", &out[..start], text, &out[end..]);
        last = start;
    }
    Ok(out)
}
