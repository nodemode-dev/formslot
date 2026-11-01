use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub mode: String,
    pub kind: String,
    pub oid: String,
    pub size: u64,
    pub path: String,
    pub path_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub status: String,
    pub path: String,
    pub path_hex: Option<String>,
    pub old_mode: String,
    pub new_mode: String,
    pub old_oid: String,
    pub new_oid: String,
}

#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
}

const SUBMODULE_MODE: &str = "160000";

fn base_command(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0");
    cmd
}

fn run_in(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = base_command(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), msg.trim()));
    }
    Ok(out.stdout)
}

pub fn decode_path(bytes: &[u8]) -> (String, Option<String>) {
    match String::from_utf8_lossy(bytes) {
        std::borrow::Cow::Borrowed(s) => (s.to_string(), None),
        std::borrow::Cow::Owned(s) => {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            (s, Some(hex))
        }
    }
}

pub fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.bytes().all(|b| b == b'0')
}

impl Repo {
    pub fn open(path: &Path) -> Result<Repo, String> {
        let canon = path
            .canonicalize()
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let out = run_in(&canon, &["rev-parse", "--show-toplevel"])?;
        let top = String::from_utf8_lossy(&out).trim().to_string();
        if top.is_empty() {
            return Err(format!("{} is not inside a git work tree", canon.display()));
        }
        Ok(Repo {
            root: PathBuf::from(top),
        })
    }

    pub fn toplevel(&self) -> &Path {
        &self.root
    }

    fn run(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        run_in(&self.root, args)
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String, String> {
        let spec = format!("{rev}^{{commit}}");
        let out = self.run(&["rev-parse", "--verify", "--end-of-options", &spec])?;
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }

    pub fn object_format(&self) -> Result<String, String> {
        match self.run(&["rev-parse", "--show-object-format"]) {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if text == "sha256" {
                    Ok(text)
                } else {
                    Ok("sha1".to_string())
                }
            }
            Err(_) => Ok("sha1".to_string()),
        }
    }

    pub fn remote_url(&self) -> Option<String> {
        let out = base_command(&self.root)
            .args(["config", "--get", "remote.origin.url"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if url.is_empty() { None } else { Some(url) }
    }

    pub fn ls_tree(&self, commit: &str) -> Result<Vec<Entry>, String> {
        let out = self.run(&["ls-tree", "-r", "-z", "-l", "--end-of-options", commit])?;
        let mut entries = Vec::new();
        for record in out.split(|b| *b == 0) {
            if record.is_empty() {
                continue;
            }
            let tab = record
                .iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| "malformed ls-tree record".to_string())?;
            let meta = String::from_utf8_lossy(&record[..tab]);
            let mut parts = meta.split_whitespace();
            let mode = parts.next().unwrap_or("").to_string();
            let kind = parts.next().unwrap_or("").to_string();
            let oid = parts.next().unwrap_or("").to_string();
            let size_text = parts.next().unwrap_or("-");
            if mode == SUBMODULE_MODE || kind != "blob" {
                continue;
            }
            let size = size_text.parse::<u64>().unwrap_or(0);
            let (path, path_hex) = decode_path(&record[tab + 1..]);
            entries.push(Entry {
                mode,
                kind,
                oid,
                size,
                path,
                path_hex,
            });
        }
        Ok(entries)
    }

    pub fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>, String> {
        let out = self.run(&[
            "diff-tree",
            "-r",
            "--raw",
            "--no-renames",
            "-z",
            "--end-of-options",
            from,
            to,
        ])?;
        let mut tokens = out.split(|b| *b == 0).filter(|t| !t.is_empty());
        let mut changes = Vec::new();
        while let Some(meta) = tokens.next() {
            if meta.first() != Some(&b':') {
                continue;
            }
            let path_bytes = tokens
                .next()
                .ok_or_else(|| "diff-tree record without a path".to_string())?;
            let text = String::from_utf8_lossy(&meta[1..]);
            let mut parts = text.split_whitespace();
            let old_mode = parts.next().unwrap_or("").to_string();
            let new_mode = parts.next().unwrap_or("").to_string();
            let old_oid = parts.next().unwrap_or("").to_string();
            let new_oid = parts.next().unwrap_or("").to_string();
            let status = parts.next().unwrap_or("").to_string();
            let (path, path_hex) = decode_path(path_bytes);
            changes.push(Change {
                status,
                path,
                path_hex,
                old_mode,
                new_mode,
                old_oid,
                new_oid,
            });
        }
        Ok(changes)
    }

    pub fn blob_reader(&self) -> Result<BlobReader, String> {
        BlobReader::spawn(&self.root)
    }
}

pub struct BlobReader {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl BlobReader {
    fn spawn(root: &Path) -> Result<BlobReader, String> {
        let mut child = base_command(root)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start git cat-file: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "git cat-file has no stdout".to_string())?;
        Ok(BlobReader {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub fn read(&mut self, oid: &str) -> Result<Option<Vec<u8>>, String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "blob reader is closed".to_string())?;
        stdin
            .write_all(oid.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("write to git cat-file failed: {e}"))?;
        let mut header = String::new();
        let n = self
            .stdout
            .read_line(&mut header)
            .map_err(|e| format!("read from git cat-file failed: {e}"))?;
        if n == 0 {
            return Err("git cat-file ended unexpectedly".to_string());
        }
        let header = header.trim_end();
        if header.ends_with(" missing") {
            return Ok(None);
        }
        let mut parts = header.split(' ');
        let _name = parts.next();
        let kind = parts.next().unwrap_or("");
        let size = parts
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| format!("bad cat-file header: {header}"))?;
        let mut data = vec![0u8; size];
        self.stdout
            .read_exact(&mut data)
            .map_err(|e| format!("short read from git cat-file: {e}"))?;
        let mut newline = [0u8; 1];
        self.stdout
            .read_exact(&mut newline)
            .map_err(|e| format!("short read from git cat-file: {e}"))?;
        if kind != "blob" {
            return Ok(None);
        }
        Ok(Some(data))
    }
}

impl Drop for BlobReader {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
