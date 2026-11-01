use std::collections::{HashMap, HashSet};
use std::time::Instant;

use rusqlite::{Connection, params};

use crate::dedup::{JACCARD_MIN, MIN_GROUP_TOKENS, PERMUTATIONS, jaccard};

pub const RELATED_MIN: f64 = 0.40;
pub const BANDS: usize = 42;
pub const ROWS: usize = 3;
pub const NAME_GROUP_CAP: usize = 50;
pub const BUILT_KEY: &str = "related_built";

const BUCKET_CAP: usize = 200;
const EXPAND_CAP: usize = 64;
const SMALL_KINDS: [&str; 3] = ["selector", "custom_property", "element_id"];
const SPELLING: [(&str, &str); 18] = [
    ("colour", "color"),
    ("behaviour", "behavior"),
    ("centre", "center"),
    ("grey", "gray"),
    ("favourite", "favorite"),
    ("neighbour", "neighbor"),
    ("flavour", "flavor"),
    ("honour", "honor"),
    ("labour", "labor"),
    ("licence", "license"),
    ("analyse", "analyze"),
    ("catalogue", "catalog"),
    ("initialis", "initializ"),
    ("normalis", "normaliz"),
    ("serialis", "serializ"),
    ("optimis", "optimiz"),
    ("organis", "organiz"),
    ("visualis", "visualiz"),
];

#[derive(Debug, Clone, Default)]
pub struct RelatedStats {
    pub units: usize,
    pub candidates: usize,
    pub name_groups: usize,
    pub names_skipped: usize,
    pub same_name: usize,
    pub similar: usize,
    pub millis: u64,
}

struct Unit {
    corpus: i64,
    path: String,
    qualified: String,
    family: &'static str,
    name: String,
    start: i64,
    end: i64,
    norm: String,
    minhash: Vec<u32>,
}

fn sql(e: rusqlite::Error) -> String {
    format!("related: {e}")
}

pub fn kind_family(kind: &str) -> &str {
    match kind {
        "function" | "method" | "const" => "callable",
        other => other,
    }
}

pub fn normalise_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect();
    for (from, to) in SPELLING {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

pub fn last_segment(qualified: &str) -> &str {
    qualified.rsplit(['.', ':', '#']).next().unwrap_or(qualified)
}

pub fn built(conn: &Connection) -> crate::Result<bool> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM meta WHERE key = ?1", params![BUILT_KEY], |r| r.get(0))
        .map_err(sql)?;
    Ok(count > 0)
}

fn decode(blob: &[u8]) -> Vec<u32> {
    blob.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn band_key(rows: &[u32], family: &str) -> u64 {
    let seed = family
        .bytes()
        .fold(0x6A09_E667_F3BC_C908u64, |h, b| mix(h.rotate_left(7) ^ b as u64));
    rows.iter().fold(seed, |h, &v| mix(h.rotate_left(11) ^ v as u64))
}

fn load_units(conn: &Connection) -> crate::Result<Vec<Unit>> {
    let mut stmt = conn
        .prepare(
            "SELECT f.corpus_id, f.path, COALESCE(u.kind, ''),
                    COALESCE(NULLIF(u.qualified, ''), u.name, ''), COALESCE(u.name, ''),
                    COALESCE(u.start_line, 0), COALESCE(u.end_line, 0), p.norm_hash, p.minhash
             FROM file f
             JOIN corpus c ON c.id = f.corpus_id
             JOIN unit u ON u.object_format = c.object_format AND u.oid = f.oid
             JOIN unit_print p ON p.object_format = u.object_format AND p.oid = u.oid AND p.idx = u.idx
             WHERE f.role = 'source' AND p.tokens >= ?1
             ORDER BY f.corpus_id, f.path, u.idx",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![MIN_GROUP_TOKENS], |r| {
            let kind: String = r.get(2)?;
            let qualified: String = r.get(3)?;
            let name: String = r.get(4)?;
            let blob: Vec<u8> = r.get(8)?;
            Ok((kind, qualified, name, r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(5)?, r.get::<_, i64>(6)?, r.get::<_, String>(7)?, blob))
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, qualified, name, corpus, path, start, end, norm, blob) = row.map_err(sql)?;
        if SMALL_KINDS.contains(&kind.as_str()) {
            continue;
        }
        let minhash = decode(&blob);
        if minhash.len() != PERMUTATIONS {
            continue;
        }
        let shown = if name.is_empty() { last_segment(&qualified).to_string() } else { name };
        let family: &'static str = match kind_family(&kind) {
            "callable" => "callable",
            "constant" => "constant",
            "class" => "class",
            "interface" => "interface",
            "type" => "type",
            "enum" => "enum",
            "struct" => "struct",
            "trait" => "trait",
            "tauri_command" => "tauri_command",
            "script" => "script",
            "style" => "style",
            _ => "other",
        };
        out.push(Unit {
            corpus,
            path,
            qualified,
            family,
            name: normalise_name(&shown),
            start,
            end,
            norm,
            minhash,
        });
    }
    Ok(out)
}

fn same_file(a: &Unit, b: &Unit) -> bool {
    a.corpus == b.corpus && a.path == b.path
}

fn same_place(a: &Unit, b: &Unit) -> bool {
    same_file(a, b) && a.start <= b.end && b.start <= a.end
}

