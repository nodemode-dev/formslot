use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;

use super::{Filter, Record, estimate_tokens, list, sql, words};

pub const ANCHOR_BUDGET: usize = 2000;
const RESERVE_CHARS: usize = 220;
const FLOOR: usize = 200;
const STOP: [&str; 20] = [
    "the", "a", "an", "of", "to", "for", "and", "or", "in", "on", "is", "it", "with", "at", "by", "how", "do", "i",
    "we", "be",
];

#[derive(Debug, Clone)]
pub struct Query {
    pub text: String,
    pub anchor: Option<String>,
    pub kind: Option<String>,
    pub corpus_id: Option<i64>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: i64,
    pub kind: String,
    pub authority: String,
    pub status: String,
    pub anchor: Option<String>,
    pub source: String,
    pub corpus: Option<String>,
    pub title: String,
    pub text: String,
    pub score: f64,
    pub truncated: bool,
    pub full_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recalled {
    pub query: String,
    pub anchor: Option<String>,
    pub budget_tokens: Option<usize>,
    pub hits: Vec<Hit>,
    pub omitted: usize,
    pub truncated: usize,
    pub tokens: usize,
    #[serde(skip)]
    pub rendered: String,
}

fn kind_order(kind: &str) -> u8 {
    match kind {
        "declined" => 0,
        "rule" => 1,
        "tombstone" => 2,
        "decision" => 3,
        "how-to" => 4,
        "hashout" => 5,
        _ => 6,
    }
}

fn score_of(record: &Record, terms: &[String], joined: &str) -> f64 {
    let title = record.title.to_lowercase();
    let text = record.text.to_lowercase();
    let triggers = record.triggers.to_lowercase();
    let anchor = record.anchor.clone().unwrap_or_default().to_lowercase();
    let mut score = 0.0;
    for term in terms {
        if title.contains(term.as_str()) {
            score += 3.0;
        }
        if !triggers.is_empty() && triggers.contains(term.as_str()) {
            score += 3.0;
        }
        if !anchor.is_empty() && anchor.contains(term.as_str()) {
            score += 2.0;
        }
        score += text.matches(term.as_str()).count().min(5) as f64;
    }
    if terms.len() > 1 {
        let phrase = terms.join(" ");
        if text.contains(&phrase) {
            score += 4.0;
        }
        if title.contains(&phrase) {
            score += 6.0;
        }
    }
    if record.kind == "declined" && !record.triggers.is_empty() {
        let padded = format!(" {joined} ");
        for phrase in record.triggers.split(',') {
            let normal = words(phrase).join(" ");
            if !normal.is_empty() && padded.contains(&format!(" {normal} ")) {
                score += 1000.0;
                break;
            }
        }
    }
    if score > 0.0 {
        if record.authority == "owner" {
            score += 1.0;
        }
        if record.status == "stale" {
            score *= 0.5;
        }
    }
    score
}

fn corpus_names(conn: &Connection) -> crate::Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT id, name FROM corpus").map_err(sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(sql)?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, name) = row.map_err(sql)?;
        out.insert(id, name);
    }
    Ok(out)
}

fn head(n: usize, hit: &Hit) -> String {
    let anchor = hit.anchor.clone().unwrap_or_else(|| "-".to_string());
    let corpus = hit.corpus.clone().unwrap_or_else(|| "-".to_string());
    format!(
        "[{n}] {} · {} · {}\n    anchor: {anchor}\n    source: {}  (record {}, corpus {corpus})\n    {}\n",
        hit.kind, hit.authority, hit.status, hit.source, hit.id, hit.title
    )
}

