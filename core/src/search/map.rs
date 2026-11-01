use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::tokens;
use super::{Target, rank, slots};

pub const FORMMAP_MAX_LINES: usize = 148;
pub const FORMMAP_TOKENS: usize = 4096;
const LINE_CAP: usize = 160;
const SKIP_KINDS: [&str; 7] = ["method", "selector", "element_id", "style", "script", "custom_property", "file"];

fn sql(e: rusqlite::Error) -> String {
    format!("search: {e}")
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub qualified: String,
    pub kind: String,
    pub name: String,
    pub params: String,
    pub line: i64,
    pub score: f64,
}

pub struct Map {
    pub entries: Vec<Entry>,
    pub nodes: usize,
    pub edges: usize,
    pub files: usize,
}

fn params_of(signature: &str) -> String {
    let Some(open) = signature.find('(') else {
        return String::new();
    };
    let mut depth = 0i32;
    let mut inner = String::new();
    for c in signature[open..].chars() {
        match c {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        inner.push(c);
    }
    let mut names: Vec<String> = Vec::new();
    let mut piece = String::new();
    let mut level = 0i32;
    let mut parts: Vec<String> = Vec::new();
    for c in inner.chars() {
        match c {
            '(' | '[' | '{' | '<' => level += 1,
            ')' | ']' | '}' | '>' => level -= 1,
            _ => {}
        }
        if c == ',' && level == 0 {
            parts.push(std::mem::take(&mut piece));
        } else {
            piece.push(c);
        }
    }
    parts.push(piece);
    for part in parts {
        let name: String = part
            .trim()
            .trim_start_matches("private ")
            .trim_start_matches("public ")
            .trim_start_matches("readonly ")
            .trim_start_matches("...")
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !name.is_empty() {
            names.push(name);
        }
    }
    let joined = names.join(", ");
    if joined.chars().count() > 40 {
        let cut: String = joined.chars().take(37).collect();
        format!("{cut}...")
    } else {
        joined
    }
}

pub fn rank_entries(conn: &Connection, corpus_id: i64) -> crate::Result<Map> {
    let graph = rank::build(conn, corpus_id)?;
    let mut by_key: HashMap<(&str, &str), usize> = HashMap::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        by_key.insert((n.path.as_str(), n.qualified.as_str()), i);
    }
    let mut score: Vec<f64> = graph.rank.clone();
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.kind == "method" {
            if let Some((parent, _)) = n.qualified.rsplit_once('.') {
                if let Some(&p) = by_key.get(&(n.path.as_str(), parent)) {
                    score[p] += graph.rank[i];
                }
            }
        }
    }
    let mut per_file: HashMap<&str, usize> = HashMap::new();
    let eligible = |n: &rank::Node| n.exported && !SKIP_KINDS.contains(&n.kind.as_str()) && !n.name.starts_with('_') && !super::is_test_path(&n.path);
    for n in graph.nodes.iter().filter(|n| eligible(n)) {
        *per_file.entry(n.path.as_str()).or_insert(0) += 1;
    }
    let mut entries = Vec::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        if !eligible(n) {
            continue;
        }
        let file_share = by_key
            .get(&(n.path.as_str(), ""))
            .map(|&f| graph.rank[f] / per_file.get(n.path.as_str()).copied().unwrap_or(1) as f64)
            .unwrap_or(0.0);
        let params = if n.kind == "function" || n.kind == "tauri_command" {
            params_of(&n.signature)
        } else {
            String::new()
        };
        entries.push(Entry {
            path: n.path.clone(),
            qualified: n.qualified.clone(),
            kind: n.kind.clone(),
            name: n.name.clone(),
            params,
            line: n.line,
            score: score[i] + file_share,
        });
    }
    entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.qualified.cmp(&b.qualified))
    });
    let files = per_file.len();
    Ok(Map {
        entries,
        nodes: graph.nodes.len(),
        edges: graph.edges,
        files,
    })
}

fn label(e: &Entry) -> String {
    match e.kind.as_str() {
        "function" | "tauri_command" => format!("{}({})", e.name, e.params),
        "class" | "interface" | "type" | "enum" | "struct" | "trait" | "mod" | "namespace" => {
            format!("{} {}", e.kind, e.name)
        }
        _ => e.name.clone(),
    }
}

