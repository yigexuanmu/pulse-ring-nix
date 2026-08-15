//! TTML adapter — clean-room port of `lyric_sources.py::parse_ttml` (lines 806-839) plus
//! the `ttml` fetch source.
//!
//! TTML (a flavour of which Apple Music ships) marks timed `<p begin="..." end="...">`
//! paragraphs, optionally carrying `role="translation"`/`role="x-roman"` parallel paragraphs
//! and word-level `<span begin="...">` children. We parse this with a tiny std-only string
//! scanner (no `quick-xml` dep): split on `<p `, pull attrs by string search, collect spans
//! the same way, then merge the translation/romanisation parallels into the primary lines
//! by nearest start time (tol 2s) — the inline merge mirrors `merge_timed` privately owned
//! by the NetEase source.
//!
//! `fetch_ttml` prefers a per-track `req.ttml_url`, else the `TTML_URL` env template
//! (`{title}`/`{artist}`/`{album}` path-percent-encoded), optional `TTML_TOKEN` bearer.

use crate::lyrics::{LyricData, LyricLine, TrackRequest};

#[derive(Clone, Copy)]
enum Kind {
    Primary,
    Translation,
    Romanization,
}

/// TTML body parser (lyric_sources.py:806-839). Std-only simple scanner, no quick-xml dep.
pub(crate) fn parse_ttml(text: &str) -> Vec<LyricLine> {
    let mut built: Vec<(LyricLine, Kind)> = Vec::new();

    for segment in text.split("<p ") {
        if segment.trim().is_empty() {
            continue;
        }
        let end_idx = match segment.find("</p>") {
            Some(i) => i,
            None => continue,
        };
        let block = &segment[..end_idx];
        let gt = match block.find('>') {
            Some(i) => i,
            None => continue,
        };
        let attrs_str = &block[..gt];
        let inner = &block[gt + 1..];

        let begin = attr_value(attrs_str, "begin");
        let end = attr_value(attrs_str, "end");
        let role = role_string(attrs_str);
        let start = parse_time_expression(begin);
        let endt = parse_time_expression(end);
        let content = clean_ttml_text(inner);
        if content.is_empty() {
            continue;
        }

        let mut chars: Vec<i64> = Vec::new();
        for span_seg in inner.split("<span ") {
            if span_seg.trim().is_empty() {
                continue;
            }
            let span_end = match span_seg.find("</span>") {
                Some(i) => i,
                None => continue,
            };
            let span_block = &span_seg[..span_end];
            let span_gt = match span_block.find('>') {
                Some(i) => i,
                None => continue,
            };
            let span_attrs = &span_block[..span_gt];
            let span_raw = &span_block[span_gt + 1..];
            let span_begin = parse_time_expression(attr_value(span_attrs, "begin"));
            let span_text = clean_ttml_text(span_raw);
            if !span_text.is_empty() && span_begin >= 0 {
                let n = span_text.chars().count().max(1);
                chars.extend(std::iter::repeat(span_begin).take(n));
            }
        }

        let duration = if endt >= start && start >= 0 {
            (endt - start).max(0)
        } else {
            0
        };
        let kind = if role.to_lowercase().contains("translation") {
            Kind::Translation
        } else if role.to_lowercase().contains("roman") {
            Kind::Romanization
        } else {
            Kind::Primary
        };
        built.push((
            LyricLine {
                start_ms: start,
                duration_ms: duration,
                text: content,
                translation: String::new(),
                romanization: String::new(),
                chars,
            },
            kind,
        ));
    }

    let mut primary: Vec<LyricLine> = built
        .iter()
        .filter(|(_, k)| matches!(k, Kind::Primary))
        .map(|(l, _)| l.clone())
        .collect();
    let translations: Vec<LyricLine> = built
        .iter()
        .filter(|(_, k)| matches!(k, Kind::Translation))
        .map(|(l, _)| l.clone())
        .collect();
    let romanizations: Vec<LyricLine> = built
        .iter()
        .filter(|(_, k)| matches!(k, Kind::Romanization))
        .map(|(l, _)| l.clone())
        .collect();

    if primary.is_empty() {
        primary = if !translations.is_empty() {
            translations.clone()
        } else {
            romanizations.clone()
        };
    }

    // inline `merge_timed` mirror (netease keeps its own private copy; no cross-file dep).
    let tol: i64 = 2000;
    if !primary.is_empty() && !translations.is_empty() {
        let timed: Vec<&LyricLine> = translations.iter().filter(|l| l.start_ms >= 0).collect();
        for target in primary.iter_mut() {
            if target.start_ms < 0 || timed.is_empty() {
                continue;
            }
            let mut best = 0usize;
            let mut diff = (target.start_ms - timed[0].start_ms).abs();
            for (i, c) in timed.iter().enumerate().skip(1) {
                let d = (target.start_ms - c.start_ms).abs();
                if d < diff {
                    diff = d;
                    best = i;
                }
            }
            if diff <= tol && target.translation.is_empty() {
                target.translation = timed[best].text.clone();
            }
        }
    }
    if !primary.is_empty() && !romanizations.is_empty() {
        let timed: Vec<&LyricLine> = romanizations.iter().filter(|l| l.start_ms >= 0).collect();
        for target in primary.iter_mut() {
            if target.start_ms < 0 || timed.is_empty() {
                continue;
            }
            let mut best = 0usize;
            let mut diff = (target.start_ms - timed[0].start_ms).abs();
            for (i, c) in timed.iter().enumerate().skip(1) {
                let d = (target.start_ms - c.start_ms).abs();
                if d < diff {
                    diff = d;
                    best = i;
                }
            }
            if diff <= tol && target.romanization.is_empty() {
                target.romanization = timed[best].text.clone();
            }
        }
    }

    super::finalize(primary, 0)
}

