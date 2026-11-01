use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::binres;

pub const RC_MARKER: &str = "# added by formslot setup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Debian,
    Fedora,
    Arch,
    OtherLinux,
    Windows,
    Other,
}

pub fn os() -> Os {
    if cfg!(target_os = "macos") {
        return Os::Mac;
    }
    if cfg!(windows) {
        return Os::Windows;
    }
    if cfg!(target_os = "linux") {
        let text = fs::read_to_string("/etc/os-release").unwrap_or_default();
        let mut ids: Vec<String> = Vec::new();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=')
                && (key == "ID" || key == "ID_LIKE")
            {
                ids.extend(value.trim_matches('"').split_whitespace().map(|s| s.to_lowercase()));
            }
        }
        let has = |names: &[&str]| ids.iter().any(|i| names.contains(&i.as_str()));
        return if has(&["debian", "ubuntu"]) {
            Os::Debian
        } else if has(&["fedora", "rhel", "centos"]) {
            Os::Fedora
        } else if has(&["arch"]) {
            Os::Arch
        } else {
            Os::OtherLinux
        };
    }
    Os::Other
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Required,
    Recommended,
    Optional,
}

#[derive(Debug, Clone)]
pub struct ToolCheck {
    pub name: &'static str,
    pub need: Need,
    pub purpose: &'static str,
    pub found: Option<String>,
    pub hint: String,
}

pub fn home_dir() -> Option<PathBuf> {
    let (first, second) = if cfg!(windows) { ("USERPROFILE", "HOME") } else { ("HOME", "USERPROFILE") };
    env::var_os(first)
        .filter(|v| !v.is_empty())
        .or_else(|| env::var_os(second).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
}

pub fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = text.strip_prefix("\\\\?\\") {
        rest.to_string()
    } else {
        text
    }
}

fn hint(tool: &str, os: Os) -> String {
    let brew = |p: &str| format!("brew install {p}");
    match (tool, os) {
        ("git", Os::Mac) => brew("git"),
        ("git", Os::Debian) => "sudo apt install git".into(),
        ("git", Os::Fedora) => "sudo dnf install git".into(),
        ("git", Os::Arch) => "sudo pacman -S git".into(),
        ("git", Os::Windows) => "winget install --id Git.Git -e".into(),
        ("git", _) => "install git from https://git-scm.com".into(),
        ("claude", Os::Mac) => "brew install --cask claude-code".into(),
        ("claude", Os::Windows) => "winget install Anthropic.ClaudeCode".into(),
        ("claude", _) => "curl -fsSL https://claude.ai/install.sh | bash".into(),
        ("rg", Os::Mac) => brew("ripgrep"),
        ("rg", Os::Debian) => "sudo apt install ripgrep".into(),
        ("rg", Os::Fedora) => "sudo dnf install ripgrep".into(),
        ("rg", Os::Arch) => "sudo pacman -S ripgrep".into(),
        ("rg", Os::Windows) => "winget install BurntSushi.ripgrep.MSVC".into(),
        ("rg", _) => "see https://github.com/BurntSushi/ripgrep#installation".into(),
        ("gitleaks", Os::Mac) => brew("gitleaks"),
        ("gitleaks", Os::Debian) => "sudo apt install gitleaks (newer releases), or download it from https://github.com/gitleaks/gitleaks/releases".into(),
        ("gitleaks", Os::Windows) => "winget install gitleaks.gitleaks".into(),
        ("gitleaks", _) => "download it from https://github.com/gitleaks/gitleaks/releases".into(),
        _ => String::new(),
    }
}

fn claude_elsewhere() -> Option<PathBuf> {
    let home = home_dir()?;
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    [home.join(".local").join("bin").join(exe), home.join(".claude").join("local").join(exe)]
        .into_iter()
        .find(|p| p.is_file())
}

pub fn check_tools() -> Vec<ToolCheck> {
    let os = os();
    let git = Command::new("git")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let found = |name: &str| binres::find(name).map(|p| p.display().to_string());
    vec![
        ToolCheck {
            name: "git",
            need: Need::Required,
            purpose: "formslot reads your projects through git",
            found: git,
            hint: hint("git", os),
        },
        ToolCheck {
            name: "claude",
            need: Need::Recommended,
            purpose: "Claude Code is the assistant that uses formslot while it works on your code",
            found: found("claude").or_else(|| claude_elsewhere().map(|p| p.display().to_string())),
            hint: hint("claude", os),
        },
        ToolCheck {
            name: "rg",
            need: Need::Optional,
            purpose: "ripgrep, a fast text search; only used when measuring formslot",
            found: found("rg"),
            hint: hint("rg", os),
        },
        ToolCheck {
            name: "gitleaks",
            need: Need::Optional,
            purpose: "gitleaks looks for passwords and keys that were saved into a project by mistake",
            found: found("gitleaks"),
            hint: hint("gitleaks", os),
        },
    ]
}

