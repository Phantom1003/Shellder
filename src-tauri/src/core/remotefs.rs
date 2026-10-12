//! Directories on a host, read with `ls` through the master when one is
//! up and through the host's session otherwise, and the same questions
//! about this machine's own filesystem.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::config;
use super::session;
use super::ssh::{self, FileSource};

/// Why a directory could not be read, in the words the window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError(pub String);

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One entry of a directory listing, on a host or on this machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub name: String,
    /// Absolute path in its own filesystem.
    pub path: String,
    pub is_dir: bool,
    /// Bytes for a file, meaningless for a directory.
    pub size: i64,
    /// Unix time, with fraction, when its content last changed. None when
    /// the side could not say: a link to nowhere, or a host whose ls only
    /// gives the day.
    pub modified: Option<f64>,
}

/// One filesystem, whichever side it is on: the calls a pane makes.
pub mod fs {
    use super::*;

    pub fn home(source: &FileSource) -> Result<String, FsError> {
        match source.host() {
            None => Ok(config::home().to_string()),
            Some(h) => remote::home(h),
        }
    }

    pub fn list(source: &FileSource, path: &str) -> Result<Vec<FileItem>, FsError> {
        match source.host() {
            None => local::list(path),
            Some(h) => remote::list(h, path),
        }
    }

    pub fn is_directory(source: &FileSource, path: &str) -> bool {
        match source.host() {
            None => std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false),
            Some(h) => remote::is_directory(h, path),
        }
    }

    /// Bytes a copy of this path has to move, or — pointed at the copy —
    /// how much of it has arrived.
    pub fn bytes(source: &FileSource, path: &str) -> Option<i64> {
        match source.host() {
            None => Some(local::bytes(path)),
            Some(h) => remote::bytes(h, path),
        }
    }

    /// Is something there already? What a copy would write over.
    pub fn exists(source: &FileSource, path: &str) -> bool {
        match source.host() {
            None => std::path::Path::new(path).exists(),
            Some(h) => remote::exists(h, path),
        }
    }

    pub fn make_directory(source: &FileSource, path: &str) -> Result<(), FsError> {
        match source.host() {
            None => local::make_directory(path),
            Some(h) => remote::make_directory(h, path),
        }
    }

    pub fn make_file(source: &FileSource, path: &str) -> Result<(), FsError> {
        match source.host() {
            None => local::make_file(path),
            Some(h) => remote::make_file(h, path),
        }
    }

    /// On this machine into the Trash, on a host gone for good — which is
    /// why the window words its question differently for each.
    pub fn remove(source: &FileSource, path: &str) -> Result<(), FsError> {
        match source.host() {
            None => local::trash(path),
            Some(h) => remote::remove(h, path),
        }
    }

    /// The first `limit` bytes of a file, and whether it goes on past
    /// them: what the Console's preview tab shows.
    pub fn read(source: &FileSource, path: &str, limit: usize) -> Result<(Vec<u8>, bool), FsError> {
        match source.host() {
            None => local::read(path, limit),
            Some(h) => remote::read(h, path, limit),
        }
    }
}

/// The remote shell gets the command as one word list: single-quote the
/// path, splicing in the single quote itself.
pub fn quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The directory above `path`, or None at the root.
pub fn parent(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => Some("/".into()),
        Some(i) => Some(trimmed[..i].to_string()),
    }
}

