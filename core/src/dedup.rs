use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Instant;

use rusqlite::{Connection, params};

pub const SCHEME: u32 = 1;
pub const PERMUTATIONS: usize = 128;
pub const SHINGLE: usize = 5;
pub const GUARANTEE: usize = 30;
pub const WINDOW: usize = GUARANTEE - SHINGLE + 1;
pub const MIN_TOKENS: u32 = 30;
pub const MIN_GROUP_TOKENS: i64 = 12;
pub const BANDS: usize = 16;
pub const ROWS: usize = 8;
pub const JACCARD_MIN: f64 = 0.70;

const SEED: u64 = 0x6A09_E667_F3BC_C908;
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
const BUCKET_CAP: usize = 300;
const EXPAND_CAP: usize = 64;
const SMALL_KINDS: [&str; 3] = ["selector", "custom_property", "element_id"];

#[derive(Debug, Clone)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Print {
    pub idx: usize,
    pub raw_hash: String,
    pub norm_hash: String,
    pub tokens: u32,
    pub minhash: Vec<u32>,
    pub winnow: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub norm: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LinkStats {
    pub units: usize,
    pub classes: usize,
    pub groups_t1: usize,
    pub groups_t2: usize,
    pub pairs_t1: usize,
    pub pairs_t2: usize,
    pub pairs_t3: usize,
    pub candidates: usize,
    pub millis: u64,
}

#[derive(Debug, Clone)]
pub struct DriftRow {
    pub state: String,
    pub a_path: String,
    pub a_qualified: String,
    pub a_start: i64,
    pub a_end: i64,
    pub b_path: String,
    pub b_qualified: String,
    pub b_start: i64,
    pub b_end: i64,
    pub score: f64,
}

fn sql(e: rusqlite::Error) -> String {
    format!("dedup: {e}")
}

#[derive(Clone, Copy, PartialEq)]
enum Family {
    Js,
    Py,
    Rs,
    Css,
}

fn family(language: &str) -> Family {
    match language {
        "py" | "python" | "pyi" => Family::Py,
        "rs" | "rust" => Family::Rs,
        "css" | "scss" | "less" => Family::Css,
        _ => Family::Js,
    }
}

const JS_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "break", "case", "catch", "class", "const", "continue",
    "debugger", "declare", "default", "delete", "do", "else", "enum", "export", "extends", "false",
    "finally", "for", "function", "if", "implements", "import", "in", "instanceof", "interface",
    "keyof", "let", "namespace", "new", "null", "of", "private", "protected", "public", "readonly",
    "return", "satisfies", "static", "super", "switch", "this", "throw", "true", "try", "type",
    "typeof", "undefined", "var", "void", "while", "with", "yield",
];

const PY_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

const RS_KEYWORDS: &[&str] = &[
    "Self", "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

const CSS_KEYWORDS: &[&str] = &[
    "and", "calc", "from", "import", "important", "keyframes", "media", "not", "only", "supports",
    "to", "var",
];

const REGEX_AFTER: &[&str] = &[
    "return", "typeof", "case", "in", "of", "delete", "void", "throw", "new", "else", "do",
];

const OPS3: &[&str] = &[
    "...", "===", "!==", "**=", "<<=", ">>=", "&&=", "||=", "??=", "..=",
];

const OPS2: &[&str] = &[
    "=>", "==", "!=", "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=",
    "%=", "&=", "|=", "^=", "<<", ">>", "**", "::", "->", "..",
];

fn keywords(f: Family) -> &'static [&'static str] {
    match f {
        Family::Js => JS_KEYWORDS,
        Family::Py => PY_KEYWORDS,
        Family::Rs => RS_KEYWORDS,
        Family::Css => CSS_KEYWORDS,
    }
}

fn ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80
}

fn ident_part(b: u8) -> bool {
    ident_start(b) || b.is_ascii_digit()
}

fn word_part(f: Family, b: u8) -> bool {
    ident_part(b) || (f == Family::Css && b == b'-')
}

fn skip_line(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block(b: &[u8], mut i: usize, nested: bool) -> usize {
    let mut depth = 0usize;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            depth += 1;
            i += 2;
            if !nested {
                depth = 1;
            }
            continue;
        }
        if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
            i += 2;
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return i;
            }
            continue;
        }
        i += 1;
    }
    b.len()
}