pub fn binary_name() -> &'static str {
    if cfg!(windows) { "formslot.exe" } else { "formslot" }
}

pub fn user_bin_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let base = env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(PathBuf::from)?;
        Some(base.join("Programs").join("formslot"))
    } else {
        Some(home_dir()?.join(".local").join("bin"))
    }
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (fs::File::open(a), fs::File::open(b)) else {
        return false;
    };
    let mut x = vec![0u8; 64 * 1024];
    let mut y = vec![0u8; 64 * 1024];
    loop {
        let (Ok(nx), Ok(ny)) = (fa.read(&mut x), fb.read(&mut y)) else {
            return false;
        };
        if nx != ny {
            let mut got = ny;
            while got < nx {
                match fb.read(&mut y[got..nx]) {
                    Ok(0) | Err(_) => return false,
                    Ok(n) => got += n,
                }
            }
            if x[..nx] != y[..nx] {
                return false;
            }
            continue;
        }
        if nx == 0 {
            return true;
        }
        if x[..nx] != y[..nx] {
            return false;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Same,
    Absent,
    Differs,
}

pub fn placement(src: &Path, dir: &Path) -> Placement {
    let dest = dir.join(binary_name());
    if !dest.exists() {
        return Placement::Absent;
    }
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if canon(src) == canon(&dest) || same_bytes(src, &dest) {
        Placement::Same
    } else {
        Placement::Differs
    }
}

pub fn place_binary(src: &Path, dir: &Path) -> crate::Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let dest = dir.join(binary_name());
    let temp = dir.join(format!("{}.new", binary_name()));
    if let Err(e) = fs::copy(src, &temp) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot copy to {}: {e}", temp.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&temp, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(&temp);
            return Err(format!("cannot make {} runnable: {e}", temp.display()));
        }
    }
    fs::rename(&temp, &dest).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace {}: {e}", dest.display())
    })?;
    Ok(dest)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

pub fn dir_on_path(dir: &Path) -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|d| !d.as_os_str().is_empty() && same_dir(&d, dir)))
        .unwrap_or(false)
}

pub fn path_with(dir: &Path) -> Option<std::ffi::OsString> {
    let mut parts = vec![dir.to_path_buf()];
    if let Some(path) = env::var_os("PATH") {
        parts.extend(env::split_paths(&path));
    }
    env::join_paths(parts).ok()
}

#[derive(Debug, Clone)]
pub struct RcPlan {
    pub shell: String,
    pub file: Option<PathBuf>,
    pub line: String,
}

pub fn bash_file(home: &Path, mac: bool) -> PathBuf {
    if !mac {
        return home.join(".bashrc");
    }
    [".bash_profile", ".bash_login", ".profile"]
        .iter()
        .map(|n| home.join(n))
        .find(|p| p.exists())
        .unwrap_or_else(|| home.join(".bash_profile"))
}

pub fn rc_plan(dir: &Path) -> RcPlan {
    let home = home_dir();
    let shown = match home.as_ref().and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) => format!("$HOME/{}", rest.to_string_lossy().replace('\\', "/")),
        None => dir.to_string_lossy().to_string(),
    };
    let shell = env::var("SHELL").ok().and_then(|s| s.rsplit('/').next().map(str::to_string)).unwrap_or_default();
    let export = format!("export PATH=\"{shown}:$PATH\"");
    let under_home = |rel: &str| home.as_ref().map(|h| h.join(rel));
    let env_dir = |name: &str| env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    match shell.as_str() {
        "zsh" => {
            let file = env_dir("ZDOTDIR").map(|d| d.join(".zshrc")).or_else(|| under_home(".zshrc"));
            RcPlan { shell, file, line: export }
        }
        "bash" => {
            let file = home.as_ref().map(|h| bash_file(h, cfg!(target_os = "macos")));
            RcPlan { shell, file, line: export }
        }
        "fish" => RcPlan {
            shell,
            file: env_dir("XDG_CONFIG_HOME")
                .map(|d| d.join("fish").join("config.fish"))
                .or_else(|| under_home(".config/fish/config.fish")),
            line: format!("fish_add_path \"{shown}\""),
        },
        _ => RcPlan { shell: if shell.is_empty() { "your shell".into() } else { shell }, file: None, line: export },
    }
}

pub fn rc_has(plan: &RcPlan) -> bool {
    let Some(file) = &plan.file else { return false };
    let Ok(bytes) = fs::read(file) else { return false };
    let text = String::from_utf8_lossy(&bytes);
    text.contains(RC_MARKER) || text.lines().any(|l| l.trim() == plan.line)
}

pub struct RcChange {
    pub backup: Option<PathBuf>,
}

