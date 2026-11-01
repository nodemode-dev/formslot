use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};

use super::family;

#[derive(Debug, Clone, Default)]
pub struct LinkStats {
    pub edges: u64,
    pub imports_resolved: u64,
    pub imports_external: u64,
    pub ambiguous: u64,
    pub unresolved: u64,
}

fn sql(e: rusqlite::Error) -> String {
    format!("symbols: {e}")
}

struct File {
    path: String,
    family: &'static str,
}

struct U {
    file: usize,
    kind: String,
    name: String,
    qualified: String,
    first: u32,
    last: u32,
}

type Edge = (String, String, &'static str, String, String, u32);

pub fn link(conn: &Connection, corpus_id: i64) -> crate::Result<LinkStats> {
    conn.execute_batch("SAVEPOINT symbols_link").map_err(sql)?;
    match build(conn, corpus_id) {
        Ok(stats) => {
            conn.execute_batch("RELEASE symbols_link").map_err(sql)?;
            Ok(stats)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO symbols_link; RELEASE symbols_link");
            Err(e)
        }
    }
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

fn join_norm(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for segment in rel.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn resolve_ts(from: &str, source: &str, index: &HashMap<String, usize>) -> Option<usize> {
    if !source.starts_with('.') {
        return None;
    }
    let base = join_norm(parent_dir(from), source)?;
    let mut candidates = vec![base.clone()];
    for ext in [".js", ".jsx", ".mjs"] {
        if let Some(stem) = base.strip_suffix(ext) {
            candidates.push(format!("{stem}.ts"));
            candidates.push(format!("{stem}.tsx"));
        }
    }
    for tail in [".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.tsx", "/index.js"] {
        candidates.push(format!("{base}{tail}"));
    }
    candidates.iter().find_map(|c| index.get(c).copied())
}

fn module_file(module: &str, index: &HashMap<String, usize>) -> Option<usize> {
    if module.is_empty() {
        return index.get("__init__.py").copied();
    }
    index
        .get(&format!("{module}.py"))
        .or_else(|| index.get(&format!("{module}/__init__.py")))
        .copied()
}

fn resolve_py(
    from: &str,
    source: &str,
    names: &[String],
    index: &HashMap<String, usize>,
) -> (Option<usize>, Vec<(String, usize)>) {
    let dots = source.chars().take_while(|c| *c == '.').count();
    let rest = source[dots..].replace('.', "/");
    let mut roots: Vec<String> = Vec::new();
    let mut dir = parent_dir(from).to_string();
    if dots > 0 {
        for _ in 1..dots {
            dir = parent_dir(&dir).to_string();
        }
        roots.push(dir);
    } else {
        loop {
            roots.push(dir.clone());
            if dir.is_empty() {
                break;
            }
            dir = parent_dir(&dir).to_string();
        }
    }
    for root in roots {
        let Some(module) = join_norm(&root, &rest) else {
            continue;
        };
        let target = if rest.is_empty() {
            let init = if module.is_empty() {
                "__init__.py".to_string()
            } else {
                format!("{module}/__init__.py")
            };
            index.get(&init).copied()
        } else {
            module_file(&module, index)
        };
        let mut subs = Vec::new();
        for name in names {
            if name == "*" {
                continue;
            }
            let sub = if module.is_empty() {
                name.clone()
            } else {
                format!("{module}/{name}")
            };
            if let Some(found) = module_file(&sub, index) {
                subs.push((name.clone(), found));
            }
        }
        if target.is_some() || !subs.is_empty() {
            return (target, subs);
        }
    }
    (None, Vec::new())
}

fn callable(kind: &str) -> bool {
    matches!(
        kind,
        "function" | "method" | "class" | "const" | "constant" | "tauri_command" | "static" | "struct"
    )
}

fn typed(kind: &str) -> bool {
    matches!(
        kind,
        "interface" | "type" | "class" | "enum" | "struct" | "trait" | "namespace"
    )
}

fn build(conn: &Connection, corpus_id: i64) -> crate::Result<LinkStats> {
    let format: String = conn
        .query_row(
            "SELECT object_format FROM corpus WHERE id = ?1",
            params![corpus_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    conn.execute("DELETE FROM edge WHERE corpus_id = ?1", params![corpus_id])
        .map_err(sql)?;

    let mut files: Vec<File> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT path, language FROM file WHERE corpus_id = ?1 AND role IN ('source', 'data', 'doc') ORDER BY path",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(sql)?;
        for row in rows {
            let (path, language) = row.map_err(sql)?;
            let Some(fam) = family(&language) else {
                continue;
            };
            index.insert(path.clone(), files.len());
            files.push(File { path, family: fam });
        }
    }

    let mut units: Vec<U> = Vec::new();
    let mut by_file: Vec<Vec<usize>> = vec![Vec::new(); files.len()];
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT f.path, u.kind, u.name, u.qualified, u.start_line, u.end_line \
                 FROM file f JOIN unit u ON u.object_format = ?2 AND u.oid = f.oid \
                 WHERE f.corpus_id = ?1 AND f.role IN ('source', 'data', 'doc') \
                 ORDER BY f.path, u.idx",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus_id, format], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (path, kind, name, qualified, first, last) = row.map_err(sql)?;
            let Some(&file) = index.get(&path) else {
                continue;
            };
            let id = units.len();
            by_file[file].push(id);
            by_name.entry(name.clone()).or_default().push(id);
            units.push(U {
                file,
                kind,
                name,
                qualified,
                first: first as u32,
                last: last as u32,
            });
        }
    }

    let mut stats = LinkStats::default();
    let mut edges: Vec<Edge> = Vec::new();
    let mut seen: HashSet<Edge> = HashSet::new();
    let mut push = |edge: Edge, edges: &mut Vec<Edge>| {
        if seen.insert(edge.clone()) {
            edges.push(edge);
        }
    };

    let mut imported_names: HashMap<(usize, String), Vec<usize>> = HashMap::new();
    let mut imported_files: Vec<Vec<usize>> = vec![Vec::new(); files.len()];
    {
        let mut stmt = conn
            .prepare(
                "SELECT f.path, i.source, i.names, i.line \
                 FROM file f JOIN unit_import i ON i.object_format = ?2 AND i.oid = f.oid \
                 WHERE f.corpus_id = ?1 AND f.role IN ('source', 'data', 'doc') \
                 ORDER BY f.path, i.line",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus_id, format], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (path, source, names, line) = row.map_err(sql)?;
            let Some(&file) = index.get(&path) else {
                continue;
            };
            let names: Vec<String> = names
                .split(',')
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();
            let mut targets: Vec<(Option<String>, usize)> = Vec::new();
            match files[file].family {
                "ts" => {
                    if let Some(t) = resolve_ts(&path, &source, &index) {
                        targets.push((None, t));
                    }
                }
                "py" => {
                    let (module, subs) = resolve_py(&path, &source, &names, &index);
                    if let Some(t) = module {
                        targets.push((None, t));
                    }
                    for (name, t) in subs {
                        targets.push((Some(name), t));
                    }
                }
                _ => {}
            }
            if targets.is_empty() {
                stats.imports_external += 1;
                continue;
            }
            stats.imports_resolved += 1;
            for (name, target) in targets {
                imported_files[file].push(target);
                match name {
                    Some(n) => imported_names.entry((file, n)).or_default().push(target),
                    None => {
                        for n in &names {
                            imported_names.entry((file, n.clone())).or_default().push(target);
                        }
                    }
                }
                push(
                    (
                        path.clone(),
                        String::new(),
                        "import",
                        files[target].path.clone(),
                        String::new(),
                        line as u32,
                    ),
                    &mut edges,
                );
            }
        }
    }

    let mut earliest: HashMap<&str, usize> = HashMap::new();
    for (id, unit) in units.iter().enumerate() {
        if unit.kind != "custom_property" {
            continue;
        }
        let slot = earliest.entry(unit.name.as_str()).or_insert(id);
        let current = &units[*slot];
        if (&files[unit.file].path, unit.first) < (&files[current.file].path, current.first) {
            *slot = id;
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT f.path, r.name, r.kind, r.line \
             FROM file f JOIN unit_ref r ON r.object_format = ?2 AND r.oid = f.oid \
             WHERE f.corpus_id = ?1 AND f.role IN ('source', 'data', 'doc') \
             ORDER BY f.path, r.line",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id, format], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(sql)?;
    for row in rows {
        let (path, name, kind, line) = row.map_err(sql)?;
        let Some(&file) = index.get(&path) else {
            continue;
        };
        let line = line as u32;
        let enclosing = by_file[file]
            .iter()
            .copied()
            .filter(|&id| units[id].first <= line && line <= units[id].last)
            .min_by_key(|&id| (units[id].last - units[id].first, std::cmp::Reverse(id)));
        let from_q = enclosing.map(|id| units[id].qualified.clone()).unwrap_or_default();

        let (edge_kind, target): (&'static str, Option<usize>) = match kind.as_str() {
            "css_var" => match earliest.get(name.as_str()) {
                Some(&id) => ("css_var", Some(id)),
                None => {
                    stats.unresolved += 1;
                    continue;
                }
            },
            "invoke" => {
                let cands: Vec<usize> = by_name
                    .get(&name)
                    .map(|v| {
                        v.iter()
                            .copied()
                            .filter(|&id| units[id].kind == "tauri_command")
                            .collect()
                    })
                    .unwrap_or_default();
                match cands.len() {
                    0 => {
                        stats.unresolved += 1;
                        continue;
                    }
                    1 => ("invoke", Some(cands[0])),
                    _ => {
                        stats.ambiguous += 1;
                        continue;
                    }
                }
            }
            "call" | "type" => {
                let static_kind: &'static str = if kind == "call" { "call" } else { "type" };
                let accepts: fn(&str) -> bool = if kind == "call" { callable } else { typed };
                let cands: Vec<usize> = by_name
                    .get(&name)
                    .map(|v| {
                        v.iter()
                            .copied()
                            .filter(|&id| {
                                accepts(&units[id].kind)
                                    && files[units[id].file].family == files[file].family
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if cands.is_empty() {
                    stats.unresolved += 1;
                    continue;
                }
                let from_imports: Vec<usize> = imported_names
                    .get(&(file, name.clone()))
                    .cloned()
                    .unwrap_or_default();
                let stages: [Vec<usize>; 4] = [
                    cands
                        .iter()
                        .copied()
                        .filter(|&id| from_imports.contains(&units[id].file))
                        .collect(),
                    cands
                        .iter()
                        .copied()
                        .filter(|&id| imported_files[file].contains(&units[id].file))
                        .collect(),
                    cands
                        .iter()
                        .copied()
                        .filter(|&id| units[id].file == file)
                        .collect(),
                    cands.clone(),
                ];
                let mut chosen = None;
                let mut ambiguous = false;
                for stage in &stages {
                    match stage.len() {
                        0 => continue,
                        1 => {
                            chosen = Some(stage[0]);
                            break;
                        }
                        _ => {
                            ambiguous = true;
                            break;
                        }
                    }
                }
                match chosen {
                    Some(id) => (static_kind, Some(id)),
                    None => {
                        if ambiguous {
                            stats.ambiguous += 1;
                        }
                        continue;
                    }
                }
            }
            _ => continue,
        };
        let Some(target) = target else {
            continue;
        };
        if Some(target) == enclosing {
            continue;
        }
        push(
            (
                path.clone(),
                from_q,
                edge_kind,
                files[units[target].file].path.clone(),
                units[target].qualified.clone(),
                line,
            ),
            &mut edges,
        );
    }
    drop(stmt);

    let mut insert = conn
        .prepare_cached(
            "INSERT INTO edge (corpus_id, from_path, from_qualified, kind, to_path, to_qualified, line) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(sql)?;
    for (from_path, from_q, kind, to_path, to_q, line) in &edges {
        insert
            .execute(params![corpus_id, from_path, from_q, kind, to_path, to_q, *line as i64])
            .map_err(sql)?;
    }
    stats.edges = edges.len() as u64;
    Ok(stats)
}