pub fn last_component(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

pub mod remote {
    use super::*;

    /// Whether the host's commands go through its master, one ssh per
    /// command and no login, or through its session, one login kept open:
    /// the master whenever it is up, the session otherwise, and on a
    /// platform without ControlMaster always.
    pub fn through_master(host: &str) -> bool {
        ssh::master_reusable(host)
    }

    /// Run one command on the host, through its master or its session:
    /// what the listings use, and what the Console window reads through.
    pub fn shell(host: &str, command: &str) -> ssh::Output {
        if through_master(host) {
            return ssh::run(&["-o", "BatchMode=yes", "-T", host, command]);
        }
        match session::for_host(host) {
            Ok(s) => s.run(command),
            Err(e) => ssh::Output {
                status: -1,
                stdout: String::new(),
                stderr: e,
            },
        }
    }

    fn failure(r: &ssh::Output) -> FsError {
        let text = r.text();
        FsError(if text.is_empty() {
            format!("ssh exited with status {}", r.status)
        } else {
            text
        })
    }

    /// The home directory, where a tree starts.
    pub fn home(host: &str) -> Result<String, FsError> {
        let r = shell(host, "cd && pwd");
        let path = r.stdout.trim().to_string();
        if r.status != 0 || !path.starts_with('/') {
            return Err(failure(&r));
        }
        Ok(path)
    }

    pub fn is_directory(host: &str, path: &str) -> bool {
        shell(host, &format!("test -d {}", quote(path))).status == 0
    }

    pub fn exists(host: &str, path: &str) -> bool {
        shell(host, &format!("test -e {}", quote(path))).status == 0
    }

    /// What `path` contains, in the order `ls` gives it: the pane sorts.
    /// `ls -L` follows symlinks, so a link to a directory opens like one. A
    /// link to nowhere keeps a line of its own (GNU) or is left out
    /// (BusyBox, the BSDs), and `ls` exits non-zero, which is only an error
    /// when nothing was read.
    pub fn list(host: &str, path: &str) -> Result<Vec<FileItem>, FsError> {
        let r = shell(host, &list_command(path));
        let items = parse(&r.stdout, path, now());
        if items.is_empty() && r.status != 0 {
            return Err(failure(&r));
        }
        Ok(items)
    }

    /// The listing, with times to the second in whichever way this host's
    /// ls knows: --full-time (GNU, BusyBox), -T (macOS, the BSDs), or
    /// failing both the plain form. In UTC, so a time that carries no zone
    /// reads the same here. Owners as numbers (-n): a group name with a
    /// blank in it would shift every field after it. Run by sh, whatever
    /// the login shell is.
    pub fn list_command(path: &str) -> String {
        let script = format!(
            "if ls --full-time -d / >/dev/null 2>&1; then t=--full-time; \
             elif ls -T -d / >/dev/null 2>&1; then t=-T; else t=; fi; \
             LC_ALL=C TZ=UTC0 ls -nAL $t -- {}",
            quote(path)
        );
        format!("sh -c {}", quote(&script))
    }

    /// Make a directory. Fails when something is there already, which is
    /// mkdir's own behaviour, and the message says so.
    pub fn make_directory(host: &str, path: &str) -> Result<(), FsError> {
        let r = shell(host, &format!("mkdir -- {}", quote(path)));
        if r.status == 0 {
            Ok(())
        } else {
            Err(failure(&r))
        }
    }

    /// Make an empty file. `set -C` is the shell's own refusal to write over
    /// something that is there already.
    pub fn make_file(host: &str, path: &str) -> Result<(), FsError> {
        let r = shell(host, &format!("set -C; : > {}", quote(path)));
        if r.status == 0 {
            Ok(())
        } else {
            Err(failure(&r))
        }
    }

    /// Delete a path and, when it is a directory, what is under it. There
    /// is no undoing this on a host, so the window asks first.
    pub fn remove(host: &str, path: &str) -> Result<(), FsError> {
        let r = shell(host, &format!("rm -rf -- {}", quote(path)));
        if r.status == 0 {
            Ok(())
        } else {
            Err(failure(&r))
        }
    }

    /// Bytes in the regular files under `path` (the path itself when it is
    /// a file). find/ls/awk are on every server; a path that is not there
    /// yet counts as zero, not as an error.
    pub fn bytes(host: &str, path: &str) -> Option<i64> {
        let r = shell(host, &format!("find {} -type f -exec ls -ln -- {{}} + 2>/dev/null | awk '{{s+=$5}} END {{print s+0}}'", quote(path)));
        if r.status != 0 {
            return None;
        }
        r.stdout.trim().parse().ok()
    }

    /// The first `limit` bytes of a file, through base64: the master's
    /// ssh and the session both bring output back as text. One byte more
    /// is asked for, which says whether the file goes on.
    pub fn read(host: &str, path: &str, limit: usize) -> Result<(Vec<u8>, bool), FsError> {
        let r = shell(host, &read_command(path, limit + 1));
        if r.status != 0 {
            return Err(failure(&r));
        }
        let mut data = decode_base64(&r.stdout).ok_or_else(|| {
            FsError(format!(
                "{} did not come back as base64: {}",
                last_component(path),
                r.text().lines().next().unwrap_or_default()
            ))
        })?;
        let more = data.len() > limit;
        data.truncate(limit);
        Ok((data, more))
    }

    /// The script that reads `bytes` of a file: a missing file, a
    /// directory and one this account may not read are refused in words
    /// first (a pipeline would keep only base64's status), then `head -c`
    /// into `base64`, or `openssl base64` on a host without it. Run by sh,
    /// whatever the login shell is.
    pub fn read_command(path: &str, bytes: usize) -> String {
        let script = format!(
            "f={}; \
             if ! test -e \"$f\"; then echo \"$f: no such file\" >&2; exit 1; fi; \
             if test -d \"$f\"; then echo \"$f is a directory\" >&2; exit 1; fi; \
             if ! test -r \"$f\"; then echo \"$f: permission denied\" >&2; exit 1; fi; \
             if command -v base64 >/dev/null 2>&1; then head -c {bytes} -- \"$f\" | base64; \
             else head -c {bytes} -- \"$f\" | openssl base64; fi",
            quote(path)
        );
        format!("sh -c {}", quote(&script))
    }
}

/// The bytes of a base64 text as a host's `base64` writes it, in lines of
/// 64 or 76 characters, or in one.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .ok()
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The blank-separated fields of one `ls -l` line, read from the left.
/// What is left after the last one read is the name, blanks and all.
struct LsFields<'a> {
    line: &'a str,
    at: usize,
}