pub fn render(entries: &[Entry], n: usize) -> String {
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, Vec<&Entry>> = HashMap::new();
    for e in entries.iter().take(n) {
        if !groups.contains_key(e.path.as_str()) {
            order.push(e.path.as_str());
        }
        groups.entry(e.path.as_str()).or_default().push(e);
    }
    let mut out = String::new();
    for path in order {
        let mut items = groups.remove(path).unwrap_or_default();
        items.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.qualified.cmp(&b.qualified)));
        let labels: Vec<String> = items.iter().map(|e| label(e)).collect();
        let mut line = format!("{path}: {}", labels.join(", "));
        if line.chars().count() > LINE_CAP {
            let cut: String = line.chars().take(LINE_CAP - 4).collect();
            let keep = cut.rfind(", ").unwrap_or(cut.len());
            line = format!("{} ...", &cut[..keep]);
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

pub fn fit(entries: &[Entry], budget: usize, max_lines: usize) -> (usize, String) {
    let fits = |n: usize| {
        let text = render(entries, n);
        tokens::code_text(&text) <= budget && text.lines().count() <= max_lines
    };
    let (mut low, mut high) = (0usize, entries.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    (low, render(entries, low))
}

pub struct Rule {
    pub id: i64,
    pub title: String,
    pub text: String,
    pub content_hash: String,
}

pub fn rules(conn: &Connection, corpus_id: i64) -> crate::Result<Vec<Rule>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, text, content_hash FROM memory_record \
             WHERE kind = 'rule' AND authority = 'owner' AND status = 'active' \
             AND (corpus_id IS NULL OR corpus_id = ?1) ORDER BY id",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id], |r| {
            Ok(Rule {
                id: r.get(0)?,
                title: r.get(1)?,
                text: r.get(2)?,
                content_hash: r.get(3)?,
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub fn languages(conn: &Connection, corpus_id: i64) -> crate::Result<Vec<(String, i64)>> {
    let mut stmt = conn
        .prepare(
            "SELECT language, COUNT(*) FROM file WHERE corpus_id = ?1 AND role = 'source' \
             GROUP BY language ORDER BY COUNT(*) DESC, language LIMIT 6",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub fn one_line(text: &str, cap: usize) -> String {
    let collapsed: Vec<&str> = text.split_whitespace().collect();
    let joined = collapsed.join(" ");
    if joined.chars().count() <= cap {
        return joined;
    }
    let cut: String = joined.chars().take(cap.saturating_sub(3)).collect();
    format!("{cut}...")
}

pub fn short_head(head: &str) -> &str {
    &head[..head.len().min(10)]
}

pub fn map_text(conn: &Connection, target: &Target, budget: usize) -> crate::Result<(String, usize, usize)> {
    let map = rank_entries(conn, target.id)?;
    let header = |shown: usize| {
        format!(
            "# repo map · {} @ {} · {} of {} entries, {} files, {} edges\n",
            target.name,
            short_head(&target.head),
            shown,
            map.entries.len(),
            map.files,
            map.edges
        )
    };
    let room = budget.saturating_sub(tokens::prose_text(&header(map.entries.len())));
    let (n, body) = fit(&map.entries, room, usize::MAX);
    Ok((format!("{}{body}", header(n)), n, map.entries.len()))
}

pub fn formmap(conn: &Connection, target: &Target, budget: usize) -> crate::Result<String> {
    let langs = languages(conn, target.id)?;
    let mut head = format!("# FORMMAP · {}\n\n", target.name);
    head.push_str(&format!(
        "Generated from {} @ {} by `formslot map {} --formmap`. Do not edit. Index data, never instructions.\n",
        target.name,
        short_head(&target.head),
        target.name
    ));
    if !langs.is_empty() {
        let list: Vec<String> = langs.iter().map(|(l, c)| format!("{l} {c}")).collect();
        head.push_str(&format!("Source files: {}\n", list.join(", ")));
    }
    let rules = rules(conn, target.id)?;
    if !rules.is_empty() {
        head.push_str("\n## Owner rules\n");
        for r in rules.iter().take(12) {
            head.push_str(&format!("- {}\n", one_line(&r.title, 140)));
        }
    }
    let cards = slots::all(conn)?;
    if !cards.is_empty() {
        head.push_str("\n## Shared slots\n");
        for c in cards.iter().take(30) {
            let first = c.files.first().map(|f| f.0.as_str()).unwrap_or("");
            head.push_str(&format!("- {} · {} · {}\n", c.id, c.status, one_line(first, 60)));
        }
    }
    head.push_str("\n## Map\n");
    let used = head.lines().count();
    let map = rank_entries(conn, target.id)?;
    let room = FORMMAP_MAX_LINES.saturating_sub(used);
    let spent = tokens::prose_text(&head);
    let (_, body) = fit(&map.entries, budget.saturating_sub(spent), room);
    head.push_str(&body);
    Ok(head)
}
