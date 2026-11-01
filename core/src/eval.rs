use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::git::Repo;
use crate::search::pack::{self, Pack};
use crate::search::{self, Target, text, tokens};
use crate::store::Store;

pub const KINDS: [&str; 4] = ["find", "reuse", "convention", "decline"];
pub const PACK_BUDGET: usize = 4000;
pub const BASELINE_READ: usize = 4;
pub const GATE_TOKEN_DROP: f64 = 0.30;
pub const GATE_RECALL: f64 = 0.80;
const RANK_LIMIT: usize = 100;
const NDCG_AT: usize = 10;
const BASELINE_FILE_CAP: usize = 40;
const BLOB_CAP: u64 = 1_000_000;

#[allow(unexpected_cfgs)]
pub const DENSE_FEATURE: bool = cfg!(feature = "dense");

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Expect {
    #[serde(default)]
    pub slots: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub memory: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Gold {
    pub id: String,
    pub corpus: String,
    pub task: String,
    pub kind: String,
    pub expect: Expect,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    Formslot,
    Baseline,
    Dense,
}

impl Arm {
    pub fn name(self) -> &'static str {
        match self {
            Arm::Formslot => "formslot",
            Arm::Baseline => "baseline",
            Arm::Dense => "dense",
        }
    }

    pub fn parse(text: &str) -> Option<Vec<Arm>> {
        match text {
            "formslot" => Some(vec![Arm::Formslot]),
            "baseline" => Some(vec![Arm::Baseline]),
            "dense" => Some(vec![Arm::Dense]),
            "all" => Some(vec![Arm::Formslot, Arm::Baseline, Arm::Dense]),
            _ => None,
        }
    }
}

enum Entry {
    File(String),
    Symbol(String),
    Slot(String),
    Memory(String),
}

fn entries_of(expect: &Expect) -> Vec<Entry> {
    let mut out = Vec::new();
    out.extend(expect.files.iter().map(|v| Entry::File(v.clone())));
    out.extend(expect.symbols.iter().map(|v| Entry::Symbol(v.clone())));
    out.extend(expect.slots.iter().map(|v| Entry::Slot(v.clone())));
    out.extend(expect.memory.iter().map(|v| Entry::Memory(v.to_lowercase())));
    out
}

struct Cand {
    kind: String,
    id: String,
    path: String,
    qualified: String,
}

struct Memo {
    cache: HashMap<i64, String>,
}

impl Memo {
    fn text(&mut self, conn: &Connection, id: &str) -> String {
        let Ok(n) = id.parse::<i64>() else {
            return String::new();
        };
        self.cache
            .entry(n)
            .or_insert_with(|| {
                conn.query_row("SELECT title, text FROM memory_record WHERE id = ?1", params![n], |r| {
                    let title: String = r.get(0)?;
                    let body: String = r.get(1)?;
                    Ok(format!("{title}\n{body}").to_lowercase())
                })
                .unwrap_or_default()
            })
            .clone()
    }
}

fn matches(entry: &Entry, cand: &Cand, conn: &Connection, memo: &mut Memo) -> bool {
    match entry {
        Entry::File(path) => matches!(cand.kind.as_str(), "unit" | "slot" | "file") && cand.path == *path,
        Entry::Symbol(sym) => match cand.kind.as_str() {
            "unit" => sym.split_once('#').is_some_and(|(p, q)| p == cand.path && q == cand.qualified),
            "file" => sym.split_once('#').map(|(p, _)| p).unwrap_or(sym) == cand.path,
            _ => false,
        },
        Entry::Slot(id) => cand.kind == "slot" && cand.id == *id,
        Entry::Memory(phrase) => cand.kind == "memory" && memo.text(conn, &cand.id).contains(phrase.as_str()),
    }
}