impl<'a> LsFields<'a> {
    fn new(line: &'a str) -> Self {
        LsFields { line, at: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let bytes = self.line.as_bytes();
        while self.at < bytes.len() && bytes[self.at] == b' ' {
            self.at += 1;
        }
        if self.at >= bytes.len() {
            return None;
        }
        let start = self.at;
        while self.at < bytes.len() && bytes[self.at] != b' ' {
            self.at += 1;
        }
        Some(&self.line[start..self.at])
    }

    /// The rest of the line after the one blank that ends the last field.
    fn rest(&self) -> &'a str {
        if self.at < self.line.len() {
            &self.line[self.at + 1..]
        } else {
            ""
        }
    }
}

/// `ls -nL` output into entries. A line is the mode, the link count, the
/// owner, the group, the size (a device's "major, minor"), the time in one
/// of the forms `ls_time` reads, then one blank and the name, which may
/// itself start with a blank.
pub fn parse(text: &str, dir: &str, now: f64) -> Vec<FileItem> {
    let mut out = Vec::new();
    for line in text.split('\n').filter(|l| !l.is_empty()) {
        let mut fields = LsFields::new(line);
        let Some(mode) = fields.next() else { continue };
        let Some(kind) = mode.chars().next() else {
            continue;
        };
        if !"-dlbcsp".contains(kind) {
            continue;
        }
        if fields.next().is_none() || fields.next().is_none() || fields.next().is_none() {
            continue;
        }
        let Some(mut size) = fields.next() else {
            continue;
        };
        if size.ends_with(',') {
            if fields.next().is_none() {
                continue;
            }
            size = "0";
        }
        let Some(time) = ls_time(&mut fields, now) else {
            continue;
        };
        let mut name = fields.rest().to_string();
        if kind == 'l' {
            if let Some(arrow) = name.find(" -> ") {
                name.truncate(arrow);
            }
        }
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        out.push(FileItem {
            path: join(dir, &name),
            name,
            is_dir: kind == 'd',
            size: size.parse().unwrap_or(0),
            modified: time,
        });
    }
    out
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The time on an `ls -l` line, in each of the forms ls writes it:
///
/// ```text
/// 2024-01-02 03:04:05.000000000 +0000   --full-time (GNU, BusyBox)
/// Jan  2 03:04:05 2024                  -T (macOS, the BSDs)
/// Jan  2 03:04    Jan  2  2024          plain: to the minute within
///                                       six months, the day before
/// ?                                     GNU, for a link to nowhere
/// ```
///
/// Returns Some(None) for a time that is there but not one to sort by (the
/// "?", or a day alone), None when the fields are not a time at all: the
/// name cannot be found on a line whose time is not understood.
fn ls_time(fields: &mut LsFields, now: f64) -> Option<Option<f64>> {
    let first = fields.next()?;
    if first == "?" {
        return Some(None);
    }
    let day: Vec<&str> = first.split('-').collect();
    if day.len() == 3 {
        let clock = fields.next()?;
        let zone = fields.next()?;
        let offset = zone_offset(zone)?;
        let t = utc_date(
            day[0].parse().ok()?,
            day[1].parse().ok()?,
            day[2].parse().ok()?,
            clock,
        )?;
        return Some(Some(t - offset));
    }
    let month = MONTHS.iter().position(|m| *m == first)? as u32 + 1;
    let day_of_month: u32 = fields.next()?.parse().ok()?;
    let third = fields.next()?;
    match third.split(':').count() {
        3 => {
            let year: i32 = fields.next()?.parse().ok()?;
            Some(Some(utc_date(year, month, day_of_month, third)?))
        }
        2 => {
            // No year: ls leaves it out within six months of its own clock,
            // so a date ahead of this one is last year's.
            let year = chrono::DateTime::from_timestamp(now as i64, 0)?
                .format("%Y")
                .to_string()
                .parse::<i32>()
                .ok()?;
            let date = utc_date(year, month, day_of_month, third)?;
            if date > now + 86400.0 {
                return Some(utc_date(year - 1, month, day_of_month, third));
            }
            Some(Some(date))
        }
        _ => {
            if third.parse::<i32>().is_ok() {
                Some(None)
            } else {
                None
            }
        }
    }
}

/// A UTC time from its day and a "03:04" or "03:04:05.5" clock.
fn utc_date(year: i32, month: u32, day: u32, clock: &str) -> Option<f64> {
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = parts[1].parse().ok()?;
    let seconds: f64 = if parts.len() > 2 {
        parts[2].parse().ok()?
    } else {
        0.0
    };
    let whole = chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(
        hour,
        minute,
        seconds.floor() as u32,
    )?;
    Some(whole.and_utc().timestamp() as f64 + (seconds - seconds.floor()))
}

/// "+0800" in seconds.
fn zone_offset(zone: &str) -> Option<f64> {
    if zone.len() != 5 {
        return None;
    }
    let sign = zone.chars().next()?;
    if sign != '+' && sign != '-' {
        return None;
    }
    let hhmm: i64 = zone[1..].parse().ok()?;
    let seconds = (hhmm / 100 * 3600 + hhmm % 100 * 60) as f64;
    Some(if sign == '-' { -seconds } else { seconds })
}

/// The same questions about this machine's own filesystem.
pub mod local {
    use super::*;

    pub fn list(path: &str) -> Result<Vec<FileItem>, FsError> {
        let entries = std::fs::read_dir(path).map_err(|e| FsError(e.to_string()))?;
        let mut out = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // Under the directory as it was asked for, not as the OS spells
            // it: /tmp and /private/tmp are the same place.
            out.push(item(join(path, &name)));
        }
        Ok(out)
    }

    fn mtime(m: &std::fs::Metadata) -> Option<f64> {
        m.modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs_f64())
    }

    /// Symlinks are followed, the way `ls -L` follows them on a host: a
    /// link to a directory opens like one. A link to nowhere stays a leaf,
    /// dated by the link itself.
    fn item(path: String) -> FileItem {
        let name = last_component(&path);
        match std::fs::metadata(&path) {
            Ok(m) => FileItem {
                name,
                is_dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.len() as i64 },
                modified: mtime(&m),
                path,
            },
            Err(_) => {
                let own = std::fs::symlink_metadata(&path)
                    .ok()
                    .and_then(|m| mtime(&m));
                FileItem {
                    name,
                    path,
                    is_dir: false,
                    size: 0,
                    modified: own,
                }
            }
        }
    }

    pub fn make_directory(path: &str) -> Result<(), FsError> {
        std::fs::create_dir(path).map_err(|e| FsError(e.to_string()))
    }

    /// An empty file. Refuses a name that is taken.
    pub fn make_file(path: &str) -> Result<(), FsError> {
        let name = last_component(path);
        if std::path::Path::new(path).exists() {
            return Err(FsError(format!("{name} is already there")));
        }
        std::fs::File::create(path)
            .map(|_| ())
            .map_err(|_| FsError(format!("could not create {name}")))
    }

    /// Into the Trash, not gone: deleting on this machine is undoable the
    /// way it is everywhere else on it.
    pub fn trash(path: &str) -> Result<(), FsError> {
        trash::delete(path).map_err(|e| FsError(e.to_string()))
    }

    /// Bytes in the files under `path`, the counterpart of `remote::bytes`.
    pub fn bytes(path: &str) -> i64 {
        let Ok(m) = std::fs::metadata(path) else {
            return 0;
        };
        if !m.is_dir() {
            return m.len() as i64;
        }
        fn walk(dir: &std::path::Path, total: &mut i64) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let Ok(m) = e.metadata() else { continue };
                if m.is_dir() {
                    walk(&e.path(), total);
                } else if m.is_file() {
                    *total += m.len() as i64;
                }
            }
        }
        let mut total = 0;
        walk(std::path::Path::new(path), &mut total);
        total
    }

    /// The first `limit` bytes of a file and whether it goes on, the
    /// counterpart of `remote::read`.
    pub fn read(path: &str, limit: usize) -> Result<(Vec<u8>, bool), FsError> {
        use std::io::Read;
        let f = std::fs::File::open(path).map_err(|e| FsError(e.to_string()))?;
        if f.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            return Err(FsError(format!("{} is a directory", last_component(path))));
        }
        let mut data = Vec::new();
        f.take(limit as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| FsError(e.to_string()))?;
        let more = data.len() > limit;
        data.truncate(limit);
        Ok((data, more))
    }
}