fn body(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn marker(shown: usize, total: usize, source: &str) -> String {
    format!(" [truncated: {shown} of {total} characters shown; full text at {source}]")
}

fn cut(text: &str, keep: usize) -> String {
    let taken: String = text.chars().take(keep).collect();
    match taken.rfind(char::is_whitespace) {
        Some(at) if at > keep / 2 => taken[..at].trim_end().to_string(),
        _ => taken,
    }
}

fn shown_text(hit: &Hit, cap: Option<usize>) -> String {
    match cap {
        Some(limit) if limit < hit.full_chars => {
            let mark_len = marker(0, hit.full_chars, &hit.source).chars().count() + 8;
            let keep = limit.saturating_sub(mark_len).max(40);
            let kept = cut(&hit.text, keep);
            let note = marker(kept.chars().count(), hit.full_chars, &hit.source);
            format!("{kept}{note}")
        }
        _ => hit.text.clone(),
    }
}

pub fn recall(conn: &Connection, query: &Query) -> crate::Result<Recalled> {
    let filter = Filter {
        kind: query.kind.clone(),
        corpus_id: query.corpus_id,
        anchor: query.anchor.clone(),
        ..Filter::default()
    };
    let records = list(conn, &filter)?;
    let names = corpus_names(conn)?;
    let all = words(&query.text);
    let mut terms: Vec<String> = all.iter().filter(|w| !STOP.contains(&w.as_str())).cloned().collect();
    if terms.is_empty() {
        terms = all.clone();
    }
    let joined = all.join(" ");
    let mut scored: Vec<(f64, Record)> = Vec::new();
    for record in records {
        let score = if query.text.trim().is_empty() {
            1.0
        } else {
            score_of(&record, &terms, &joined)
        };
        if score > 0.0 {
            scored.push((score, record));
        }
    }
    if query.anchor.is_some() {
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| (a.1.status != "active").cmp(&(b.1.status != "active")))
                .then_with(|| kind_order(&a.1.kind).cmp(&kind_order(&b.1.kind)))
                .then_with(|| b.1.id.cmp(&a.1.id))
        });
    } else {
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| kind_order(&a.1.kind).cmp(&kind_order(&b.1.kind)))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
    }
    let mut hits: Vec<Hit> = scored
        .into_iter()
        .take(query.limit)
        .map(|(score, r)| Hit {
            id: r.id,
            kind: r.kind,
            authority: r.authority,
            status: r.status,
            anchor: r.anchor,
            source: r.source,
            corpus: r.corpus_id.and_then(|c| names.get(&c).cloned()),
            title: r.title,
            full_chars: r.text.chars().count(),
            text: r.text,
            score,
            truncated: false,
        })
        .collect();
    let budget = query.anchor.as_ref().map(|_| ANCHOR_BUDGET);
    let mut caps: Vec<Option<usize>> = vec![None; hits.len()];
    let mut omitted = 0usize;
    if let Some(tokens) = budget {
        let max_chars = (tokens * 4).saturating_sub(RESERVE_CHARS);
        let cost = |hits: &[Hit], caps: &[Option<usize>]| -> usize {
            hits.iter()
                .enumerate()
                .map(|(i, hit)| {
                    head(i + 1, hit).chars().count() + body(&shown_text(hit, caps[i])).chars().count()
                })
                .sum()
        };
        loop {
            let total = cost(&hits, &caps);
            if total <= max_chars {
                break;
            }
            let excess = total - max_chars;
            let effective = |i: usize, caps: &[Option<usize>]| -> usize {
                caps[i].map(|c| c.min(hits[i].full_chars)).unwrap_or(hits[i].full_chars)
            };
            let mut order: Vec<usize> = (0..hits.len()).collect();
            order.sort_by_key(|i| std::cmp::Reverse(effective(*i, &caps)));
            let Some(&longest) = order.first() else { break };
            let current = effective(longest, &caps);
            if current <= FLOOR {
                if hits.len() <= 1 {
                    break;
                }
                hits.pop();
                caps.pop();
                omitted += 1;
                continue;
            }
            let second = order.get(1).map(|i| effective(*i, &caps)).unwrap_or(FLOOR);
            let target = second.max(FLOOR).max(current.saturating_sub(excess + 8));
            let target = if target >= current { current - 1 } else { target };
            caps[longest] = Some(target.max(FLOOR));
        }
    }
    let mut rendered = String::new();
    let mut truncated = 0usize;
    for (i, hit) in hits.iter_mut().enumerate() {
        let shown = shown_text(hit, caps[i]);
        hit.truncated = shown != hit.text;
        if hit.truncated {
            truncated += 1;
        }
        rendered.push_str(&head(i + 1, hit));
        rendered.push_str(&body(&shown));
        hit.text = shown;
    }
    let tokens = estimate_tokens(&rendered);
    Ok(Recalled {
        query: query.text.clone(),
        anchor: query.anchor.clone(),
        budget_tokens: budget,
        hits,
        omitted,
        truncated,
        tokens,
        rendered,
    })
}