fn walk(cands: &[Cand], entries: &[Entry], conn: &Connection, memo: &mut Memo) -> (Option<usize>, Vec<usize>) {
    let mut done = vec![false; entries.len()];
    let mut first = None;
    let mut gains = Vec::new();
    for (i, cand) in cands.iter().enumerate().take(RANK_LIMIT) {
        let mut gained = false;
        for (j, entry) in entries.iter().enumerate() {
            if !done[j] && matches(entry, cand, conn, memo) {
                done[j] = true;
                gained = true;
            }
        }
        if gained {
            if first.is_none() {
                first = Some(i + 1);
            }
            gains.push(i + 1);
        }
    }
    (first, gains)
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub id: String,
    pub kind: String,
    pub first: Option<usize>,
    pub gains: Vec<usize>,
    pub wanted: usize,
    pub budget_found: Option<bool>,
    pub budget_found_p0: Option<bool>,
    pub tokens: usize,
    pub millis: f64,
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub n: usize,
    pub recall: Vec<f64>,
    pub mrr: f64,
    pub ndcg: f64,
    pub budget: Option<f64>,
    pub budget_p0: Option<f64>,
    pub tokens_mean: f64,
    pub tokens_median: f64,
    pub millis_median: f64,
}

pub struct ArmReport {
    pub arm: Arm,
    pub not_run: Option<String>,
    pub method: String,
    pub outcomes: Vec<Outcome>,
    pub overall: Option<Summary>,
    pub by_kind: Vec<(String, Summary)>,
}

pub struct Report {
    pub gold_path: String,
    pub total: usize,
    pub kind_counts: Vec<(String, usize)>,
    pub corpora: Vec<(String, String)>,
    pub ks: Vec<usize>,
    pub arms: Vec<ArmReport>,
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    }
}

fn discount(rank: usize) -> f64 {
    1.0 / ((rank + 1) as f64).log2()
}

fn ndcg(o: &Outcome) -> f64 {
    let dcg: f64 = o.gains.iter().filter(|p| **p <= NDCG_AT).map(|p| discount(*p)).sum();
    let ideal: f64 = (1..=o.wanted.min(NDCG_AT)).map(discount).sum();
    if ideal == 0.0 { 0.0 } else { (dcg / ideal).max(0.0) }
}

pub fn summarise(outcomes: &[&Outcome], ks: &[usize]) -> Summary {
    let n = outcomes.len();
    let share = |count: usize| if n == 0 { 0.0 } else { count as f64 / n as f64 };
    let recall = ks
        .iter()
        .map(|k| share(outcomes.iter().filter(|o| o.first.is_some_and(|f| f <= *k)).count()))
        .collect();
    let mrr = if n == 0 {
        0.0
    } else {
        outcomes.iter().map(|o| o.first.map(|f| 1.0 / f as f64).unwrap_or(0.0)).sum::<f64>() / n as f64
    };
    let ndcg_mean = if n == 0 { 0.0 } else { outcomes.iter().map(|o| ndcg(o)).sum::<f64>() / n as f64 };
    let budget = if outcomes.iter().all(|o| o.budget_found.is_some()) && n > 0 {
        Some(share(outcomes.iter().filter(|o| o.budget_found == Some(true)).count()))
    } else {
        None
    };
    let budget_p0 = if outcomes.iter().all(|o| o.budget_found_p0.is_some()) && n > 0 {
        Some(share(outcomes.iter().filter(|o| o.budget_found_p0 == Some(true)).count()))
    } else {
        None
    };
    let mut tok: Vec<f64> = outcomes.iter().map(|o| o.tokens as f64).collect();
    let mut ms: Vec<f64> = outcomes.iter().map(|o| o.millis).collect();
    Summary {
        n,
        recall,
        mrr,
        ndcg: ndcg_mean,
        budget,
        budget_p0,
        tokens_mean: if n == 0 { 0.0 } else { tok.iter().sum::<f64>() / n as f64 },
        tokens_median: median(&mut tok),
        millis_median: median(&mut ms),
    }
}

fn finish(arm: Arm, method: String, outcomes: Vec<Outcome>, ks: &[usize]) -> ArmReport {
    let all: Vec<&Outcome> = outcomes.iter().collect();
    let overall = summarise(&all, ks);
    let mut by_kind = Vec::new();
    for kind in KINDS {
        let subset: Vec<&Outcome> = outcomes.iter().filter(|o| o.kind == kind).collect();
        if !subset.is_empty() {
            by_kind.push((kind.to_string(), summarise(&subset, ks)));
        }
    }
    ArmReport {
        arm,
        not_run: None,
        method,
        outcomes,
        overall: Some(overall),
        by_kind,
    }
}

