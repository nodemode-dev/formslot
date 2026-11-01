#[derive(Debug, Clone)]
pub enum Kind {
    Object(Vec<Member>),
    Array(Vec<Node>),
    Str(String),
    Other,
}

#[derive(Debug, Clone)]
pub struct Member {
    pub key: String,
    pub key_start: usize,
    pub value: Node,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum J {
    Str(String),
    Num(i64),
    Bool(bool),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

pub struct Splice {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.at < self.bytes.len() && matches!(self.bytes[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn string(&mut self) -> Result<(usize, usize, String), String> {
        let start = self.at;
        if self.bytes.get(self.at) != Some(&b'"') {
            return Err(format!("expected a string at byte {start}"));
        }
        self.at += 1;
        while self.at < self.bytes.len() {
            match self.bytes[self.at] {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    let raw = &self.text[start..self.at];
                    let decoded: String = serde_json::from_str(raw).map_err(|e| format!("bad string at byte {start}: {e}"))?;
                    return Ok((start, self.at, decoded));
                }
                _ => self.at += 1,
            }
        }
        Err(format!("unterminated string at byte {start}"))
    }

    fn value(&mut self) -> Result<Node, String> {
        self.skip();
        let start = self.at;
        match self.bytes.get(self.at) {
            Some(b'{') => {
                self.at += 1;
                let mut members = Vec::new();
                loop {
                    self.skip();
                    if self.bytes.get(self.at) == Some(&b'}') {
                        self.at += 1;
                        break;
                    }
                    let (key_start, _, key) = self.string()?;
                    self.skip();
                    if self.bytes.get(self.at) != Some(&b':') {
                        return Err(format!("expected a colon at byte {}", self.at));
                    }
                    self.at += 1;
                    let value = self.value()?;
                    members.push(Member { key, key_start, value });
                    self.skip();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {}
                        _ => return Err(format!("expected a comma or a closing brace at byte {}", self.at)),
                    }
                }
                Ok(Node { start, end: self.at, kind: Kind::Object(members) })
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip();
                    if self.bytes.get(self.at) == Some(&b']') {
                        self.at += 1;
                        break;
                    }
                    items.push(self.value()?);
                    self.skip();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {}
                        _ => return Err(format!("expected a comma or a closing bracket at byte {}", self.at)),
                    }
                }
                Ok(Node { start, end: self.at, kind: Kind::Array(items) })
            }
            Some(b'"') => {
                let (s, e, decoded) = self.string()?;
                Ok(Node { start: s, end: e, kind: Kind::Str(decoded) })
            }
            Some(_) => {
                while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                    self.at += 1;
                }
                if self.at == start {
                    return Err(format!("unexpected character at byte {start}"));
                }
                Ok(Node { start, end: self.at, kind: Kind::Other })
            }
            None => Err("the document ends early".to_string()),
        }
    }
}