pub fn add_to_rc(plan: &RcPlan) -> crate::Result<RcChange> {
    let Some(file) = &plan.file else {
        return Err("no startup file is known for this shell".into());
    };
    if rc_has(plan) {
        return Ok(RcChange { backup: None });
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let existing: Option<Vec<u8>> = if file.exists() {
        Some(fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?)
    } else {
        None
    };
    let mut backup = None;
    if existing.is_some() {
        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut copy = file.with_file_name(format!("{name}.formslot-backup"));
        if copy.exists() {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            copy = file.with_file_name(format!("{name}.formslot-backup-{stamp}"));
        }
        fs::copy(file, &copy).map_err(|e| format!("cannot back up {}: {e}", file.display()))?;
        backup = Some(copy);
    }
    let mut add = String::new();
    if existing.is_none()
        && file.file_name().is_some_and(|n| n == ".bash_profile")
        && file.with_file_name(".profile").is_file()
    {
        add.push_str("[ -f \"$HOME/.profile\" ] && . \"$HOME/.profile\"\n\n");
    }
    if existing.as_deref().is_some_and(|t| !t.is_empty() && !t.ends_with(b"\n")) {
        add.push('\n');
    }
    if existing.as_deref().is_some_and(|t| !t.is_empty()) {
        add.push('\n');
    }
    add.push_str(RC_MARKER);
    add.push('\n');
    add.push_str(&plan.line);
    add.push('\n');
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .map_err(|e| format!("cannot open {}: {e}", file.display()))?;
    out.write_all(add.as_bytes()).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(RcChange { backup })
}

pub fn windows_path_script(dir: &Path, backup: &Path) -> String {
    let d = dir.to_string_lossy().replace('\'', "''");
    let b = backup.to_string_lossy().replace('\'', "''");
    format!(
        "$d='{d}'; $b='{b}'; \
$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment'); \
$p=$k.GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); $k.Close(); \
if (($p -split ';') -notcontains $d) {{ \
Write-Host ('Your user Path was: ' + $p); \
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $b) | Out-Null; \
Set-Content -LiteralPath $b -Value $p -Encoding UTF8; \
Write-Host ('Saved a copy in ' + $b); \
$w=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment',$true); \
$w.SetValue('Path',(($p.TrimEnd(';') + ';' + $d).TrimStart(';')),[Microsoft.Win32.RegistryValueKind]::ExpandString); $w.Close(); \
[Environment]::SetEnvironmentVariable('FORMSLOT_PATH_NOTIFY',$null,'User') }}"
    )
}

pub fn path_backup_file(data_dir: &Path) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    data_dir.join(format!("user-path-backup-{stamp}.txt"))
}

pub fn clean_path(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    loop {
        let bytes = s.as_bytes();
        if bytes.len() >= 2 && (bytes[0] == b'"' || bytes[0] == b'\'') && bytes[bytes.len() - 1] == bytes[0] {
            s = s[1..s.len() - 1].trim().to_string();
        } else {
            break;
        }
    }
    if !cfg!(windows) {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(next) => out.push(next),
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        s = out;
    }
    if (s == "~" || s.starts_with("~/") || s.starts_with("~\\"))
        && let Some(home) = home_dir()
    {
        s = format!("{}{}", home.display(), &s[1..]);
    }
    s
}

pub fn rc_candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let env_dir = |name: &str| env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = env_dir("ZDOTDIR") {
        out.push(d.join(".zshrc"));
    }
    if let Some(d) = env_dir("XDG_CONFIG_HOME") {
        out.push(d.join("fish").join("config.fish"));
    }
    if let Some(h) = home_dir() {
        for n in [".zshrc", ".bashrc", ".bash_profile", ".bash_login", ".profile", ".config/fish/config.fish"] {
            out.push(h.join(n));
        }
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    for p in out {
        if !seen.contains(&p) {
            seen.push(p);
        }
    }
    seen
}

fn rc_block_lines(bytes: &[u8]) -> Vec<usize> {
    let lines: Vec<&[u8]> = bytes.split_inclusive(|b| *b == b'\n').collect();
    let mut hits = Vec::new();
    for i in 0..lines.len() {
        if lines[i].trim_ascii() != RC_MARKER.as_bytes() {
            continue;
        }
        if let Some(next) = lines.get(i + 1) {
            let t = next.trim_ascii();
            if t.starts_with(b"export PATH=") || t.starts_with(b"fish_add_path ") {
                hits.push(i);
            }
        }
    }
    hits
}

pub fn rc_marked() -> Vec<PathBuf> {
    rc_candidates()
        .into_iter()
        .filter(|p| p.is_file() && fs::read(p).map(|b| !rc_block_lines(&b).is_empty()).unwrap_or(false))
        .collect()
}