fn not_run(arm: Arm, reason: String) -> ArmReport {
    ArmReport {
        arm,
        not_run: Some(reason),
        method: String::new(),
        outcomes: Vec::new(),
        overall: None,
        by_kind: Vec::new(),
    }
}

pub fn load_gold(path: &Path) -> crate::Result<Vec<Gold>> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("eval: cannot read {}: {e}", path.display()))?;
    let mut out: Vec<Gold> = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let at = i + 1;
        let item: Gold = serde_json::from_str(line).map_err(|e| format!("eval: gold line {at}: {e}"))?;
        if !KINDS.contains(&item.kind.as_str()) {
            return Err(format!("eval: gold line {at}: kind {} is not one of {}", item.kind, KINDS.join(", ")));
        }
        let e = &item.expect;
        if e.slots.is_empty() && e.files.is_empty() && e.symbols.is_empty() && e.memory.is_empty() {
            return Err(format!("eval: gold line {at}: {} expects nothing", item.id));
        }
        if out.iter().any(|g| g.id == item.id) {
            return Err(format!("eval: gold line {at}: id {} is used twice", item.id));
        }
        out.push(item);
    }
    if out.is_empty() {
        return Err(format!("eval: {} has no tasks", path.display()));
    }
    Ok(out)
}

fn target_of<'a>(targets: &'a [Target], name: &str) -> crate::Result<&'a Target> {
    targets
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("eval: the gold set names corpus {name}, which is not indexed"))
}

fn excerpt_spans(text: &str) -> Vec<(String, i64, i64)> {
    let mut out = Vec::new();
    for line in text.lines().filter(|l| l.starts_with('{')) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let path = v["source"]["path"].as_str().unwrap_or("");
        let a = v["source"]["lines"][0].as_i64().unwrap_or(0);
        let b = v["source"]["lines"][1].as_i64().unwrap_or(0);
        if !path.is_empty() {
            out.push((path.to_string(), a, b));
        }
    }
    out
}

fn unit_lines(p1: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in p1.lines().filter(|l| l.starts_with("  ") && !l.trim_start().starts_with("files:")) {
        let body = line.trim_start();
        let mut parts = body.splitn(3, ' ');
        let place = parts.next().unwrap_or("");
        let _kind = parts.next();
        let qualified = parts.next().unwrap_or("");
        if let Some((path, _)) = place.rsplit_once(':') {
            out.push((path.to_string(), qualified.to_string()));
        }
    }
    out
}

fn card_files(p1: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in p1.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("files: ") {
            for item in rest.split(", ") {
                let path = item.split(" (").next().unwrap_or(item);
                out.push(path.to_string());
            }
        }
    }
    out
}

fn in_pack(pack: &Pack, entries: &[Entry], conn: &Connection, corpus_id: i64, with_p0: bool) -> bool {
    let p1 = &pack.parts.p1.text;
    let p2 = &pack.parts.p2.text;
    let spans = excerpt_spans(p2);
    let units = unit_lines(p1);
    let cards = card_files(p1);
    let memory_lines: String = p2
        .lines()
        .filter(|l| l.starts_with("- ["))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    let p0 = pack.parts.p0.text.to_lowercase();
    entries.iter().any(|entry| match entry {
        Entry::File(path) => {
            spans.iter().any(|s| s.0 == *path) || units.iter().any(|u| u.0 == *path) || cards.iter().any(|c| c == path)
        }
        Entry::Symbol(sym) => {
            let Some((path, qualified)) = sym.split_once('#') else { return false };
            if units.iter().any(|u| u.0 == path && u.1 == qualified) {
                return true;
            }
            let mut stmt = match conn
                .prepare_cached("SELECT start_line, end_line FROM search_unit WHERE corpus_id = ?1 AND path = ?2 AND qualified = ?3")
            {
                Ok(s) => s,
                Err(_) => return false,
            };
            let rows: Vec<(i64, i64)> = stmt
                .query_map(params![corpus_id, path, qualified], |r| Ok((r.get(0)?, r.get(1)?)))
                .map(|m| m.flatten().collect())
                .unwrap_or_default();
            spans
                .iter()
                .any(|s| s.0 == path && rows.iter().any(|(a, b)| s.1 <= *b && s.2 >= *a))
        }
        Entry::Slot(id) => pack.slots.iter().any(|s| s.id == *id),
        Entry::Memory(phrase) => memory_lines.contains(phrase.as_str()) || (with_p0 && p0.contains(phrase.as_str())),
    })
}