fn skip_quoted(b: &[u8], start: usize, triple: bool) -> usize {
    let q = b[start];
    let n = b.len();
    let mut i = start + if triple { 3 } else { 1 };
    if triple {
        while i < n {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == q && i + 2 < n && b[i + 1] == q && b[i + 2] == q {
                return i + 3;
            }
            i += 1;
        }
        return n;
    }
    while i < n && b[i] != q {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'\n' && q != b'`' {
            return i;
        }
        i += 1;
    }
    if i < n { i + 1 } else { n }
}

fn skip_raw(b: &[u8], from: usize) -> Option<usize> {
    let n = b.len();
    let mut j = from;
    while j < n && b[j] == b'#' {
        j += 1;
    }
    let hashes = j - from;
    if j >= n || b[j] != b'"' {
        return None;
    }
    j += 1;
    while j < n {
        if b[j] == b'"' {
            let mut k = 0;
            while k < hashes && j + 1 + k < n && b[j + 1 + k] == b'#' {
                k += 1;
            }
            if k == hashes {
                return Some(j + 1 + hashes);
            }
        }
        j += 1;
    }
    Some(n)
}

fn regex_allowed(prev: Option<&Token>) -> bool {
    match prev {
        None => true,
        Some(t) => {
            let first = t.norm.as_bytes()[0];
            if first == b'$' && t.norm.len() > 1 {
                false
            } else if first.is_ascii_alphabetic() {
                REGEX_AFTER.contains(&t.norm.as_str())
            } else {
                !matches!(t.norm.as_str(), ")" | "]" | "}")
            }
        }
    }
}

