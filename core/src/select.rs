use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Source,
    Data,
    Doc,
    Generated,
    Mirror,
    Vendored,
    Log,
    Binary,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Source => "source",
            Role::Data => "data",
            Role::Doc => "doc",
            Role::Generated => "generated",
            Role::Mirror => "mirror",
            Role::Vendored => "vendored",
            Role::Log => "log",
            Role::Binary => "binary",
        }
    }

    pub fn parse(text: &str) -> Option<Role> {
        ROLES.iter().copied().find(|r| r.as_str() == text)
    }
}

pub const ROLES: &[Role] = &[
    Role::Source,
    Role::Data,
    Role::Doc,
    Role::Generated,
    Role::Mirror,
    Role::Vendored,
    Role::Log,
    Role::Binary,
];

pub struct Context {
    pub has_claude_skills: bool,
}

impl Context {
    pub fn from_paths<'a>(paths: impl Iterator<Item = &'a str>) -> Context {
        let mut has_claude_skills = false;
        for path in paths {
            if path.starts_with(".claude/skills/") {
                has_claude_skills = true;
                break;
            }
        }
        Context { has_claude_skills }
    }
}

const LANGUAGES: &[(&str, &[&str])] = &[
    ("ts", &["ts", "mts", "cts"]),
    ("tsx", &["tsx"]),
    ("js", &["js", "mjs", "cjs", "jsx"]),
    ("py", &["py", "pyi"]),
    ("rs", &["rs"]),
    ("css", &["css"]),
    ("html", &["html", "htm"]),
    ("md", &["md", "mdx"]),
    ("json", &["json", "jsonc"]),
    ("toml", &["toml"]),
    ("yaml", &["yaml", "yml"]),
    ("sh", &["sh", "bash", "zsh"]),
    ("wgsl", &["wgsl"]),
];

const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "icns", "tif", "tiff", "psd", "exr", "af",
    "afdesign", "icc", "woff", "woff2", "ttf", "otf", "eot", "wasm", "onnx", "safetensors", "pt",
    "pth", "ckpt", "bin", "zip", "gz", "tgz", "xz", "bz2", "zst", "7z", "rar", "pdf", "mp3",
    "wav", "flac", "ogg", "mp4", "mov", "webm", "mkv", "dll", "so", "dylib", "exe", "a", "o",
    "lib", "shc", "pyc", "sqlite", "db",
];

#[derive(Clone, Copy)]
struct Rule {
    role: Role,
    prefix: &'static str,
    segment: &'static str,
    suffix: &'static str,
    names: &'static [&'static str],
    not_names: &'static [&'static str],
    needs_claude_skills: bool,
}

const ANY: Rule = Rule {
    role: Role::Source,
    prefix: "",
    segment: "",
    suffix: "",
    names: &[],
    not_names: &[],
    needs_claude_skills: false,
};

const SKIP_RULES: &[Rule] = &[
    Rule {
        role: Role::Mirror,
        prefix: ".agents/skills/",
        needs_claude_skills: true,
        ..ANY
    },
    Rule {
        role: Role::Generated,
        names: &[
            "package-lock.json",
            "pnpm-lock.yaml",
            "Cargo.lock",
            "yarn.lock",
            "poetry.lock",
            "uv.lock",
        ],
        ..ANY
    },
    Rule {
        role: Role::Generated,
        prefix: ".claude/skills/",
        names: &["commands.md", "tools.md", "tool-map.md", "command-map.md"],
        ..ANY
    },
    Rule {
        role: Role::Generated,
        prefix: "licenses/",
        suffix: ".html",
        ..ANY
    },
    Rule {
        role: Role::Generated,
        suffix: ".min.js",
        ..ANY
    },
    Rule {
        role: Role::Generated,
        suffix: ".min.css",
        ..ANY
    },
    Rule {
        role: Role::Vendored,
        segment: "third_party",
        ..ANY
    },
    Rule {
        role: Role::Vendored,
        segment: "vendor",
        ..ANY
    },
    Rule {
        role: Role::Vendored,
        segment: "node_modules",
        ..ANY
    },
];

const CONTENT_RULES: &[Rule] = &[
    Rule {
        role: Role::Log,
        names: &["CHANGELOG.md", "HASHOUT_LOG.md"],
        ..ANY
    },
    Rule {
        role: Role::Data,
        segment: "i18n",
        suffix: ".ts",
        not_names: &["index.ts", "loader.ts"],
        ..ANY
    },
    Rule {
        role: Role::Data,
        segment: "data",
        suffix: ".json",
        ..ANY
    },
    Rule {
        role: Role::Doc,
        segment: "docs",
        ..ANY
    },
    Rule {
        role: Role::Doc,
        segment: ".docs",
        ..ANY
    },
    Rule {
        role: Role::Doc,
        suffix: ".md",
        ..ANY
    },
    Rule {
        role: Role::Doc,
        suffix: ".mdx",
        ..ANY
    },
    Rule {
        role: Role::Doc,
        suffix: ".txt",
        ..ANY
    },
];

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn extension(path: &str) -> Option<String> {
    let name = file_name(path);
    let dot = name.rfind('.')?;
    if dot == 0 || dot + 1 == name.len() {
        return None;
    }
    Some(name[dot + 1..].to_ascii_lowercase())
}

fn has_segment(path: &str, segment: &str) -> bool {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop();
    parts.iter().any(|p| *p == segment)
}

fn matches(rule: &Rule, path: &str, ctx: &Context) -> bool {
    if rule.needs_claude_skills && !ctx.has_claude_skills {
        return false;
    }
    let name = file_name(path);
    if !rule.prefix.is_empty() && !path.starts_with(rule.prefix) {
        return false;
    }
    if !rule.segment.is_empty() && !has_segment(path, rule.segment) {
        return false;
    }
    if !rule.suffix.is_empty() && !name.ends_with(rule.suffix) {
        return false;
    }
    if !rule.names.is_empty() && !rule.names.contains(&name) {
        return false;
    }
    if rule.not_names.contains(&name) {
        return false;
    }
    true
}

fn first_match(rules: &[Rule], path: &str, ctx: &Context) -> Option<Role> {
    rules.iter().find(|r| matches(r, path, ctx)).map(|r| r.role)
}

pub fn language(path: &str) -> &'static str {
    let Some(ext) = extension(path) else {
        return "other";
    };
    LANGUAGES
        .iter()
        .find(|(_, exts)| exts.contains(&ext.as_str()))
        .map(|(name, _)| *name)
        .unwrap_or("other")
}

pub fn binary_extension(path: &str) -> bool {
    match extension(path) {
        Some(ext) => BINARY_EXTENSIONS.contains(&ext.as_str()),
        None => false,
    }
}

pub fn sniff_binary(bytes: &[u8]) -> bool {
    let window = &bytes[..bytes.len().min(8000)];
    window.contains(&0)
}

pub fn skip_read(path: &str, ctx: &Context) -> bool {
    match first_match(SKIP_RULES, path, ctx) {
        Some(Role::Generated) | Some(Role::Mirror) => true,
        _ => binary_extension(path),
    }
}

pub fn role(path: &str, ctx: &Context, is_binary: bool) -> Role {
    if let Some(found) = first_match(SKIP_RULES, path, ctx) {
        return found;
    }
    if is_binary || binary_extension(path) {
        return Role::Binary;
    }
    first_match(CONTENT_RULES, path, ctx).unwrap_or(Role::Source)
}
