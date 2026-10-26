//! Reading text out of a flat ODT made by LibreOffice's PDF import, where every run of text is
//! a positioned `draw:frame`. Frames are sorted by page, then top to bottom, then left to right,
//! and frames on the same line are joined with spaces.

struct Frame {
    page: u32,
    x: f64,
    y: f64,
    h: f64,
    text: String,
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Length in points ("1.5in", "2cm", "10mm", "12pt").
fn length_pt(v: &str) -> f64 {
    let v = v.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(v.len());
    let n: f64 = v[..split].parse().unwrap_or(0.0);
    match &v[split..] {
        "in" => n * 72.0,
        "cm" => n * 72.0 / 2.54,
        "mm" => n * 72.0 / 25.4,
        "pc" => n * 12.0,
        _ => n,
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
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

/// Text content of an XML fragment: paragraphs become lines; ODF space/tab/break elements
/// become their characters; all other tags are dropped.
fn fragment_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        out.push_str(&decode_entities(&rest[..lt]));
        let Some(gt) = rest[lt..].find('>') else { break };
        let tag = &rest[lt + 1..lt + gt];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        match name {
            "text:s" => {
                let n: usize = attr(tag, "text:c").and_then(|c| c.parse().ok()).unwrap_or(1);
                out.push_str(&" ".repeat(n));
            }
            "text:tab" => out.push('\t'),
            "text:line-break" => out.push('\n'),
            "text:p" | "text:h" if tag.starts_with('/') => out.push('\n'),
            _ => {}
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(&decode_entities(rest));
    out.trim_matches('\n').to_string()
}

pub fn frames_text(xml: &str) -> String {
    let mut frames: Vec<Frame> = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<draw:frame ") {
        rest = &rest[start..];
        let Some(tag_end) = rest.find('>') else { break };
        let tag = &rest[..tag_end];
        let Some(close) = rest.find("</draw:frame>") else { break };
        let body = &rest[tag_end + 1..close];
        let text = fragment_text(body);
        if !text.trim().is_empty() {
            frames.push(Frame {
                page: attr(tag, "text:anchor-page-number").and_then(|p| p.parse().ok()).unwrap_or(0),
                x: attr(tag, "svg:x").map(length_pt).unwrap_or(0.0),
                y: attr(tag, "svg:y").map(length_pt).unwrap_or(0.0),
                h: attr(tag, "svg:height").map(length_pt).unwrap_or(0.0),
                text,
            });
        }
        rest = &rest[close + "</draw:frame>".len()..];
    }
    frames.sort_by(|a, b| {
        a.page.cmp(&b.page).then(a.y.total_cmp(&b.y)).then(a.x.total_cmp(&b.x))
    });

    let mut out = String::new();
    let mut prev: Option<&Frame> = None;
    for f in &frames {
        match prev {
            Some(p) if p.page == f.page && (f.y - p.y).abs() < (p.h.max(f.h) / 2.0).max(1.0) => out.push(' '),
            Some(p) if p.page != f.page => out.push_str("\n\n"),
            Some(_) => out.push('\n'),
            None => {}
        }
        out.push_str(&f.text);
        prev = Some(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_read_in_page_and_position_order() {
        let xml = r#"
<draw:frame text:anchor-page-number="2" svg:x="1in" svg:y="1in" svg:height="0.2in"><draw:text-box><text:p>page two</text:p></draw:text-box></draw:frame>
<draw:frame text:anchor-page-number="1" svg:x="3in" svg:y="1in" svg:height="0.2in"><draw:text-box><text:p>right</text:p></draw:text-box></draw:frame>
<draw:frame text:anchor-page-number="1" svg:x="1in" svg:y="1in" svg:height="0.2in"><draw:text-box><text:p><text:span>left&amp;<text:s text:c="2"/>more</text:span></text:p></draw:text-box></draw:frame>
<draw:frame text:anchor-page-number="1" svg:x="1in" svg:y="2.54cm" svg:height="0.2in"><draw:text-box><text:p>A &lt;b&gt; &#233;</text:p></draw:text-box></draw:frame>
<draw:frame text:anchor-page-number="1" svg:x="1in" svg:y="2in" svg:height="0.2in"><draw:text-box><text:p>second line</text:p></draw:text-box></draw:frame>
"#;
        assert_eq!(frames_text(xml), "left&  more A <b> é right\nsecond line\n\npage two");
    }
}