fn run_formslot(store: &mut Store, gold: &[Gold], targets: &[Target], ks: &[usize]) -> crate::Result<ArmReport> {
    let mut memo = Memo { cache: HashMap::new() };
    let mut outcomes = Vec::new();
    for item in gold {
        let target = target_of(targets, &item.corpus)?;
        let entries = entries_of(&item.expect);
        let started = Instant::now();
        let found = search::retrieve(store.conn(), std::slice::from_ref(target), Some(target.id), &item.task)?;
        let built = pack::run(store, &item.task, Some(&target.name), Some(PACK_BUDGET), None)?;
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        let cands: Vec<Cand> = found
            .hits
            .iter()
            .map(|h| Cand {
                kind: h.kind.clone(),
                id: h.id.clone(),
                path: h.path.clone(),
                qualified: h.qualified.clone(),
            })
            .collect();
        let conn = store.conn();
        let (first, gains) = walk(&cands, &entries, conn, &mut memo);
        outcomes.push(Outcome {
            id: item.id.clone(),
            kind: item.kind.clone(),
            first,
            gains,
            wanted: entries.len(),
            budget_found: Some(in_pack(&built, &entries, conn, target.id, false)),
            budget_found_p0: Some(in_pack(&built, &entries, conn, target.id, true)),
            tokens: built.tokens.sent,
            millis,
        });
    }
    Ok(finish(
        Arm::Formslot,
        format!("the fused search ranking and the T4 pack at {PACK_BUDGET} tokens; tokens are the pack's sent count; latency is search plus pack"),
        outcomes,
        ks,
    ))
}

fn ripgrep_version() -> Option<String> {
    let out = Command::new("rg").arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).lines().next().map(str::to_string)
}

fn tree_clean(path: &str, head: &str) -> bool {
    let Ok(repo) = Repo::open(Path::new(path)) else {
        return false;
    };
    if repo.rev_parse("HEAD").ok().as_deref() != Some(head) {
        return false;
    }
    Command::new("git")
        .args(["-C", path, "status", "--porcelain", "--untracked-files=no"])
        .output()
        .map(|o| o.status.success() && o.stdout.is_empty())
        .unwrap_or(false)
}

fn code_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    !lower.starts_with("licenses/") && !lower.contains("/licenses/") && !lower.ends_with("hashout_log.md")
        && !matches!(ext, "md" | "mdx" | "txt" | "html" | "json" | "lock" | "svg" | "csv" | "rst")
}

fn rank_files(mut counts: Vec<(String, usize)>) -> Vec<(String, usize)> {
    counts.retain(|(path, _)| code_file(path));
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts.truncate(BASELINE_FILE_CAP);
    counts
}

fn rg_counts(root: &str, terms: &[String]) -> Option<Vec<(String, usize)>> {
    let mut cmd = Command::new("rg");
    cmd.current_dir(root).args(["--no-messages", "--count", "--ignore-case", "--fixed-strings"]);
    for term in terms {
        cmd.arg("-e").arg(term);
    }
    cmd.arg("--").arg(".");
    let out = cmd.output().ok()?;
    if !out.status.success() && out.status.code() != Some(1) {
        return None;
    }
    let mut counts = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((path, n)) = line.rsplit_once(':') else { continue };
        let Ok(n) = n.parse::<usize>() else { continue };
        counts.push((path.strip_prefix("./").unwrap_or(path).to_string(), n));
    }
    Some(counts)
}