fn clone_pairs(conn: &Connection, units: &[Unit]) -> crate::Result<HashSet<(usize, usize)>> {
    let mut index: HashMap<(i64, &str, &str, i64), usize> = HashMap::with_capacity(units.len());
    for (i, u) in units.iter().enumerate() {
        index.insert((u.corpus, u.path.as_str(), u.qualified.as_str(), u.start), i);
    }
    let mut stmt = conn
        .prepare(
            "SELECT a_corpus, a_path, a_qualified, a_start, b_corpus, b_path, b_qualified, b_start FROM clone_pair",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })
        .map_err(sql)?;
    let mut out = HashSet::new();
    for row in rows {
        let (ac, ap, aq, a_start, bc, bp, bq, b_start) = row.map_err(sql)?;
        let a = index.get(&(ac, ap.as_str(), aq.as_str(), a_start));
        let b = index.get(&(bc, bp.as_str(), bq.as_str(), b_start));
        if let (Some(&a), Some(&b)) = (a, b) {
            out.insert((a.min(b), a.max(b)));
        }
    }
    Ok(out)
}

fn insert(
    stmt: &mut rusqlite::CachedStatement<'_>,
    reason: &str,
    score: f64,
    a: &Unit,
    b: &Unit,
) -> crate::Result<()> {
    stmt.execute(params![
        reason, score, a.corpus, a.path, a.qualified, a.start, a.end, b.corpus, b.path, b.qualified, b.start,
        b.end
    ])
    .map_err(sql)?;
    Ok(())
}

pub fn link(conn: &Connection) -> crate::Result<RelatedStats> {
    conn.execute_batch("SAVEPOINT related_link").map_err(sql)?;
    match link_all(conn) {
        Ok(stats) => match conn.execute_batch("RELEASE related_link") {
            Ok(()) => Ok(stats),
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK TO related_link; RELEASE related_link");
                Err(sql(e))
            }
        },
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO related_link; RELEASE related_link");
            Err(e)
        }
    }
}

fn link_all(conn: &Connection) -> crate::Result<RelatedStats> {
    let begin = Instant::now();
    let mut stats = RelatedStats::default();
    conn.execute_batch("DELETE FROM related_pair").map_err(sql)?;
    let units = load_units(conn)?;
    stats.units = units.len();
    let mut seen = clone_pairs(conn, &units)?;
    let mut stmt = conn
        .prepare_cached(
            "INSERT INTO related_pair (reason, score, a_corpus, a_path, a_qualified, a_start, a_end,
                                      b_corpus, b_path, b_qualified, b_start, b_end)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .map_err(sql)?;

    let mut by_name: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, u) in units.iter().enumerate() {
        if !u.name.is_empty() {
            by_name.entry((u.family, u.name.as_str())).or_default().push(i);
        }
    }
    let mut names: Vec<(&(&str, &str), &Vec<usize>)> = by_name.iter().filter(|(_, v)| v.len() >= 2).collect();
    names.sort_by(|a, b| a.0.cmp(b.0));
    for (_, members) in names {
        if members.len() > NAME_GROUP_CAP {
            stats.names_skipped += 1;
            continue;
        }
        let mut distinct_files = false;
        for &m in &members[1..] {
            if !same_file(&units[members[0]], &units[m]) {
                distinct_files = true;
                break;
            }
        }
        if !distinct_files {
            continue;
        }
        stats.name_groups += 1;
        for (x, &a) in members.iter().enumerate() {
            for &b in &members[x + 1..] {
                if same_file(&units[a], &units[b]) || units[a].norm == units[b].norm || !seen.insert((a, b)) {
                    continue;
                }
                let score = jaccard(&units[a].minhash, &units[b].minhash);
                insert(&mut stmt, "same_name", score, &units[a], &units[b])?;
                stats.same_name += 1;
            }
        }
    }

    let mut by_class: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, u) in units.iter().enumerate() {
        by_class.entry((u.family, u.norm.as_str())).or_default().push(i);
    }
    let mut classes: Vec<((&str, &str), Vec<usize>)> = by_class.into_iter().collect();
    classes.sort_by(|a, b| a.0.cmp(&b.0));
    let mut buckets: HashMap<(usize, u64), Vec<u32>> = HashMap::new();
    for (ci, ((family, _), members)) in classes.iter().enumerate() {
        let mh = &units[members[0]].minhash;
        for band in 0..BANDS {
            let key = band_key(&mh[band * ROWS..(band + 1) * ROWS], family);
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
        let ma = &units[classes[ca as usize].1[0]].minhash;
        let mb = &units[classes[cb as usize].1[0]].minhash;
        let score = jaccard(ma, mb);
        if !(RELATED_MIN..JACCARD_MIN).contains(&score) {
            continue;
        }
        let mut emitted = 0usize;
        'outer: for &a in &classes[ca as usize].1 {
            for &b in &classes[cb as usize].1 {
                if same_place(&units[a], &units[b]) {
                    continue;
                }
                let (lo, hi) = (a.min(b), a.max(b));
                if !seen.insert((lo, hi)) {
                    continue;
                }
                insert(&mut stmt, "similar", score, &units[lo], &units[hi])?;
                stats.similar += 1;
                emitted += 1;
                if emitted >= EXPAND_CAP {
                    break 'outer;
                }
            }
        }
    }
    drop(stmt);
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, '1') ON CONFLICT(key) DO UPDATE SET value = '1'",
        params![BUILT_KEY],
    )
    .map_err(sql)?;
    stats.millis = begin.elapsed().as_millis() as u64;
    Ok(stats)
}
