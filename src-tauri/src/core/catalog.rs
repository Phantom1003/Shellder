//! The user's ssh configuration: the Host blocks, read straight from
//! ~/.ssh/config (following Include), and what `ssh -G` resolves them to.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::config;

/// One `Host` block from the user's ssh configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostEntry {
    /// First non-pattern name of the Host line.
    pub alias: String,
    /// Remaining names on the same line.
    pub aliases: Vec<String>,
    /// File the block was read from.
    pub source: String,
}

/// What `ssh -G alias` resolves to for a host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedHost {
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub proxy_jump: Option<String>,
    pub proxy_command: Option<String>,
    pub control_path: Option<String>,
    pub control_master: String,
    pub control_persist: String,
    pub identity_files: Vec<String>,
    pub server_alive_interval: u32,
    pub strict_host_key_checking: String,
    /// Alias in the catalog that the first ProxyJump hop refers to, if any.
    pub jump_alias: Option<String>,
}

impl ResolvedHost {
    pub fn from_config(cfg: &HashMap<String, Vec<String>>) -> Self {
        let first = |k: &str| cfg.get(k).and_then(|v| v.first()).cloned();
        let not_none = |v: Option<String>| v.filter(|s| s.to_lowercase() != "none");
        ResolvedHost {
            hostname: first("hostname").unwrap_or_default(),
            user: first("user").unwrap_or_default(),
            port: first("port").unwrap_or_else(|| "22".into()),
            proxy_jump: not_none(first("proxyjump")),
            proxy_command: not_none(first("proxycommand")),
            control_path: not_none(first("controlpath")),
            control_master: first("controlmaster").unwrap_or_default(),
            control_persist: first("controlpersist").unwrap_or_default(),
            identity_files: cfg.get("identityfile").cloned().unwrap_or_default(),
            server_alive_interval: first("serveraliveinterval")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            strict_host_key_checking: first("stricthostkeychecking").unwrap_or_default(),
            jump_alias: None,
        }
    }

    /// First hop of ProxyJump with user@ and :port stripped.
    pub fn first_jump_host(&self) -> Option<String> {
        let pj = self.proxy_jump.as_ref()?;
        let mut hop = pj.split(',').next()?.to_string();
        if let Some(at) = hop.rfind('@') {
            hop = hop[at + 1..].to_string();
        }
        if let Some(rest) = hop.strip_prefix('[') {
            hop = rest.split(']').next().unwrap_or(rest).to_string();
        } else if let Some(colon) = hop.rfind(':') {
            hop.truncate(colon);
        }
        if hop.is_empty() {
            None
        } else {
            Some(hop)
        }
    }

    pub fn identity_files_expanded(&self) -> Vec<String> {
        self.identity_files
            .iter()
            .map(|f| config::expand_tilde(f))
            .collect()
    }
}

#[derive(Debug, Default, Clone)]
pub struct Catalog {
    pub entries: Vec<HostEntry>,
    pub files: Vec<String>,
}

/// Reads Host aliases straight from ~/.ssh/config (following Include).
/// Patterns (`*`, `?`, `!`) and Match blocks are skipped: they are not
/// something one can open a master connection to.
pub fn load() -> Catalog {
    let mut r = Catalog::default();
    let mut seen = HashSet::new();
    parse(&config::get().ssh_config_file, 0, &mut r, &mut seen);
    r
}

pub fn modification_dates(files: &[String]) -> HashMap<String, Option<SystemTime>> {
    files
        .iter()
        .map(|f| {
            (
                f.clone(),
                std::fs::metadata(f).ok().and_then(|m| m.modified().ok()),
            )
        })
        .collect()
}