pub struct RcRemoval {
    pub removed: usize,
    pub backup: Option<PathBuf>,
}

pub fn remove_rc_block(file: &Path, dry_run: bool) -> crate::Result<RcRemoval> {
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RcRemoval { removed: 0, backup: None }),
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    let hits = rc_block_lines(&bytes);
    if hits.is_empty() {
        return Ok(RcRemoval { removed: 0, backup: None });
    }
    let lines: Vec<&[u8]> = bytes.split_inclusive(|b| *b == b'\n').collect();
    let mut drop = vec![false; lines.len()];
    for i in &hits {
        drop[*i] = true;
        drop[*i + 1] = true;
        if *i > 0 && lines[*i - 1].trim_ascii().is_empty() && *i - 1 > 0 {
            drop[*i - 1] = true;
        }
    }
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    for (i, line) in lines.iter().enumerate() {
        if !drop[i] {
            out.extend_from_slice(line);
        }
    }
    if dry_run {
        return Ok(RcRemoval { removed: hits.len(), backup: None });
    }
    let copy = crate::install::backup_name(file);
    fs::copy(file, &copy).map_err(|e| format!("cannot back up {}: {e}", file.display()))?;
    let temp = file.with_file_name(format!(".formslot-rc-{}", std::process::id()));
    fs::write(&temp, &out).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    if let Ok(meta) = fs::metadata(file) {
        let _ = fs::set_permissions(&temp, meta.permissions());
    }
    fs::rename(&temp, file).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("cannot write {}: {e}", file.display())
    })?;
    Ok(RcRemoval { removed: hits.len(), backup: Some(copy) })
}

pub fn windows_path_has_script(dir: &Path) -> String {
    let d = dir.to_string_lossy().replace('\'', "''");
    format!(
        "$d='{d}'.TrimEnd('\\'); \
$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment'); \
$p=$k.GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); $k.Close(); \
if (($p -split ';' | ForEach-Object {{ $_.Trim().TrimEnd('\\') }}) -contains $d) {{ Write-Output 'yes' }} else {{ Write-Output 'no' }}"
    )
}

pub fn windows_path_remove_script(dir: &Path, backup: &Path) -> String {
    let d = dir.to_string_lossy().replace('\'', "''");
    let b = backup.to_string_lossy().replace('\'', "''");
    format!(
        "$d='{d}'.TrimEnd('\\'); $b='{b}'; \
$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment'); \
$p=$k.GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); $k.Close(); \
$parts=@($p -split ';'); \
$keep=@($parts | Where-Object {{ $_.Trim().TrimEnd('\\') -ne $d }}); \
if ($keep.Count -ne $parts.Count) {{ \
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $b) | Out-Null; \
Set-Content -LiteralPath $b -Value $p -Encoding UTF8; \
Write-Host ('Saved a copy of your user Path in ' + $b); \
$w=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment',$true); \
$w.SetValue('Path',($keep -join ';'),[Microsoft.Win32.RegistryValueKind]::ExpandString); $w.Close(); \
[Environment]::SetEnvironmentVariable('FORMSLOT_PATH_NOTIFY',$null,'User'); Write-Output 'removed' }} else {{ Write-Output 'absent' }}"
    )
}

pub fn remove_binary(dir: &Path) -> crate::Result<bool> {
    let target = dir.join(binary_name());
    if fs::symlink_metadata(&target).is_err() {
        return Ok(false);
    }
    fs::remove_file(&target).map_err(|e| format!("cannot remove {}: {e}", target.display()))?;
    let _ = fs::remove_file(dir.join(format!("{}.new", binary_name())));
    if cfg!(windows) {
        let _ = fs::remove_dir(dir);
    }
    Ok(true)
}

pub fn data_dir_refusal(dir: &Path) -> Option<String> {
    let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if canon.parent().is_none() || canon.components().count() < 3 {
        return Some(format!("{} is too close to the top of the disk to delete safely", dir.display()));
    }
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if canon == home || home.starts_with(&canon) {
            return Some(format!("{} holds your home folder, so it is not deleted", dir.display()));
        }
    }
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Some(format!("{} is a link, so it is not deleted", dir.display()));
    }
    let ours = ["corpus.sqlite", "service.json", "formslot.log", "bin"].iter().any(|n| dir.join(n).exists());
    let empty = fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false);
    if !ours && !empty {
        return Some(format!("{} does not look like a formslot data folder (no corpus.sqlite), so it is not deleted", dir.display()));
    }
    None
}

pub fn remove_data_dir(dir: &Path) -> crate::Result<()> {
    if let Some(why) = data_dir_refusal(dir) {
        return Err(why);
    }
    fs::remove_dir_all(dir).map_err(|e| format!("cannot delete {}: {e}", dir.display()))
}