struct Walk {
    files: Vec<(String, String)>,
    blobs: Option<crate::git::BlobReader>,
    cache: HashMap<String, Vec<u8>>,
}

impl Walk {
    fn open(path: &str, head: &str) -> crate::Result<Walk> {
        let repo = Repo::open(Path::new(path))?;
        let files = repo
            .ls_tree(head)?
            .into_iter()
            .filter(|e| (e.size as u64) <= BLOB_CAP)
            .map(|e| (e.path, e.oid))
            .collect();
        let blobs = repo.blob_reader()?;
        Ok(Walk {
            files,
            blobs: Some(blobs),
            cache: HashMap::new(),
        })
    }

    fn bytes(&mut self, oid: &str) -> Option<&Vec<u8>> {
        if !self.cache.contains_key(oid) {
            let data = self.blobs.as_mut()?.read(oid).ok().flatten()?;
            self.cache.insert(oid.to_string(), data);
        }
        self.cache.get(oid)
    }

    fn counts(&mut self, terms: &[String]) -> Vec<(String, usize)> {
        let files = self.files.clone();
        let mut out = Vec::new();
        for (path, oid) in files {
            let Some(data) = self.bytes(&oid) else { continue };
            if data.contains(&0) {
                continue;
            }
            let body = String::from_utf8_lossy(data).to_lowercase();
            let n = body.lines().filter(|l| terms.iter().any(|t| l.contains(t.as_str()))).count();
            if n > 0 {
                out.push((path, n));
            }
        }
        out
    }

    fn size(&mut self, path: &str) -> usize {
        let oid = self.files.iter().find(|f| f.0 == path).map(|f| f.1.clone());
        oid.and_then(|o| self.bytes(&o).map(|d| d.len())).unwrap_or(0)
    }
}

fn run_baseline(store: &Store, gold: &[Gold], targets: &[Target], ks: &[usize]) -> crate::Result<ArmReport> {
    let rg = ripgrep_version();
    let mut memo = Memo { cache: HashMap::new() };
    let mut walks: HashMap<String, Walk> = HashMap::new();
    let mut used_rg = false;
    let mut used_walk = false;
    let mut outcomes = Vec::new();
    for item in gold {
        let target = target_of(targets, &item.corpus)?;
        let entries = entries_of(&item.expect);
        let terms = text::query_terms(&item.task);
        let started = Instant::now();
        let mut ranked: Option<Vec<(String, usize)>> = None;
        let mut sizes: Option<Vec<usize>> = None;
        if rg.is_some() && tree_clean(&target.path, &target.head) {
            if let Some(counts) = rg_counts(&target.path, &terms) {
                let top = rank_files(counts);
                sizes = Some(
                    top.iter()
                        .take(BASELINE_READ)
                        .map(|(p, _)| std::fs::metadata(Path::new(&target.path).join(p)).map(|m| m.len() as usize).unwrap_or(0))
                        .collect(),
                );
                ranked = Some(top);
                used_rg = true;
            }
        }
        let (top, sizes) = match (ranked, sizes) {
            (Some(r), Some(s)) => (r, s),
            _ => {
                if !walks.contains_key(&target.name) {
                    walks.insert(target.name.clone(), Walk::open(&target.path, &target.head)?);
                }
                let walk = walks.get_mut(&target.name).ok_or("eval: no walk")?;
                let top = rank_files(walk.counts(&terms));
                let sizes = top.iter().take(BASELINE_READ).map(|(p, _)| walk.size(p)).collect();
                used_walk = true;
                (top, sizes)
            }
        };
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        let cands: Vec<Cand> = top
            .iter()
            .map(|(p, _)| Cand {
                kind: "file".to_string(),
                id: String::new(),
                path: p.clone(),
                qualified: String::new(),
            })
            .collect();
        let (first, gains) = walk(&cands, &entries, store.conn(), &mut memo);
        let found4 = first.is_some_and(|f| f <= BASELINE_READ);
        outcomes.push(Outcome {
            id: item.id.clone(),
            kind: item.kind.clone(),
            first,
            gains,
            wanted: entries.len(),
            budget_found: Some(found4),
            budget_found_p0: None,
            tokens: sizes.iter().map(|b| tokens::code(*b)).sum(),
            millis,
        });
    }
    let how = match (used_rg, used_walk, rg) {
        (true, false, Some(v)) => format!("{v} over the clean checkout at the indexed commit"),
        (false, true, _) => "the built-in walk over the indexed files read from git at the indexed commit (ripgrep was not used)".to_string(),
        (true, true, Some(v)) => format!("{v} where the checkout was clean at the indexed commit, the built-in git walk otherwise"),
        _ => "no search ran".to_string(),
    };
    Ok(finish(
        Arm::Baseline,
        format!(
            "{how}; the task's terms with stop words dropped, case-insensitive, fixed strings; files ranked by matching-line count; tokens are the estimate of the top {BASELINE_READ} files read whole; the R@4k columns are the top {BASELINE_READ} files"
        ),
        outcomes,
        ks,
    ))
}