fn parse(file: &Path, depth: usize, r: &mut Catalog, seen: &mut HashSet<String>) {
    let name = config::path_str(file);
    if depth >= 8 || r.files.contains(&name) {
        return;
    }
    r.files.push(name.clone());
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = split_key_value(line);
        match key.to_lowercase().as_str() {
            "host" => {
                let names: Vec<String> = tokens(value)
                    .into_iter()
                    .filter(|n| !n.contains('*') && !n.contains('?') && !n.starts_with('!'))
                    .collect();
                let Some(primary) = names.first() else {
                    continue;
                };
                if !seen.insert(primary.clone()) {
                    continue;
                }
                r.entries.push(HostEntry {
                    alias: primary.clone(),
                    aliases: names[1..].to_vec(),
                    source: name.clone(),
                });
            }
            "include" => {
                for pat in tokens(value) {
                    for f in expand(&pat) {
                        parse(&f, depth + 1, r, seen);
                    }
                }
            }
            _ => {}
        }
    }
}

/// "Key value", "Key=value" and "Key = value" are all valid ssh_config.
fn split_key_value(line: &str) -> (&str, &str) {
    let Some(idx) = line.find([' ', '\t', '=']) else {
        return (line, "");
    };
    let key = &line[..idx];
    let mut rest = line[idx..].trim();
    if let Some(r) = rest.strip_prefix('=') {
        rest = r.trim();
    }
    (key, rest)
}

/// Whitespace-separated tokens with support for double-quoted strings.
fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut has = false;
    for c in s.chars() {
        if c == '"' {
            quoted = !quoted;
            has = true;
            continue;
        }
        if !quoted && (c == ' ' || c == '\t') {
            if has {
                out.push(std::mem::take(&mut cur));
                has = false;
            }
            continue;
        }
        cur.push(c);
        has = true;
    }
    if has {
        out.push(cur);
    }
    out
}

/// Include paths: ~ expansion, relative to ~/.ssh, shell globs.
fn expand(pattern: &str) -> Vec<PathBuf> {
    let mut p = config::expand_tilde(pattern);
    if !p.starts_with('/') {
        p = format!("{}/{}", config::path_str(&config::get().ssh_dir), p);
    }
    glob(&p)
}

#[cfg(target_os = "macos")]
const GLOB_FLAGS: libc::c_int = 0x0080 | 0x0800; // GLOB_BRACE | GLOB_TILDE
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const GLOB_FLAGS: libc::c_int = libc::GLOB_BRACE | libc::GLOB_TILDE;
#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
const GLOB_FLAGS: libc::c_int = 0;

#[cfg(unix)]
fn glob(pattern: &str) -> Vec<PathBuf> {
    use std::ffi::{CStr, CString};
    let Ok(c) = CString::new(pattern) else {
        return vec![];
    };
    let mut g: libc::glob_t = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::glob(c.as_ptr(), GLOB_FLAGS, None, &mut g) };
    let mut out = Vec::new();
    if rc == 0 {
        for i in 0..g.gl_pathc as usize {
            let ptr = unsafe { *g.gl_pathv.add(i) };
            if !ptr.is_null() {
                out.push(PathBuf::from(
                    unsafe { CStr::from_ptr(ptr) }
                        .to_string_lossy()
                        .into_owned(),
                ));
            }
        }
    }
    unsafe { libc::globfree(&mut g) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_value_forms() {
        assert_eq!(split_key_value("Host a b"), ("Host", "a b"));
        assert_eq!(split_key_value("Host=a"), ("Host", "a"));
        assert_eq!(split_key_value("Host = a"), ("Host", "a"));
    }

    #[test]
    fn quoted_tokens() {
        assert_eq!(tokens("a \"b c\" d"), vec!["a", "b c", "d"]);
    }

    #[test]
    fn jump_host_forms() {
        let mut r = ResolvedHost {
            proxy_jump: Some("alice@jump:2222,other".into()),
            ..Default::default()
        };
        assert_eq!(r.first_jump_host().as_deref(), Some("jump"));
        r.proxy_jump = Some("[::1]:22".into());
        assert_eq!(r.first_jump_host().as_deref(), Some("::1"));
    }
}