pub fn parse(text: &str) -> Result<Node, String> {
    serde_json::from_str::<serde_json::Value>(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let mut parser = Parser { bytes: text.as_bytes(), text, at: 0 };
    let node = parser.value()?;
    Ok(node)
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.kind {
            Kind::Object(members) => members.iter().find(|m| m.key == key).map(|m| &m.value),
            _ => None,
        }
    }

    pub fn is_object(&self) -> bool {
        matches!(self.kind, Kind::Object(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self.kind, Kind::Array(_))
    }

    pub fn items(&self) -> &[Node] {
        match &self.kind {
            Kind::Array(items) => items,
            _ => &[],
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            Kind::Str(s) => Some(s),
            _ => None,
        }
    }
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

impl J {
    pub fn obj(pairs: Vec<(&str, J)>) -> J {
        J::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    pub fn s(text: &str) -> J {
        J::Str(text.to_string())
    }

    pub fn compact(&self) -> String {
        match self {
            J::Str(s) => quote(s),
            J::Num(n) => n.to_string(),
            J::Bool(b) => b.to_string(),
            J::Arr(items) => format!("[{}]", items.iter().map(J::compact).collect::<Vec<_>>().join(", ")),
            J::Obj(pairs) => format!(
                "{{{}}}",
                pairs.iter().map(|(k, v)| format!("{}: {}", quote(k), v.compact())).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    pub fn pretty(&self, base: &str, unit: &str) -> String {
        match self {
            J::Arr(items) if !items.is_empty() && !items.iter().all(|i| matches!(i, J::Str(_) | J::Num(_) | J::Bool(_))) => {
                let inner = format!("{base}{unit}");
                let lines: Vec<String> = items.iter().map(|i| format!("{inner}{}", i.pretty(&inner, unit))).collect();
                format!("[\n{}\n{base}]", lines.join(",\n"))
            }
            J::Obj(pairs) if !pairs.is_empty() => {
                let inner = format!("{base}{unit}");
                let lines: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{inner}{}: {}", quote(k), v.pretty(&inner, unit)))
                    .collect();
                format!("{{\n{}\n{base}}}", lines.join(",\n"))
            }
            other => other.compact(),
        }
    }
}

pub fn line_indent(text: &str, at: usize) -> String {
    let before = &text[..at.min(text.len())];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    before[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

pub fn unit_of(text: &str, root: &Node) -> String {
    if let Kind::Object(members) = &root.kind {
        if let Some(first) = members.first() {
            let indent = line_indent(text, first.key_start);
            let only_space = text[..first.key_start].rsplit('\n').next().map(|l| l.trim().is_empty()).unwrap_or(false);
            if only_space && !indent.is_empty() {
                return indent;
            }
        }
    }
    "  ".to_string()
}

fn multiline(text: &str, from: usize, to: usize) -> bool {
    text[from..to].contains('\n')
}

pub fn append(text: &str, container: &Node, items: &[(Option<String>, J)], unit: &str) -> Option<Splice> {
    if items.is_empty() {
        return None;
    }
    let outer = line_indent(text, container.start);
    let inner = format!("{outer}{unit}");
    let render = |entry: &(Option<String>, J), base: &str| -> String {
        match &entry.0 {
            Some(key) => format!("{}: {}", quote(key), entry.1.pretty(base, unit)),
            None => entry.1.pretty(base, unit),
        }
    };
    let render_inline = |entry: &(Option<String>, J)| -> String {
        match &entry.0 {
            Some(key) => format!("{}: {}", quote(key), entry.1.compact()),
            None => entry.1.compact(),
        }
    };
    let (last_end, first_start, count) = match &container.kind {
        Kind::Object(members) => (
            members.last().map(|m| m.value.end),
            members.first().map(|m| m.key_start),
            members.len(),
        ),
        Kind::Array(elements) => (elements.last().map(|n| n.end), elements.first().map(|n| n.start), elements.len()),
        _ => return None,
    };
    let (open, close) = if container.is_object() { ("{", "}") } else { ("[", "]") };
    if count == 0 {
        let body: Vec<String> = items.iter().map(|e| format!("{inner}{}", render(e, &inner))).collect();
        return Some(Splice {
            start: container.start,
            end: container.end,
            text: format!("{open}\n{}\n{outer}{close}", body.join(",\n")),
        });
    }
    let last_end = last_end?;
    let first_start = first_start?;
    if multiline(text, container.start, first_start) {
        let member_indent = line_indent(text, first_start);
        let body: Vec<String> = items.iter().map(|e| format!("{member_indent}{}", render(e, &member_indent))).collect();
        Some(Splice { start: last_end, end: last_end, text: format!(",\n{}", body.join(",\n")) })
    } else {
        let body: Vec<String> = items.iter().map(render_inline).collect();
        Some(Splice { start: last_end, end: last_end, text: format!(", {}", body.join(", ")) })
    }
}

pub fn apply(text: &str, mut splices: Vec<Splice>) -> String {
    splices.sort_by(|a, b| b.start.cmp(&a.start));
    let mut out = text.to_string();
    for s in splices {
        out.replace_range(s.start..s.end, &s.text);
    }
    out
}