/// `fetch_ttml`: per-track `ttml_url` (else `TTML_URL` template); optional `TTML_TOKEN`.
pub(crate) fn fetch_ttml(req: &TrackRequest) -> Result<LyricData, String> {
    let url = if !req.ttml_url.is_empty() {
        req.ttml_url.clone()
    } else {
        let template = std::env::var("TTML_URL")
            .map_err(|_| "ttml: ttml_url or env TTML_URL required".to_string())?;
        template
            .replace("{title}", &url_encode(&req.title))
            .replace("{artist}", &url_encode(&req.artist))
            .replace("{album}", &url_encode(&req.album))
    };

    // std-only scheme/host sanity check (avoids pulling the `url` crate dep).
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("ttml: invalid endpoint".to_string());
    }
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(&url);
    let host = after_scheme.split('/').next().unwrap_or("");
    if host.is_empty() || host.starts_with(':') {
        return Err("ttml: invalid endpoint".to_string());
    }

    let mut headers: Vec<(String, String)> = Vec::new();
    if let Ok(token) = std::env::var("TTML_TOKEN") {
        if !token.is_empty() {
            headers.push(("Authorization".to_string(), format!("Bearer {}", token)));
        }
    }
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let (body, _cs) = super::request_data(&url, Some(&header_refs), super::REQUEST_TIMEOUT)?;
    let text = String::from_utf8_lossy(&body);
    let lines = parse_ttml(&text);
    Ok(LyricData {
        source: "ttml".to_string(),
        lines,
    })
}

// ---- small std-only TTML helpers ----

/// Path-`url_encode` (space → `%20`), distinct from the form-style `percent_encode_form`.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Pull `key="value"` (or `key='value'`) out of an attribute blob; `""` if absent.
fn attr_value<'a>(attrs: &'a str, key: &str) -> &'a str {
    let needle = format!("{key}=\"");
    if let Some(s) = attrs.find(&needle) {
        let start = s + needle.len();
        if let Some(end) = attrs[start..].find('"') {
            return &attrs[start..start + end];
        }
    }
    let needle2 = format!("{key}='");
    if let Some(s) = attrs.find(&needle2) {
        let start = s + needle2.len();
        if let Some(end) = attrs[start..].find('\'') {
            return &attrs[start..start + end];
        }
    }
    ""
}

/// Concatenate the values of any attr whose key contains `role` (case-insensitive).
fn role_string(attrs: &str) -> String {
    let mut out = String::new();
    for chunk in attrs.split_whitespace() {
        let kv: Vec<&str> = chunk.splitn(2, '=').collect();
        if kv.len() == 2 && kv[0].to_lowercase().contains("role") {
            let v = kv[1].trim_matches(|c| c == '"' || c == '\'');
            out.push_str(v);
            out.push(' ');
        }
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(c);
        }
    }
    out
}

/// Strip tags, then html-unescape via the shared `clean_text` (handles `&#NN;`/`&`/...).
fn clean_ttml_text(s: &str) -> String {
    super::clean_text(strip_tags(s).trim())
}

/// Parse a TTML time expression: `[mm:ss.fff]`, `HH:MM:SS.fff`, `mm:ss.fff`, `Nms`, `Ns`,
/// or a bare integer (ms). Returns -1 if it can't make sense of the string.
fn parse_time_expression(s: &str) -> i64 {
    let s = s.trim();
    if s.is_empty() {
        return -1;
    }
    let s = s.trim_matches(|c| c == '[' || c == ']');
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let (h, m, sec) = match parts.len() {
            3 => (
                parts[0].parse::<i64>().unwrap_or(0),
                parts[1].parse::<i64>().unwrap_or(0),
                parts[2].parse::<f64>().unwrap_or(0.0),
            ),
            2 => (
                0,
                parts[0].parse::<i64>().unwrap_or(0),
                parts[1].parse::<f64>().unwrap_or(0.0),
            ),
            _ => return -1,
        };
        return (h * 3600 + m * 60) * 1000 + (sec * 1000.0) as i64;
    }
    if let Some(n) = s.strip_suffix("ms") {
        return n.parse::<i64>().unwrap_or(-1);
    }
    if let Some(n) = s.strip_suffix('s') {
        return (n.parse::<f64>().unwrap_or(0.0) * 1000.0) as i64;
    }
    s.parse::<i64>().unwrap_or(-1)
}
