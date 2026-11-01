use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Item,
    Paragraph,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub line: usize,
    pub text: String,
    pub trail: Vec<(usize, String)>,
}

impl Block {
    pub fn heading(&self) -> &str {
        self.trail.last().map(|(_, text)| text.as_str()).unwrap_or("")
    }

    pub fn under(&self, matcher: &Regex) -> bool {
        self.trail.iter().any(|(_, text)| matcher.is_match(text))
    }

    pub fn deepest_level(&self) -> usize {
        self.trail.last().map(|(level, _)| *level).unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
pub struct Section {
    pub line: usize,
    pub level: usize,
    pub title: String,
    pub body: String,
}

fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

pub fn blocks(text: &str) -> Vec<Block> {
    let heading = Regex::new(r"^(#{1,6})\s+(.*?)\s*#*\s*$").expect("heading pattern");
    let item = Regex::new(r"^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$").expect("item pattern");
    let mut out: Vec<Block> = Vec::new();
    let mut trail: Vec<(usize, String)> = Vec::new();
    let mut open: Option<Block> = None;
    let mut fenced = false;
    let flush = |open: &mut Option<Block>, out: &mut Vec<Block>| {
        if let Some(block) = open.take() {
            if !block.text.trim().is_empty() {
                out.push(block);
            }
        }
    };
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        let number = index + 1;
        if is_fence(line) {
            flush(&mut open, &mut out);
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut open, &mut out);
            continue;
        }
        if let Some(caps) = heading.captures(line) {
            flush(&mut open, &mut out);
            let level = caps[1].len();
            trail.retain(|(l, _)| *l < level);
            trail.push((level, caps[2].to_string()));
            continue;
        }
        if line.trim_start().starts_with('|') {
            flush(&mut open, &mut out);
            continue;
        }
        if let Some(caps) = item.captures(line) {
            flush(&mut open, &mut out);
            open = Some(Block {
                kind: BlockKind::Item,
                line: number,
                text: caps[2].trim().to_string(),
                trail: trail.clone(),
            });
            continue;
        }
        match open.as_mut() {
            Some(block) => {
                block.text.push(' ');
                block.text.push_str(line.trim());
            }
            None => {
                open = Some(Block {
                    kind: BlockKind::Paragraph,
                    line: number,
                    text: line.trim().to_string(),
                    trail: trail.clone(),
                });
            }
        }
    }
    flush(&mut open, &mut out);
    out
}

pub fn sections(text: &str) -> Vec<Section> {
    let heading = Regex::new(r"^(#{1,6})\s+(.*?)\s*#*\s*$").expect("heading pattern");
    let mut out: Vec<Section> = Vec::new();
    let mut fenced = false;
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        if is_fence(line) {
            fenced = !fenced;
        }
        if !fenced
            && let Some(caps) = heading.captures(line)
            && !is_fence(line)
        {
            out.push(Section {
                line: index + 1,
                level: caps[1].len(),
                title: caps[2].to_string(),
                body: String::new(),
            });
            continue;
        }
        if let Some(last) = out.last_mut() {
            if !last.body.is_empty() || !line.trim().is_empty() {
                last.body.push_str(line);
                last.body.push('\n');
            }
        }
    }
    for section in out.iter_mut() {
        section.body = section.body.trim().to_string();
    }
    out
}

#[derive(Debug, Clone)]
pub struct Row {
    pub line: usize,
    pub cells: Vec<String>,
    pub trail: Vec<(usize, String)>,
}

pub fn table_rows(text: &str) -> Vec<Row> {
    let heading = Regex::new(r"^(#{1,6})\s+(.*?)\s*#*\s*$").expect("heading pattern");
    let mut out: Vec<Row> = Vec::new();
    let mut trail: Vec<(usize, String)> = Vec::new();
    let mut fenced = false;
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        if is_fence(line) {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(caps) = heading.captures(line) {
            let level = caps[1].len();
            trail.retain(|(l, _)| *l < level);
            trail.push((level, caps[2].to_string()));
            continue;
        }
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<String> = t
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        if cells.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')) {
            continue;
        }
        if cells.iter().filter(|c| !c.is_empty()).count() < 2 {
            continue;
        }
        out.push(Row {
            line: index + 1,
            cells,
            trail: trail.clone(),
        });
    }
    out
}

pub fn clip_words(text: &str, count: usize, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, word) in text.split_whitespace().enumerate() {
        if i >= count || out.chars().count() + word.chars().count() + 1 > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}