pub fn run(store: &mut Store, gold: Vec<Gold>, gold_path: &str, corpus: Option<&str>, arms: &[Arm], ks: &[usize]) -> crate::Result<Report> {
    let gold: Vec<Gold> = match corpus {
        Some(c) => gold.into_iter().filter(|g| g.corpus == c).collect(),
        None => gold,
    };
    if gold.is_empty() {
        return Err("eval: no gold task is left after the corpus filter".to_string());
    }
    let mut names: Vec<String> = gold.iter().map(|g| g.corpus.clone()).collect();
    names.sort();
    names.dedup();
    let mut targets = Vec::new();
    for name in &names {
        let mut found = search::resolve(store.conn(), Some(name))?;
        targets.append(&mut found);
    }
    search::ensure(store.conn(), &targets)?;
    let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();
    for g in &gold {
        *kind_counts.entry(g.kind.clone()).or_default() += 1;
    }
    let kind_counts = KINDS
        .iter()
        .map(|k| (k.to_string(), kind_counts.get(*k).copied().unwrap_or(0)))
        .collect();
    let mut reports = Vec::new();
    for arm in arms {
        reports.push(match arm {
            Arm::Formslot => run_formslot(store, &gold, &targets, ks)?,
            Arm::Baseline => run_baseline(store, &gold, &targets, ks)?,
            Arm::Dense => not_run(
                Arm::Dense,
                if DENSE_FEATURE {
                    "the build has the `dense` feature, but the evaluation harness has no hook into the dense tier yet".to_string()
                } else {
                    "this build has no `dense` cargo feature, so there is no embedding index or model to rank with".to_string()
                },
            ),
        });
    }
    Ok(Report {
        gold_path: gold_path.to_string(),
        total: gold.len(),
        kind_counts,
        corpora: targets.iter().map(|t| (t.name.clone(), t.head.clone())).collect(),
        ks: ks.to_vec(),
        arms: reports,
    })
}

fn pct(v: f64) -> String {
    format!("{:.3}", v + 0.0)
}

fn opt(v: Option<f64>) -> String {
    v.map(pct).unwrap_or_else(|| "-".to_string())
}

fn row(arm: &str, kind: &str, s: &Summary) -> Vec<String> {
    let mut cells = vec![arm.to_string(), kind.to_string(), s.n.to_string()];
    cells.extend(s.recall.iter().map(|v| pct(*v)));
    cells.push(pct(s.mrr));
    cells.push(pct(s.ndcg));
    cells.push(opt(s.budget));
    cells.push(opt(s.budget_p0));
    cells.push(format!("{:.0}", s.tokens_mean));
    cells.push(format!("{:.0}", s.tokens_median));
    cells.push(format!("{:.1}", s.millis_median));
    cells
}

fn header(ks: &[usize]) -> Vec<String> {
    let mut cells = vec!["arm".to_string(), "kind".to_string(), "n".to_string()];
    cells.extend(ks.iter().map(|k| format!("R@{k}")));
    for name in ["MRR", "nDCG@10", "R@4k", "R@4k+P0", "tok mean", "tok median", "ms median"] {
        cells.push(name.to_string());
    }
    cells
}

