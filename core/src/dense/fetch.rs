use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

use super::{MODEL, MODEL_LICENCE, MODEL_REVISION, model_dir, runtime_dir};

pub struct Part {
    pub remote: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub const PARTS: [Part; 2] = [
    Part {
        remote: "onnx/model_int8.onnx",
        file: "model_int8.onnx",
        bytes: 150_218_016,
        sha256: "bae96b276d342bf86eeee07c1bdbc0c75bb82bf4033941aab7fabc1e33ee3b44",
    },
    Part {
        remote: "tokenizer.json",
        file: "tokenizer.json",
        bytes: 3_583_228,
        sha256: "6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30",
    },
];

pub fn url(part: &Part) -> String {
    format!(
        "https://huggingface.co/Alibaba-NLP/{MODEL}/resolve/{MODEL_REVISION}/{}",
        part.remote
    )
}

pub fn model_file() -> PathBuf {
    model_dir().join(PARTS[0].file)
}

pub fn tokenizer_file() -> PathBuf {
    model_dir().join(PARTS[1].file)
}

pub fn present() -> bool {
    model_file().is_file() && tokenizer_file().is_file()
}

pub fn plan() -> String {
    let mut out = String::new();
    out.push_str(&format!("model: {MODEL} (int8 ONNX), licence {MODEL_LICENCE}\n"));
    out.push_str(&format!("revision: {MODEL_REVISION}\n"));
    let total: u64 = PARTS.iter().map(|p| p.bytes).sum();
    for part in &PARTS {
        out.push_str(&format!(
            "  {}  {:.1} MB  sha256 {}\n",
            url(part),
            part.bytes as f64 / 1_000_000.0,
            part.sha256
        ));
    }
    out.push_str(&format!("total: {:.1} MB\n", total as f64 / 1_000_000.0));
    out.push_str(&format!("target folder: {}\n", model_dir().display()));
    out.push_str(&format!(
        "ONNX Runtime (>= 1.23) is not downloaded by formslot: install it and set ORT_DYLIB_PATH to its shared library, or place the library in {}\n",
        runtime_dir().display()
    ));
    out
}

fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn verified(path: &Path, part: &Part) -> bool {
    path.is_file()
        && fs::metadata(path).map(|m| m.len() == part.bytes).unwrap_or(false)
        && digest(path).map(|d| d == part.sha256).unwrap_or(false)
}

pub fn download() -> crate::Result<Vec<String>> {
    let dir = model_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for private in [super::dense_dir(), dir.clone()] {
            fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot set mode on {}: {e}", private.display()))?;
        }
    }
    let mut notes = Vec::new();
    for part in &PARTS {
        let target = dir.join(part.file);
        if verified(&target, part) {
            notes.push(format!("{} already present and verified", part.file));
            continue;
        }
        let partial = dir.join(format!("{}.part", part.file));
        let _ = fs::remove_file(&partial);
        let status = Command::new("curl")
            .args(["--fail", "--location", "--proto", "=https", "--tlsv1.2", "--silent", "--show-error", "--progress-bar"])
            .arg("--output")
            .arg(&partial)
            .arg(url(part))
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("cannot run curl ({e}); download {} yourself into {} and run the command again", url(part), dir.display()))?;
        if !status.success() {
            let _ = fs::remove_file(&partial);
            return Err(format!("the download of {} failed (curl {status})", url(part)));
        }
        if !verified(&partial, part) {
            let _ = fs::remove_file(&partial);
            return Err(format!(
                "{} does not match the expected size and SHA-256 and was deleted",
                part.file
            ));
        }
        fs::rename(&partial, &target).map_err(|e| format!("cannot place {}: {e}", target.display()))?;
        notes.push(format!("{} downloaded, {} bytes, sha256 verified", part.file, part.bytes));
    }
    Ok(notes)
}
