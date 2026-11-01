use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::Store;

const FAMILY_FORMAT: &str = "formmcp-family";
const FAMILY_FORMAT_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyApp {
    pub id: String,
    pub app: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub repository: String,
    pub launcher: String,
    pub state: String,
}

#[derive(Deserialize)]
struct FamilyFile {
    format: String,
    format_version: u64,
    apps: Vec<FamilyApp>,
}

fn sql(e: rusqlite::Error) -> String {
    format!("family: {e}")
}

pub fn load_family_json(path: &Path) -> crate::Result<Vec<FamilyApp>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let file: FamilyFile = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not a family list: {e}", path.display()))?;
    if file.format != FAMILY_FORMAT {
        return Err(format!("unexpected family format {}", file.format).into());
    }
    if file.format_version != FAMILY_FORMAT_VERSION {
        return Err(format!("unsupported family format_version {}", file.format_version).into());
    }
    let mut apps = file.apps;
    apps.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(apps)
}

pub fn read_family(store: &Store) -> crate::Result<Vec<FamilyApp>> {
    let mut stmt = store
        .conn()
        .prepare(
            "SELECT id, app, name, kind, summary, repository, launcher, state FROM family ORDER BY id",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FamilyApp {
                id: r.get(0)?,
                app: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                summary: r.get(4)?,
                repository: r.get(5)?,
                launcher: r.get(6)?,
                state: r.get(7)?,
            })
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn write_family(store: &mut Store, apps: &[FamilyApp]) -> crate::Result<()> {
    let tx = store.conn_mut().transaction().map_err(sql)?;
    tx.execute("DELETE FROM family", []).map_err(sql)?;
    for app in apps {
        tx.execute(
            "INSERT INTO family (id, app, name, kind, summary, repository, launcher, state) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                app.id,
                app.app,
                app.name,
                app.kind,
                app.summary,
                app.repository,
                app.launcher,
                app.state
            ],
        )
        .map_err(sql)?;
    }
    tx.commit().map_err(sql)?;
    Ok(())
}

pub fn check(store: &Store, apps: &[FamilyApp]) -> crate::Result<bool> {
    let mut wanted = apps.to_vec();
    wanted.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(read_family(store)? == wanted)
}