fn skip_regex(b: &[u8], start: usize) -> Option<usize> {
    let n = b.len();
    let mut j = start + 1;
    let mut in_class = false;
    while j < n {
        match b[j] {
            b'\n' => return None,
            b'\\' => j += 1,
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => {
                j += 1;
                while j < n && ident_part(b[j]) {
                    j += 1;
                }
                return Some(j);
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn push(out: &mut Vec<Token>, norm: &str, start: usize, end: usize) {
    out.push(Token { norm: norm.to_string(), start, end });
}

pub fn tokenize(language: &str, text: &str) -> Vec<Token> {
    let f = family(language);
    let b = text.as_bytes();
    let n = b.len();
    let kw = keywords(f);
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if f == Family::Py && c == b'#' {
            i = skip_line(b, i);
            continue;
        }
        if f != Family::Py && c == b'/' && i + 1 < n {
            if b[i + 1] == b'/' && f != Family::Css {
                i = skip_line(b, i);
                continue;
            }
            if b[i + 1] == b'*' {
                i = skip_block(b, i, f == Family::Rs);
                continue;
            }
        }
        if f == Family::Js && c == b'/' && regex_allowed(out.last()) {
            if let Some(end) = skip_regex(b, i) {
                push(&mut out, "$str", start, end);
                i = end;
                continue;
            }
        }
        if c == b'"' || (c == b'\'' && f != Family::Rs) || (c == b'`' && f == Family::Js) {
            let triple = f == Family::Py && i + 2 < n && b[i + 1] == c && b[i + 2] == c;
            i = skip_quoted(b, i, triple);
            push(&mut out, "$str", start, i);
            continue;
        }
        if c == b'\'' && f == Family::Rs {
            if i + 2 < n && b[i + 1] != b'\\' && b[i + 2] == b'\'' {
                i += 3;
                push(&mut out, "$str", start, i);
            } else if i + 1 < n && b[i + 1] == b'\\' {
                let mut j = i + 2;
                while j < n && b[j] != b'\'' && b[j] != b'\n' {
                    j += 1;
                }
                i = if j < n && b[j] == b'\'' { j + 1 } else { j };
                push(&mut out, "$str", start, i);
            } else if i + 1 < n && ident_start(b[i + 1]) {
                let mut j = i + 1;
                while j < n && ident_part(b[j]) {
                    j += 1;
                }
                if j < n && b[j] == b'\'' {
                    i = j + 1;
                    push(&mut out, "$str", start, i);
                } else {
                    i = j;
                    push(&mut out, "$id", start, i);
                }
            } else {
                i += 1;
                push(&mut out, "'", start, i);
            }
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && i + 1 < n && b[i + 1].is_ascii_digit()) {
            let mut j = i + 1;
            while j < n && (ident_part(b[j]) || (b[j] == b'.' && j + 1 < n && b[j + 1].is_ascii_digit())) {
                j += 1;
            }
            if f == Family::Css && j < n && b[j] == b'%' {
                j += 1;
            }
            i = j;
            push(&mut out, "$num", start, i);
            continue;
        }
        if f == Family::Css && c == b'#' && i + 1 < n && ident_part(b[i + 1]) {
            let mut j = i + 1;
            while j < n && word_part(f, b[j]) {
                j += 1;
            }
            let hex = matches!(j - i - 1, 3 | 4 | 6 | 8) && b[i + 1..j].iter().all(|x| x.is_ascii_hexdigit());
            i = j;
            push(&mut out, if hex { "$num" } else { "$id" }, start, i);
            continue;
        }
        let css_dash = f == Family::Css
            && c == b'-'
            && i + 1 < n
            && (ident_start(b[i + 1]) || b[i + 1] == b'-');
        if ident_start(c) || css_dash {
            let mut j = i + 1;
            while j < n && word_part(f, b[j]) {
                j += 1;
            }
            let word = String::from_utf8_lossy(&b[i..j]).into_owned();
            let next = if j < n { b[j] } else { 0 };
            if f == Family::Py
                && word.len() <= 2
                && word.bytes().all(|x| b"rRbBfFuU".contains(&x))
                && (next == b'"' || next == b'\'')
            {
                i = j;
                continue;
            }
            if f == Family::Rs {
                if matches!(word.as_str(), "b" | "c") && next == b'"' {
                    i = j;
                    continue;
                }
                if matches!(word.as_str(), "r" | "br" | "cr") && (next == b'"' || next == b'#') {
                    if let Some(end) = skip_raw(b, j) {
                        i = end;
                        push(&mut out, "$str", start, i);
                        continue;
                    }
                }
            }
            i = j;
            if kw.contains(&word.as_str()) {
                push(&mut out, &word, start, i);
            } else {
                push(&mut out, "$id", start, i);
            }
            continue;
        }
        let rest = &b[i..];
        if let Some(op) = OPS3.iter().find(|op| rest.starts_with(op.as_bytes())) {
            i += 3;
            push(&mut out, op, start, i);
            continue;
        }
        if let Some(op) = OPS2.iter().find(|op| rest.starts_with(op.as_bytes())) {
            i += 2;
            push(&mut out, op, start, i);
            continue;
        }
        i += 1;
        let one = String::from_utf8_lossy(&b[start..i]).into_owned();
        push(&mut out, &one, start, i);
    }
    out
}

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn token_hash(norm: &str) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in norm.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    mix(h)
}

fn shingle_hashes(tokens: &[Token]) -> Vec<u64> {
    let hashes: Vec<u64> = tokens.iter().map(|t| token_hash(&t.norm)).collect();
    if hashes.len() < SHINGLE {
        return Vec::new();
    }
    hashes
        .windows(SHINGLE)
        .map(|w| w.iter().fold(SEED, |h, &t| mix(h.rotate_left(7) ^ t)))
        .collect()
}

fn permutations() -> &'static Vec<(u64, u64)> {
    static PERMS: OnceLock<Vec<(u64, u64)>> = OnceLock::new();
    PERMS.get_or_init(|| {
        (0..PERMUTATIONS as u64)
            .map(|i| {
                let a = mix(SEED.wrapping_add((2 * i + 1).wrapping_mul(GOLDEN))) | 1;
                let b = mix(SEED.wrapping_add((2 * i + 2).wrapping_mul(GOLDEN)));
                (a, b)
            })
            .collect()
    })
}

fn minhash(shingles: &[u64]) -> Vec<u32> {
    let perms = permutations();
    let mut mins = vec![u32::MAX; PERMUTATIONS];
    for &x in shingles {
        for (slot, &(a, b)) in mins.iter_mut().zip(perms.iter()) {
            let v = (a.wrapping_mul(x).wrapping_add(b) >> 32) as u32;
            if v < *slot {
                *slot = v;
            }
        }
    }
    mins
}

fn winnow(shingles: &[u64]) -> Vec<u64> {
    if shingles.is_empty() {
        return Vec::new();
    }
    let mut picked: Vec<u64> = Vec::new();
    if shingles.len() <= WINDOW {
        picked.push(*shingles.iter().min().unwrap_or(&0));
    } else {
        for window in shingles.windows(WINDOW) {
            let mut best = window[0];
            for &h in window {
                if h <= best {
                    best = h;
                }
            }
            picked.push(best);
        }
    }
    picked.sort_unstable();
    picked.dedup();
    picked
}

fn normalise_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        out.push_str(line.trim_end_matches([' ', '\t', '\r']));
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn fingerprint(language: &str, text: &str, spans: &[Span]) -> Vec<Print> {
    let bytes = text.as_bytes();
    spans
        .iter()
        .enumerate()
        .map(|(idx, span)| {
            let end = span.end_byte.min(bytes.len());
            let start = span.start_byte.min(end);
            let slice = String::from_utf8_lossy(&bytes[start..end]);
            let raw_hash = hex(normalise_lines(&slice).as_bytes());
            let tokens = tokenize(language, &slice);
            let mut hasher = blake3::Hasher::new();
            for t in &tokens {
                hasher.update(t.norm.as_bytes());
                hasher.update(&[0x1f]);
            }
            let norm_hash = hasher.finalize().to_hex().to_string();
            let count = tokens.len() as u32;
            let (mh, wn) = if count >= MIN_TOKENS {
                let shingles = shingle_hashes(&tokens);
                (minhash(&shingles), winnow(&shingles))
            } else {
                (Vec::new(), Vec::new())
            };
            Print { idx, raw_hash, norm_hash, tokens: count, minhash: mh, winnow: wn }
        })
        .collect()
}

fn encode_u32(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn encode_u64(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn decode_u32(blob: &[u8]) -> Vec<u32> {
    blob.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn decode_u64(blob: &[u8]) -> Vec<u64> {
    blob.chunks_exact(8)
        .map(|c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
        .collect()
}

pub fn store(conn: &Connection, object_format: &str, oid: &str, prints: &[Print]) -> crate::Result<()> {
    conn.execute_batch("SAVEPOINT dedup_store").map_err(sql)?;
    let written = write_prints(conn, object_format, oid, prints);
    match written {
        Ok(()) => conn.execute_batch("RELEASE dedup_store").map_err(sql),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO dedup_store; RELEASE dedup_store");
            Err(e)
        }
    }
}

fn write_prints(conn: &Connection, object_format: &str, oid: &str, prints: &[Print]) -> crate::Result<()> {
    conn.execute(
        "DELETE FROM unit_print WHERE object_format = ?1 AND oid = ?2",
        params![object_format, oid],
    )
    .map_err(sql)?;
    {
        let mut stmt = conn
            .prepare_cached(
                "INSERT INTO unit_print (object_format, oid, idx, raw_hash, norm_hash, tokens, minhash, winnow, scheme)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .map_err(sql)?;
        for p in prints {
            stmt.execute(params![
                object_format,
                oid,
                p.idx as i64,
                p.raw_hash,
                p.norm_hash,
                p.tokens as i64,
                encode_u32(&p.minhash),
                encode_u64(&p.winnow),
                SCHEME as i64
            ])
            .map_err(sql)?;
        }
    }
    Ok(())
}

pub fn jaccard(a: &[u32], b: &[u32]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let same = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    same as f64 / a.len() as f64
}

fn shared(a: &[u64], b: &[u64]) -> usize {
    let (mut i, mut j, mut count) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            count += 1;
            i += 1;
            j += 1;
        } else if a[i] < b[j] {
            i += 1;
        } else {
            j += 1;
        }
    }
    count
}

#[derive(Debug, Clone)]
struct Occ {
    corpus: i64,
    path: String,
    format: String,
    oid: String,
    idx: i64,
    kind: String,
    qualified: String,
    start: i64,
    end: i64,
    raw: String,
    norm: String,
    tokens: i64,
}

fn load_occs(conn: &Connection, pair: Option<(i64, i64)>) -> crate::Result<Vec<Occ>> {
    let mut query = String::from(
        "SELECT f.corpus_id, f.path, c.object_format, f.oid, u.idx,
                COALESCE(u.kind, ''), COALESCE(NULLIF(u.qualified, ''), u.name, ''),
                COALESCE(u.start_line, 0), COALESCE(u.end_line, 0),
                p.raw_hash, p.norm_hash, p.tokens
         FROM file f
         JOIN corpus c ON c.id = f.corpus_id
         JOIN unit u ON u.object_format = c.object_format AND u.oid = f.oid
         JOIN unit_print p ON p.object_format = u.object_format AND p.oid = u.oid AND p.idx = u.idx
         WHERE f.role = 'source'",
    );
    if let Some((a, b)) = pair {
        query.push_str(&format!(" AND f.corpus_id IN ({a}, {b})"));
    }
    query.push_str(" ORDER BY f.corpus_id, f.path, u.idx");
    let mut stmt = conn.prepare(&query).map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Occ {
                corpus: r.get(0)?,
                path: r.get(1)?,
                format: r.get(2)?,
                oid: r.get(3)?,
                idx: r.get(4)?,
                kind: r.get(5)?,
                qualified: r.get(6)?,
                start: r.get(7)?,
                end: r.get(8)?,
                raw: r.get(9)?,
                norm: r.get(10)?,
                tokens: r.get(11)?,
            })
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

fn load_signature(conn: &Connection, occ: &Occ) -> crate::Result<(Vec<u32>, Vec<u64>)> {
    let mut stmt = conn
        .prepare_cached("SELECT minhash, winnow FROM unit_print WHERE object_format = ?1 AND oid = ?2 AND idx = ?3")
        .map_err(sql)?;
    let (m, w): (Vec<u8>, Vec<u8>) = stmt
        .query_row(params![occ.format, occ.oid, occ.idx], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?;
    Ok((decode_u32(&m), decode_u64(&w)))
}

fn same_place(a: &Occ, b: &Occ) -> bool {
    a.corpus == b.corpus && a.path == b.path && a.start <= b.end && b.start <= a.end
}

fn insert_pair(
    stmt: &mut rusqlite::CachedStatement<'_>,
    tier: &str,
    score: f64,
    a: &Occ,
    b: &Occ,
) -> crate::Result<()> {
    stmt.execute(params![
        tier, score, a.corpus, a.path, a.qualified, a.start, a.end, b.corpus, b.path, b.qualified,
        b.start, b.end
    ])
    .map_err(sql)?;
    Ok(())
}

fn group_edges(members: &[usize], occs: &[Occ]) -> Vec<(usize, usize)> {
    let mut first_of: Vec<(i64, usize)> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for &m in members {
        match first_of.iter().find(|(c, _)| *c == occs[m].corpus) {
            Some(&(_, head)) => edges.push((head, m)),
            None => first_of.push((occs[m].corpus, m)),
        }
    }
    for (x, &(_, a)) in first_of.iter().enumerate() {
        for &(_, b) in &first_of[x + 1..] {
            edges.push((a, b));
        }
    }
    edges
}

fn band_key(rows: &[u32]) -> u64 {
    rows.iter().fold(SEED, |h, &v| mix(h.rotate_left(11) ^ v as u64))
}

pub fn link(conn: &Connection) -> crate::Result<LinkStats> {
    conn.execute_batch("SAVEPOINT dedup_link").map_err(sql)?;
    match link_all(conn) {
        Ok(stats) => {
            conn.execute_batch("RELEASE dedup_link").map_err(sql)?;
            Ok(stats)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO dedup_link; RELEASE dedup_link");
            Err(e)
        }
    }
}

fn link_all(conn: &Connection) -> crate::Result<LinkStats> {
    let begin = Instant::now();
    let mut stats = LinkStats::default();
    let occs: Vec<Occ> = load_occs(conn, None)?
        .into_iter()
        .filter(|o| o.tokens >= MIN_GROUP_TOKENS)
        .collect();
    stats.units = occs.len();
    let tx = conn;
    tx.execute_batch("DELETE FROM clone_member; DELETE FROM clone_group; DELETE FROM clone_pair;")
        .map_err(sql)?;
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    {
        let mut group_stmt = tx
            .prepare_cached("INSERT INTO clone_group (tier, hash, members) VALUES (?1, ?2, ?3)")
            .map_err(sql)?;
        let mut member_stmt = tx
            .prepare_cached(
                "INSERT INTO clone_member (group_id, corpus_id, path, qualified, start_line, end_line)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(sql)?;
        let mut pair_stmt = tx
            .prepare_cached(
                "INSERT INTO clone_pair (tier, score, a_corpus, a_path, a_qualified, a_start, a_end,
                                         b_corpus, b_path, b_qualified, b_start, b_end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )
            .map_err(sql)?;

        let mut by_raw: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut by_norm: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, o) in occs.iter().enumerate() {
            by_raw.entry(o.raw.as_str()).or_default().push(i);
            by_norm.entry(o.norm.as_str()).or_default().push(i);
        }
        stats.classes = by_norm.len();

        let mut raw_groups: Vec<(&str, Vec<usize>)> =
            by_raw.into_iter().filter(|(_, v)| v.len() >= 2).collect();
        raw_groups.sort_by(|a, b| a.0.cmp(b.0));
        for (hash, members) in &raw_groups {
            group_stmt
                .execute(params!["T1", hash, members.len() as i64])
                .map_err(sql)?;
            let gid = tx.last_insert_rowid();
            for &m in members {
                let o = &occs[m];
                member_stmt
                    .execute(params![gid, o.corpus, o.path, o.qualified, o.start, o.end])
                    .map_err(sql)?;
            }
            stats.groups_t1 += 1;
            for (a, b) in group_edges(members, &occs) {
                if same_place(&occs[a], &occs[b]) {
                    continue;
                }
                insert_pair(&mut pair_stmt, "T1", 1.0, &occs[a], &occs[b])?;
                seen.insert((a.min(b), a.max(b)));
                stats.pairs_t1 += 1;
            }
        }

        let mut norm_groups: Vec<(&str, Vec<usize>)> = by_norm
            .iter()
            .filter(|(_, v)| v.len() >= 2)
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        norm_groups.sort_by(|a, b| a.0.cmp(b.0));
        for (hash, members) in &norm_groups {
            let mut reps: Vec<usize> = Vec::new();
            let mut raws: HashSet<&str> = HashSet::new();
            for &m in members {
                if raws.insert(occs[m].raw.as_str()) {
                    reps.push(m);
                }
            }
            if reps.len() < 2 {
                continue;
            }
            group_stmt
                .execute(params!["T2", hash, members.len() as i64])
                .map_err(sql)?;
            let gid = tx.last_insert_rowid();
            for &m in members {
                let o = &occs[m];
                member_stmt
                    .execute(params![gid, o.corpus, o.path, o.qualified, o.start, o.end])
                    .map_err(sql)?;
            }
            stats.groups_t2 += 1;
            for (a, b) in group_edges(&reps, &occs) {
                if same_place(&occs[a], &occs[b]) {
                    continue;
                }
                insert_pair(&mut pair_stmt, "T2", 1.0, &occs[a], &occs[b])?;
                seen.insert((a.min(b), a.max(b)));
                stats.pairs_t2 += 1;
            }
        }

        let mut classes: Vec<(&str, Vec<usize>)> = Vec::new();
        for (norm, members) in by_norm.iter() {
            let eligible: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&m| {
                    occs[m].tokens as u32 >= MIN_TOKENS && !SMALL_KINDS.contains(&occs[m].kind.as_str())
                })
                .collect();
            if !eligible.is_empty() {
                classes.push((*norm, eligible));
            }
        }
        classes.sort_by(|a, b| a.0.cmp(b.0));
        let mut sigs: Vec<(Vec<u32>, Vec<u64>)> = Vec::with_capacity(classes.len());
        for (_, members) in &classes {
            sigs.push(load_signature(&tx, &occs[members[0]])?);
        }
        let mut buckets: HashMap<(usize, u64), Vec<u32>> = HashMap::new();
        for (ci, (mh, _)) in sigs.iter().enumerate() {
            if mh.len() != PERMUTATIONS {
                continue;
            }
            for band in 0..BANDS {
                let key = band_key(&mh[band * ROWS..(band + 1) * ROWS]);
                buckets.entry((band, key)).or_default().push(ci as u32);
            }
        }
        let mut candidates: HashSet<(u32, u32)> = HashSet::new();
        for members in buckets.values() {
            if members.len() < 2 || members.len() > BUCKET_CAP {
                continue;
            }
            for (x, &a) in members.iter().enumerate() {
                for &b in &members[x + 1..] {
                    candidates.insert((a.min(b), a.max(b)));
                }
            }
        }
        stats.candidates = candidates.len();
        let mut ordered: Vec<(u32, u32)> = candidates.into_iter().collect();
        ordered.sort_unstable();
        for (ca, cb) in ordered {
            let (ma, wa) = &sigs[ca as usize];
            let (mb, wb) = &sigs[cb as usize];
            let score = jaccard(ma, mb);
            if score < JACCARD_MIN {
                continue;
            }
            let common = shared(wa, wb);
            let smaller = wa.len().min(wb.len());
            if common == 0 || common * 4 < smaller {
                continue;
            }
            let mut emitted = 0usize;
            'outer: for &a in &classes[ca as usize].1 {
                for &b in &classes[cb as usize].1 {
                    if same_place(&occs[a], &occs[b]) {
                        continue;
                    }
                    let (lo, hi) = (a.min(b), a.max(b));
                    if !seen.insert((lo, hi)) {
                        continue;
                    }
                    insert_pair(&mut pair_stmt, "T3", score, &occs[lo], &occs[hi])?;
                    stats.pairs_t3 += 1;
                    emitted += 1;
                    if emitted >= EXPAND_CAP {
                        break 'outer;
                    }
                }
            }
        }
    }
    stats.millis = begin.elapsed().as_millis() as u64;
    Ok(stats)
}

fn state_rank(state: &str) -> u8 {
    match state {
        "identical" => 0,
        "renamed" => 1,
        "drifted" => 2,
        _ => 3,
    }
}

pub fn drift(conn: &Connection, a: i64, b: i64) -> crate::Result<Vec<DriftRow>> {
    let occs = load_occs(conn, Some((a, b)))?;
    let side_a: Vec<&Occ> = occs.iter().filter(|o| o.corpus == a).collect();
    let side_b: Vec<&Occ> = occs.iter().filter(|o| o.corpus == b).collect();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_path: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, o) in side_b.iter().enumerate() {
        if !o.qualified.is_empty() {
            by_name.entry(o.qualified.as_str()).or_default().push(i);
        }
        by_path.entry(o.path.as_str()).or_default().push(i);
    }
    let mut cache: HashMap<(String, i64), Vec<u32>> = HashMap::new();
    let mut signature = |o: &Occ| -> crate::Result<Vec<u32>> {
        if o.tokens < MIN_TOKENS as i64 {
            return Ok(Vec::new());
        }
        let key = (o.oid.clone(), o.idx);
        if let Some(v) = cache.get(&key) {
            return Ok(v.clone());
        }
        let (mh, _) = load_signature(conn, o)?;
        cache.insert(key, mh.clone());
        Ok(mh)
    };
    let mut rows: Vec<DriftRow> = Vec::new();
    for ua in &side_a {
        let named: Vec<usize> = by_name.get(ua.qualified.as_str()).cloned().unwrap_or_default();
        let by_name_hit = !ua.qualified.is_empty() && !named.is_empty();
        let candidates: Vec<usize> = if by_name_hit {
            named
        } else {
            by_path.get(ua.path.as_str()).cloned().unwrap_or_default()
        };
        let mut best: Option<(u8, f64, bool, usize, &'static str)> = None;
        for ci in candidates {
            let ub = side_b[ci];
            let (state, score): (&'static str, f64) = if ua.raw == ub.raw {
                ("identical", 1.0)
            } else if ua.norm == ub.norm {
                ("renamed", 1.0)
            } else {
                let sa = signature(ua)?;
                let sb = signature(ub)?;
                let j = jaccard(&sa, &sb);
                if j >= JACCARD_MIN { ("drifted", j) } else { ("diverged", j) }
            };
            let rank = state_rank(state);
            let same_path = ua.path == ub.path;
            let better = match &best {
                None => true,
                Some((r, s, p, _, _)) => {
                    rank < *r || (rank == *r && (score > *s || (score == *s && same_path && !*p)))
                }
            };
            if better {
                best = Some((rank, score, same_path, ci, state));
            }
        }
        if let Some((_, score, _, ci, state)) = best {
            if !by_name_hit && state == "diverged" {
                continue;
            }
            let ub = side_b[ci];
            rows.push(DriftRow {
                state: state.to_string(),
                a_path: ua.path.clone(),
                a_qualified: ua.qualified.clone(),
                a_start: ua.start,
                a_end: ua.end,
                b_path: ub.path.clone(),
                b_qualified: ub.qualified.clone(),
                b_start: ub.start,
                b_end: ub.end,
                score,
            });
        }
    }
    rows.sort_by(|x, y| {
        state_rank(&x.state)
            .cmp(&state_rank(&y.state))
            .then(x.a_path.cmp(&y.a_path))
            .then(x.a_start.cmp(&y.a_start))
    });
    Ok(rows)
}