#[allow(dead_code)]
fn _unused(_: Duration) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> f64 {
        chrono::DateTime::parse_from_rfc3339(s).unwrap().timestamp() as f64
    }

    /// What a host's base64 writes, wrapped or not, comes back as the
    /// bytes; the script reads one path, quoted, whatever is in its name.
    #[test]
    fn reading_through_base64() {
        assert_eq!(decode_base64("aGVs\nbG8=\n").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("").unwrap(), b"");
        assert!(decode_base64("not base64!").is_none());
        let cmd = remote::read_command("/tmp/it's here", 7);
        assert!(cmd.starts_with("sh -c '"), "{cmd}");
        assert!(
            cmd.contains("f='\\''/tmp/it'\\''\\'\\'''\\''s here'\\''"),
            "{cmd}"
        );
        assert!(cmd.contains("head -c 7 -- \"$f\" | base64"), "{cmd}");
        let dir = std::env::temp_dir().join(format!("shjt-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"0123456789").unwrap();
        let path = file.to_str().unwrap();
        assert_eq!(local::read(path, 4).unwrap(), (b"0123".to_vec(), true));
        assert_eq!(
            local::read(path, 10).unwrap(),
            (b"0123456789".to_vec(), false)
        );
        assert!(local::read(dir.to_str().unwrap(), 4)
            .unwrap_err()
            .0
            .ends_with("is a directory"));
        assert!(local::read(&format!("{path}.missing"), 4).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quoting_and_paths() {
        assert_eq!(quote("a'b"), "'a'\\''b'");
        assert_eq!(join("/", "x"), "/x");
        assert_eq!(join("/a", "x"), "/a/x");
        assert_eq!(parent("/a/b").as_deref(), Some("/a"));
        assert_eq!(parent("/a").as_deref(), Some("/"));
        assert_eq!(parent("/"), None);
        assert_eq!(last_component("/a/b c"), "b c");
    }

    /// Every form of line the parser knows, on a fixed clock.
    #[test]
    fn every_form_of_line() {
        let text = "total 8\n\
l????????? ? ? ?    ?                                   ? broken\n\
crw-rw-rw- 1 0 0 1, 3 2026-09-24 08:48:49.25 +0000 null\n\
-rw-r--r-- 1 0 0    4 2026-09-24 16:48:49.5 +0800  lead\n\
-rw-r--r--@ 1 501 20 4 Jan  2 03:04:05 2024 bsd file\n\
-rw-r--r-- 1 0 0    4 Sep 20 10:31 recent\n\
-rw-r--r-- 1 0 0    4 Dec 31 23:59 last year\n\
-rw-r--r-- 1 0 0    4 Jan  2  2024 old\n\
drwxr-xr-x 2 0 0 4096 2026-01-01 00:00:00 -0130 dir\n\
lrwxrwxrwx 1 0 0 7 2026-01-01 00:00:00 +0000 link -> target\n";
        let parsed = parse(text, "/x", utc("2026-09-24T12:00:00Z"));
        let names: Vec<&str> = parsed.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "broken",
                "null",
                " lead",
                "bsd file",
                "recent",
                "last year",
                "old",
                "dir",
                "link"
            ]
        );
        let by = |n: &str| parsed.iter().find(|i| i.name == n).unwrap();
        assert_eq!(by("broken").modified, None);
        assert_eq!(by("null").size, 0);
        assert_eq!(
            by("null").modified,
            Some(utc("2026-09-24T08:48:49Z") + 0.25)
        );
        assert_eq!(
            by(" lead").modified,
            Some(utc("2026-09-24T08:48:49Z") + 0.5)
        );
        assert_eq!(by(" lead").path, "/x/ lead");
        assert_eq!(by("bsd file").modified, Some(utc("2024-01-02T03:04:05Z")));
        assert_eq!(by("recent").modified, Some(utc("2026-09-20T10:31:00Z")));
        assert_eq!(by("last year").modified, Some(utc("2025-12-31T23:59:00Z")));
        assert_eq!(by("old").modified, None);
        assert!(by("dir").is_dir);
        assert_eq!(by("dir").modified, Some(utc("2026-01-01T01:30:00Z")));
        assert!(!by("link").is_dir);
    }

    #[test]
    fn list_command_is_one_word_for_the_remote_shell() {
        let c = list_command_for_test("/a b");
        assert!(c.starts_with("sh -c '"));
        assert!(c.contains("ls -nAL $t -- '\\''/a b'\\'''"));
    }

    fn list_command_for_test(p: &str) -> String {
        remote::list_command(p)
    }
}
