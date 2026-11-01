use std::collections::HashMap;

pub const MAX_BYTES: i64 = 262_144;
pub const SECTION_LEVEL: usize = 3;
const MIN_SECTION_BYTES: usize = 24;
const WHOLE_EXTENSIONS: [&str; 10] = ["json", "toml", "yaml", "yml", "sh", "bash", "bat", "ps1", "sql", "cfg"];
const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];
const SKIPPED_NAMES: [&str; 5] = ["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "composer.lock", "poetry.lock"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Markdown,
    Whole,
    Script,
}

#[derive(Debug, Clone)]
pub struct DocUnit {
    pub kind: &'static str,
    pub name: String,
    pub qualified: String,
    pub signature: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: i64,
    pub end_line: i64,
}

pub fn base_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn mode_of(path: &str, size: i64) -> Option<Mode> {
    if size <= 0 || size > MAX_BYTES {
        return None;
    }
    if path.starts_with("docs/eval/") || path.contains("/docs/eval/") {
        return None;
    }
    let base = base_of(path);
    if SKIPPED_NAMES.contains(&base) || base.ends_with(".min.json") {
        return None;
    }
    match base.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            let ext = ext.to_ascii_lowercase();
            if MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
                Some(Mode::Markdown)
            } else if WHOLE_EXTENSIONS.contains(&ext.as_str()) {
                Some(Mode::Whole)
            } else {
                None
            }
        }
        Some(_) => None,
        None => {
            let upper = base.to_ascii_uppercase();
            if ["LICENSE", "LICENCE", "COPYING", "NOTICE", "AUTHORS", "CODEOWNERS"].contains(&upper.as_str()) {
                None
            } else {
                Some(Mode::Script)
            }
        }
    }
}

fn heading_of(line: &str) -> Option<(usize, String)> {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let trimmed = rest.trim();
    let hashes = trimmed.len() - trimmed.trim_end_matches('#').len();
    let title = if hashes > 0 {
        let head = &trimmed[..trimmed.len() - hashes];
        if head.is_empty() {
            ""
        } else if head.ends_with(char::is_whitespace) {
            head.trim_end()
        } else {
            trimmed
        }
    } else {
        trimmed
    };
    if title.is_empty() { None } else { Some((level, title.to_string())) }
}

fn fence_of(line: &str) -> Option<(char, usize)> {
    let t = line.trim_start();
    let c = t.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = t.chars().take_while(|x| *x == c).count();
    if n >= 3 { Some((c, n)) } else { None }
}

fn unique(taken: &mut HashMap<String, usize>, wanted: String) -> String {
    let count = taken.entry(wanted.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        return wanted;
    }
    let mut n = *count;
    loop {
        let candidate = format!("{wanted} ~{n}");
        if !taken.contains_key(&candidate) {
            taken.insert(candidate.clone(), 1);
            if let Some(c) = taken.get_mut(&wanted) {
                *c = n;
            }
            return candidate;
        }
        n += 1;
    }
}

pub fn markdown(path: &str, text: &str) -> Vec<DocUnit> {
    struct Open {
        title: String,
        trail: String,
        start_byte: usize,
        start_line: i64,
    }
    let base = base_of(path).to_string();
    let mut out: Vec<DocUnit> = Vec::new();
    let mut taken: HashMap<String, usize> = HashMap::new();
    let mut trail: Vec<(usize, String)> = Vec::new();
    let mut open = Open {
        title: base.clone(),
        trail: base.clone(),
        start_byte: 0,
        start_line: 1,
    };
    let mut fenced: Option<(char, usize)> = None;
    let mut offset = 0usize;
    let mut line_no = 0i64;
    let close = |open: &Open, end_byte: usize, end_line: i64, out: &mut Vec<DocUnit>, taken: &mut HashMap<String, usize>| {
        let body = text.get(open.start_byte..end_byte).unwrap_or("");
        if body.trim().len() < MIN_SECTION_BYTES {
            return;
        }
        out.push(DocUnit {
            kind: "section",
            name: open.title.clone(),
            qualified: unique(taken, open.trail.clone()),
            signature: path.to_string(),
            start_byte: open.start_byte,
            end_byte,
            start_line: open.start_line,
            end_line: end_line.max(open.start_line),
        });
    };
    for raw in text.split_inclusive('\n') {
        line_no += 1;
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some((c, n)) = fence_of(line) {
            match fenced {
                None => fenced = Some((c, n)),
                Some((oc, on)) if oc == c && n >= on && line.trim().chars().all(|x| x == c) => fenced = None,
                Some(_) => {}
            }
        } else if fenced.is_none()
            && let Some((level, title)) = heading_of(line)
            && level <= SECTION_LEVEL
        {
            close(&open, offset, line_no - 1, &mut out, &mut taken);
            trail.retain(|(l, _)| *l < level);
            trail.push((level, title.clone()));
            open = Open {
                title,
                trail: trail.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" > "),
                start_byte: offset,
                start_line: line_no,
            };
        }
        offset += raw.len();
    }
    close(&open, text.len(), line_no, &mut out, &mut taken);
    out
}

pub fn whole(path: &str, text: &str) -> Vec<DocUnit> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let base = base_of(path).to_string();
    vec![DocUnit {
        kind: "text",
        name: base.clone(),
        qualified: base,
        signature: path.to_string(),
        start_byte: 0,
        end_byte: text.len(),
        start_line: 1,
        end_line: text.lines().count().max(1) as i64,
    }]
}

pub fn script(path: &str, bytes: &[u8]) -> Vec<DocUnit> {
    if !bytes.starts_with(b"#!") {
        return Vec::new();
    }
    whole(path, &String::from_utf8_lossy(bytes))
}

pub fn units(mode: Mode, path: &str, bytes: &[u8]) -> Vec<DocUnit> {
    if bytes.contains(&0) {
        return Vec::new();
    }
    match mode {
        Mode::Markdown => markdown(path, &String::from_utf8_lossy(bytes)),
        Mode::Whole => whole(path, &String::from_utf8_lossy(bytes)),
        Mode::Script => script(path, bytes),
    }
}