fn rows(report: &Report) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for a in &report.arms {
        let Some(overall) = &a.overall else { continue };
        out.push(row(a.arm.name(), "all", overall));
        for (kind, s) in &a.by_kind {
            out.push(row(a.arm.name(), kind, s));
        }
    }
    out
}

pub fn table(report: &Report) -> String {
    let head = header(&report.ks);
    let body = rows(report);
    let mut widths: Vec<usize> = head.iter().map(|h| h.len()).collect();
    for r in &body {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.len());
        }
    }
    let line = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| if i < 2 { format!("{c:<w$}", w = widths[i]) } else { format!("{c:>w$}", w = widths[i]) })
            .collect::<Vec<_>>()
            .join("  ")
    };
    let mut out = String::new();
    out.push_str(&line(&head));
    out.push('\n');
    for r in &body {
        out.push_str(&line(r));
        out.push('\n');
    }
    for a in &report.arms {
        if let Some(reason) = &a.not_run {
            out.push_str(&format!("{}: not run: {reason}\n", a.arm.name()));
        }
    }
    out
}

fn markdown_table(report: &Report) -> String {
    let head = header(&report.ks);
    let mut out = format!("| {} |\n", head.join(" | "));
    out.push_str(&format!("|{}|\n", head.iter().map(|_| " --- ").collect::<Vec<_>>().join("|")));
    for r in rows(report) {
        out.push_str(&format!("| {} |\n", r.join(" | ")));
    }
    out
}

pub struct Gates {
    pub lines: Vec<String>,
}

fn arm_of(report: &Report, arm: Arm) -> Option<&Summary> {
    report.arms.iter().find(|a| a.arm == arm).and_then(|a| a.overall.as_ref())
}

pub fn gates(report: &Report) -> Gates {
    let formslot = arm_of(report, Arm::Formslot);
    let baseline = arm_of(report, Arm::Baseline);
    let mut lines = Vec::new();
    match (formslot, baseline) {
        (Some(f), Some(b)) if b.tokens_mean > 0.0 => {
            let drop = 1.0 - f.tokens_mean / b.tokens_mean;
            let verdict = if drop >= GATE_TOKEN_DROP { "met" } else { "not met" };
            lines.push(format!(
                "Input tokens down at least {:.0} percent: measured, {verdict}. The formslot pack averages {:.0} tokens per task against {:.0} for the baseline (top {BASELINE_READ} files read whole), a change of {:.1} percent. This compares what is sent, not whether the answer is right.",
                GATE_TOKEN_DROP * 100.0,
                f.tokens_mean,
                b.tokens_mean,
                -drop * 100.0
            ));
        }
        _ => lines.push(format!(
            "Input tokens down at least {:.0} percent: not measured, it needs both the formslot and the baseline arm.",
            GATE_TOKEN_DROP * 100.0
        )),
    }
    match formslot.and_then(|f| f.budget) {
        Some(r) => {
            let verdict = if r >= GATE_RECALL { "met" } else { "not met" };
            let with_p0 = formslot.and_then(|f| f.budget_p0).map(pct).unwrap_or_else(|| "-".to_string());
            let base = baseline.and_then(|b| b.budget).map(|v| format!("{} with its top {BASELINE_READ} files", pct(v))).unwrap_or_else(|| "not run".to_string());
            lines.push(format!(
                "Recall@4k at least {GATE_RECALL:.2}: measured, {verdict}. {} of the gold tasks have an expected item in the {PACK_BUDGET}-token pack outside the stable prefix and the echoed task (with the prefix counted: {with_p0}); the baseline reaches {base}.",
            pct(r)
            ));
        }
        None => lines.push(format!("Recall@4k at least {GATE_RECALL:.2}: not measured, the formslot arm did not run.")),
    }
    let key = if std::env::var_os("ANTHROPIC_API_KEY").is_some() {
        "a key is in the environment but the harness makes no model calls unless the owner asks for the pass-rate arm"
    } else {
        "it needs model calls and there is no ANTHROPIC_API_KEY in the environment"
    };
    lines.push(format!("Pass rate within 1 point: not run, {key}."));
    Gates { lines }
}

