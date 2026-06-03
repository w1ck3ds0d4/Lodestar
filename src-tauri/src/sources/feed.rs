//! Shared feed parsing for the `rss` and `weworkremotely` sources. A small pull-parser
//! over quick-xml that pulls the standard item elements plus any custom ones (We Work
//! Remotely adds `<region>`, `<type>`, `<category>`, ...) into `extra`. Handles both
//! RSS 2.0 (`<item>`, text `<link>`, `<pubDate>`, `<guid>`) and Atom 1.0 (`<entry>`,
//! `<link href>`, `<published>`/`<updated>`, `<id>`, `<content>`, `<author><name>`).

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// One feed `<item>`. Standard fields are promoted; everything else (namespaced or
/// feed-specific) lands in `extra`, keyed by its local element name.
#[derive(Debug, Default, Clone)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub guid: String,
    pub description: String,
    pub pub_date: String,
    pub extra: HashMap<String, String>,
}

/// Parse the `<item>`s out of an RSS document. Tolerant: on a malformed document it
/// returns whatever items it managed to read before the error.
pub fn parse(xml: &str) -> Vec<FeedItem> {
    // NB: trim_text is intentionally left off. quick-xml emits entity references as
    // separate `GeneralRef` events, so trimming each text run would drop the spaces
    // that sit around an entity (e.g. "home &amp; relax"). We keep raw runs and trim
    // each field once, when the item closes.
    let mut reader = Reader::from_str(xml);

    let mut items: Vec<FeedItem> = Vec::new();
    let mut cur: Option<FeedItem> = None;
    let mut tag: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(e.local_name().as_ref());
                if name == "item" || name == "entry" {
                    cur = Some(FeedItem::default());
                    tag = None;
                } else if let Some(item) = cur.as_mut() {
                    // Atom links carry the URL in an `href` attribute, not text.
                    if name == "link" && item.link.is_empty() {
                        if let Some(h) = atom_link_href(&e) {
                            item.link = h;
                        }
                    }
                    tag = Some(name);
                }
            }
            // Self-closing Atom link: <link href="..." rel="alternate"/>.
            Ok(Event::Empty(e)) => {
                let name = local_name(e.local_name().as_ref());
                if let Some(item) = cur.as_mut() {
                    if name == "link" && item.link.is_empty() {
                        if let Some(h) = atom_link_href(&e) {
                            item.link = h;
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.local_name().as_ref());
                if name == "item" || name == "entry" {
                    if let Some(it) = cur.take() {
                        push_item(it, &mut items);
                    }
                    tag = None;
                } else if tag.as_deref() == Some(name.as_str()) {
                    tag = None;
                }
            }
            // Plain text run: no entities here, those arrive as GeneralRef.
            Ok(Event::Text(t)) => {
                if let (Some(item), Some(tg)) = (cur.as_mut(), tag.as_ref()) {
                    if let Ok(txt) = t.decode() {
                        assign(item, tg, &txt);
                    }
                }
            }
            // CDATA is literal content (e.g. raw HTML); take it verbatim.
            Ok(Event::CData(t)) => {
                if let (Some(item), Some(tg)) = (cur.as_mut(), tag.as_ref()) {
                    if let Ok(txt) = t.decode() {
                        assign(item, tg, &txt);
                    }
                }
            }
            // An entity reference like &lt; / &amp; / &#39; - resolve it back to text.
            Ok(Event::GeneralRef(r)) => {
                if let (Some(item), Some(tg)) = (cur.as_mut(), tag.as_ref()) {
                    if let Some(s) = resolve_ref(&r) {
                        assign(item, tg, &s);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    items
}

/// Trim every field (internal whitespace preserved) and keep the item only if it has
/// at least one identifying field.
fn push_item(mut it: FeedItem, items: &mut Vec<FeedItem>) {
    it.title = it.title.trim().to_string();
    it.link = it.link.trim().to_string();
    it.guid = it.guid.trim().to_string();
    it.description = it.description.trim().to_string();
    it.pub_date = it.pub_date.trim().to_string();
    for v in it.extra.values_mut() {
        *v = v.trim().to_string();
    }
    if !it.title.is_empty() || !it.link.is_empty() || !it.guid.is_empty() {
        items.push(it);
    }
}

/// Resolve a general/character entity reference to its text. Handles numeric refs
/// (`&#39;`, `&#x27;`) and the five predefined XML entities; unknown named entities
/// are dropped (vanishingly rare in job feeds).
fn resolve_ref(r: &quick_xml::events::BytesRef) -> Option<String> {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        return Some(c.to_string());
    }
    let decoded = r.decode().ok()?;
    let name = decoded.trim_start_matches('&').trim_end_matches(';');
    Some(
        match name {
            "lt" => "<",
            "gt" => ">",
            "amp" => "&",
            "quot" => "\"",
            "apos" => "'",
            _ => return None,
        }
        .to_string(),
    )
}

fn local_name(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).into_owned()
}

/// The `href` of an Atom `<link>`, taking only the primary link (rel absent or
/// "alternate") so a `rel="self"`/`rel="related"` link never wins.
fn atom_link_href(e: &BytesStart) -> Option<String> {
    let mut href = None;
    let mut rel = String::new();
    for a in e.attributes().flatten() {
        match a.key.local_name().as_ref() {
            // Decode entities (a URL query may be written &amp; in the attribute).
            b"href" => {
                let raw = String::from_utf8_lossy(a.value.as_ref());
                href = Some(
                    quick_xml::escape::unescape(&raw)
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| raw.into_owned()),
                );
            }
            b"rel" => rel = String::from_utf8_lossy(a.value.as_ref()).into_owned(),
            _ => {}
        }
    }
    if rel.is_empty() || rel == "alternate" {
        href
    } else {
        None
    }
}

/// Accumulate an element's text into the matching field (text can arrive across
/// several events, e.g. when split by an entity).
fn assign(item: &mut FeedItem, tag: &str, val: &str) {
    match tag {
        "title" => item.title.push_str(val),
        // RSS `link` is text; Atom's href is captured from the attribute, so only
        // append here when the href didn't already fill it.
        "link" => item.link.push_str(val),
        // RSS `guid` and Atom `id` are both the canonical per-item identifier.
        "guid" | "id" => item.guid.push_str(val),
        // RSS `description`, Atom `summary`/`content`, and `content:encoded` (local
        // name `encoded`) all carry the body.
        "description" | "summary" | "content" | "encoded" => item.description.push_str(val),
        // RSS `pubDate`/`date` and Atom `published`/`updated`; first one wins.
        "pubDate" | "date" | "published" | "updated" => {
            if item.pub_date.is_empty() {
                item.pub_date.push_str(val);
            }
        }
        other => {
            item.extra
                .entry(other.to_string())
                .or_default()
                .push_str(val);
        }
    }
}

/// Best-effort convert a feed date to `YYYY-MM-DD`. Accepts an ISO-8601 prefix
/// ("2026-06-02T20:31:23Z") or RFC-2822 ("Tue, 02 Jun 2026 20:31:23 +0000").
pub fn to_iso_date(s: &str) -> Option<String> {
    let s = s.trim();
    let b = s.as_bytes();
    // ISO-8601: yyyy-mm-dd...
    if b.len() >= 10 && b[4] == b'-' && b[7] == b'-' {
        let head: String = s.chars().take(10).collect();
        if head.split('-').count() == 3
            && head
                .split('-')
                .all(|p| p.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(head);
        }
    }
    rfc2822_date(s)
}

/// Parse an RFC-2822 date ("Tue, 02 Jun 2026 ...", weekday optional) to `YYYY-MM-DD`.
fn rfc2822_date(s: &str) -> Option<String> {
    let rest = s.split_once(", ").map(|(_, r)| r).unwrap_or(s);
    let mut it = rest.split_whitespace();
    let day: u32 = it.next()?.parse().ok()?;
    let mon = month_num(it.next()?)?;
    let year: i32 = it.next()?.parse().ok()?;
    if !(1..=31).contains(&day) || year < 1970 {
        return None;
    }
    Some(format!("{year:04}-{mon:02}-{day:02}"))
}

fn month_num(m: &str) -> Option<u32> {
    let key: String = m.chars().take(3).collect::<String>().to_lowercase();
    Some(match key.as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse, to_iso_date};

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/">
  <channel>
    <title>Feed title (ignored)</title>
    <link>https://example.com</link>
    <item>
      <title>Express Vitamin: Product Marketing Tester</title>
      <region>Anywhere in the World</region>
      <type>Full-Time</type>
      <category>Sales and Marketing</category>
      <description>&lt;p&gt;Work from home &amp; relax&lt;/p&gt;</description>
      <pubDate>Tue, 02 Jun 2026 20:31:22 +0000</pubDate>
      <guid>https://example.com/jobs/1</guid>
      <link>https://example.com/jobs/1</link>
      <media:content url="https://example.com/logo.png"/>
    </item>
    <item>
      <title>Second Co: Engineer</title>
      <description><![CDATA[<b>Build</b> things]]></description>
      <pubDate>Wed, 03 Jun 2026 10:00:00 +0000</pubDate>
      <guid>https://example.com/jobs/2</guid>
      <link>https://example.com/jobs/2</link>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parses_items_with_standard_and_custom_fields() {
        let items = parse(SAMPLE);
        assert_eq!(items.len(), 2);
        let a = &items[0];
        assert_eq!(a.title, "Express Vitamin: Product Marketing Tester");
        assert_eq!(a.guid, "https://example.com/jobs/1");
        assert_eq!(a.link, "https://example.com/jobs/1");
        // entity-decoded, still HTML (cleaned by the adapter)
        assert_eq!(a.description, "<p>Work from home & relax</p>");
        assert_eq!(
            a.extra.get("region").map(String::as_str),
            Some("Anywhere in the World")
        );
        assert_eq!(a.extra.get("type").map(String::as_str), Some("Full-Time"));
        assert_eq!(a.pub_date, "Tue, 02 Jun 2026 20:31:22 +0000");
    }

    #[test]
    fn handles_cdata_bodies() {
        let items = parse(SAMPLE);
        assert_eq!(items[1].description, "<b>Build</b> things");
    }

    #[test]
    fn channel_level_title_is_not_an_item() {
        // Only the two <item>s, never the channel <title>/<link>.
        assert_eq!(parse(SAMPLE).len(), 2);
    }

    #[test]
    fn dates_normalize_to_iso() {
        assert_eq!(
            to_iso_date("Tue, 02 Jun 2026 20:31:22 +0000").as_deref(),
            Some("2026-06-02")
        );
        assert_eq!(
            to_iso_date("02 Jun 2026 20:31:22 +0000").as_deref(),
            Some("2026-06-02")
        );
        assert_eq!(
            to_iso_date("2026-06-02T20:31:23Z").as_deref(),
            Some("2026-06-02")
        );
        assert_eq!(to_iso_date("garbage"), None);
    }

    #[test]
    fn empty_or_broken_xml_yields_nothing() {
        assert!(parse("").is_empty());
        assert!(parse("<rss><channel><item><title>No close").is_empty());
    }

    // Atom 1.0: <entry>, <link href rel>, <id>, <published>/<updated>, <content>,
    // nested <author><name>. Covers both a self-closing and a paired <link>.
    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>A Job Board</title>
  <link href="https://board.example/" rel="self"/>
  <entry>
    <title>Senior Rust Engineer</title>
    <link href="https://board.example/self" rel="self"/>
    <link href="https://board.example/jobs/42" rel="alternate"/>
    <author><name>Globex Corp</name></author>
    <content type="html">&lt;p&gt;Build &amp; ship in Rust&lt;/p&gt;</content>
    <updated>2026-06-03T21:44:53Z</updated>
    <published>2026-06-01T09:00:00Z</published>
    <id>https://board.example/jobs/42</id>
  </entry>
  <entry>
    <title>Designer</title>
    <link href="https://board.example/jobs/43"></link>
    <content type="html"><![CDATA[<b>Make</b> things]]></content>
    <updated>2026-06-02T00:00:00Z</updated>
    <id>tag:board.example,2026:43</id>
  </entry>
</feed>"#;

    #[test]
    fn parses_atom_entries() {
        let items = parse(ATOM);
        assert_eq!(items.len(), 2, "two <entry>s, not the channel");
        let a = &items[0];
        assert_eq!(a.title, "Senior Rust Engineer");
        // The rel="alternate" link wins over rel="self".
        assert_eq!(a.link, "https://board.example/jobs/42");
        assert_eq!(a.guid, "https://board.example/jobs/42");
        assert_eq!(a.description, "<p>Build & ship in Rust</p>");
        assert_eq!(a.extra.get("name").map(String::as_str), Some("Globex Corp"));
        // updated comes first in the entry, so it wins the pub_date slot.
        assert_eq!(to_iso_date(&a.pub_date).as_deref(), Some("2026-06-03"));

        let b = &items[1];
        assert_eq!(b.link, "https://board.example/jobs/43");
        assert_eq!(b.description, "<b>Make</b> things");
        assert_eq!(b.guid, "tag:board.example,2026:43");
    }
}