pub fn markdown(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("# formslot evaluation\n\n");
    out.push_str(&format!("formslot {} · gold set `{}` · {} tasks\n\n", crate::VERSION, report.gold_path, report.total));
    out.push_str("## Set\n\n");
    let kinds: Vec<String> = report.kind_counts.iter().map(|(k, n)| format!("{k} {n}")).collect();
    out.push_str(&format!("- kinds: {}\n", kinds.join(", ")));
    for (name, head) in &report.corpora {
        out.push_str(&format!("- corpus {name} at {head}\n"));
    }
    out.push_str(&format!("- token estimate: {}\n\n", pack::ESTIMATOR));
    out.push_str("## Results\n\n");
    out.push_str(&markdown_table(report));
    out.push('\n');
    out.push_str("R@k: share of tasks with an expected file, symbol, slot or memory phrase in the top k of the ranking. R@4k: share with an expected item in the pack sent at 4,000 tokens (baseline: in the top 4 files read); R@4k+P0 also counts the stable prefix. Tokens are per task.\n\n");
    out.push_str("## Arms\n\n");
    for a in &report.arms {
        match &a.not_run {
            Some(reason) => out.push_str(&format!("- {}: not run. {reason}.\n", a.arm.name())),
            None => out.push_str(&format!("- {}: {}.\n", a.arm.name(), a.method)),
        }
    }
    out.push_str("\n## Gates\n\n");
    for line in gates(report).lines {
        out.push_str(&format!("- {line}\n"));
    }
    out.push_str("\n## Matching\n\n");
    out.push_str("- A file counts when a unit or slot hit has that path; a symbol when a unit hit's `path#qualified` is the expected string; a slot when a slot hit has the id; a memory phrase when a memory hit's title or text holds it, ignoring case.\n");
    out.push_str("- The baseline ranks files only, so a symbol counts for it when the file is the symbol's file, and it cannot reach slot or memory items.\n");
    out.push_str("- nDCG@10 gives a gain of 1 to each hit that meets an expected entry not met before; the ideal run has one gain per expected entry, up to 10.\n");
    out
}

pub fn json(report: &Report) -> Value {
    let summary = |s: &Summary| {
        json!({
            "n": s.n,
            "recall": report.ks.iter().zip(s.recall.iter()).map(|(k, v)| json!({"k": k, "value": v})).collect::<Vec<_>>(),
            "mrr": s.mrr,
            "ndcg10": s.ndcg,
            "recall_4k": s.budget,
            "recall_4k_with_prefix": s.budget_p0,
            "tokens_mean": s.tokens_mean,
            "tokens_median": s.tokens_median,
            "millis_median": s.millis_median,
        })
    };
    let arms: Vec<Value> = report
        .arms
        .iter()
        .map(|a| match &a.not_run {
            Some(reason) => json!({"arm": a.arm.name(), "run": false, "reason": reason}),
            None => json!({
                "arm": a.arm.name(),
                "run": true,
                "method": a.method,
                "overall": a.overall.as_ref().map(summary),
                "by_kind": a.by_kind.iter().map(|(k, s)| json!({"kind": k, "summary": summary(s)})).collect::<Vec<_>>(),
                "tasks": a.outcomes.iter().map(|o| json!({
                    "id": o.id,
                    "kind": o.kind,
                    "first_rank": o.first,
                    "in_pack": o.budget_found,
                    "tokens": o.tokens,
                    "millis": o.millis,
                })).collect::<Vec<_>>(),
            }),
        })
        .collect();
    json!({
        "format": "formslot.eval",
        "format_version": 1,
        "version": crate::VERSION,
        "gold": report.gold_path,
        "tasks": report.total,
        "kinds": report.kind_counts.iter().map(|(k, n)| json!({"kind": k, "count": n})).collect::<Vec<_>>(),
        "k": report.ks,
        "arms": arms,
        "gates": gates(report).lines,
    })
}
