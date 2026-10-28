//! "Start at login": a per-user LaunchAgent on macOS, an XDG autostart
//! entry on Linux. The entry starts the app with `--login`, so that the app
//! can tell a login-item launch from a double click: only the former
//! honours the "start silently" setting.

use std::path::PathBuf;

use super::config::{self, LABEL};
use super::log;

/// The flag the entry passes; the app strips it before parsing commands.
pub const LOGIN_FLAG: &str = "--login";

pub fn entry_file() -> PathBuf {
    let home = &config::get().home;
    if cfg!(target_os = "macos") {
        home.join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("autostart")
            .join(format!("{LABEL}.desktop"))
    }
}

pub fn installed() -> bool {
    entry_file().exists()
}

/// The executable path the installed entry points at, if any.
pub fn installed_path() -> Option<String> {
    let text = std::fs::read_to_string(entry_file()).ok()?;
    if cfg!(target_os = "macos") {
        plist_program(&text)
    } else {
        desktop_program(&text)
    }
}

/// The first <string> after ProgramArguments.
fn plist_program(text: &str) -> Option<String> {
    let after = text.split("<key>ProgramArguments</key>").nth(1)?;
    let s = after.split("<string>").nth(1)?;
    Some(xml_unescape(s.split("</string>").next()?))
}

/// The program of a desktop entry's Exec key, in the [Desktop Entry]
/// group. The value is read as the Desktop Entry Specification has it:
/// first the escapes of every string value (`\s`, `\n`, `\t`, `\r`,
/// `\\`), then the command line's own quoting, where an argument with a
/// space in it is in double quotes and a `"`, `` ` ``, `$` or `\` inside
/// them has a backslash before it.
fn desktop_program(text: &str) -> Option<String> {
    let mut group = "";
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            group = name;
            continue;
        }
        if group != "Desktop Entry" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "Exec" {
            return exec_arguments(&unescape_value(value.trim()))
                .into_iter()
                .next();
        }
    }
    None
}

/// A desktop entry string value with its escapes undone.
fn unescape_value(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // Not an escape of the value: kept for the command line's
            // quoting to read.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The arguments of an Exec command line: split at unquoted spaces, a
/// double-quoted stretch one argument with its backslash escapes undone,
/// `%%` a percent sign.
fn exec_arguments(s: &str) -> Vec<String> {
    let mut args = vec![];
    let mut arg = String::new();
    let mut in_arg = false;
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                in_arg = true;
            }
            '\\' if quoted => match chars.next() {
                Some(e @ ('"' | '`' | '$' | '\\')) => arg.push(e),
                Some(other) => {
                    arg.push('\\');
                    arg.push(other);
                }
                None => arg.push('\\'),
            },
            '%' if chars.peek() == Some(&'%') => {
                chars.next();
                arg.push('%');
                in_arg = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_arg {
                    args.push(std::mem::take(&mut arg));
                    in_arg = false;
                }
            }
            c => {
                arg.push(c);
                in_arg = true;
            }
        }
    }
    if in_arg {
        args.push(arg);
    }
    args
}

/// An Exec argument as the specification wants it written: in double
/// quotes, with `"`, `` ` ``, `$` and `\` escaped inside them and `%`
/// doubled, then the value's own escape for the backslashes that makes.
fn exec_quote(arg: &str) -> String {
    let mut q = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                q.push('\\');
                q.push(c);
            }
            '%' => q.push_str("%%"),
            c => q.push(c),
        }
    }
    q.push('"');
    q.replace('\\', "\\\\")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(target_os = "macos")]
fn launchctl(args: &[String]) -> i32 {
    std::process::Command::new("/bin/launchctl")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.code().unwrap_or(-1))
        .unwrap_or(-1)
}

#[cfg(target_os = "macos")]
fn domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

/// Write the entry. `bootstrap` also starts it right away — only do that
/// when no instance of the app is running.
pub fn install(bootstrap: bool) -> Result<(), String> {
    let c = config::get();
    let file = entry_file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let exe = config::path_str(&c.self_path);
    let text = if cfg!(target_os = "macos") {
        // KeepAlive only restarts the app after a crash (SuccessfulExit =
        // false), so Quit from the menu sticks. AssociatedBundleIdentifiers
        // makes System Settings > Login Items show the app's name and icon.
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{exe}</string>
		<string>{flag}</string>
	</array>
	<key>AssociatedBundleIdentifiers</key>
	<array>
		<string>{label}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>{log}</string>
	<key>StandardErrorPath</key>
	<string>{log}</string>
	<key>EnvironmentVariables</key>
	<dict>
		<key>PATH</key>
		<string>/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin</string>
		<key>HOME</key>
		<string>{home}</string>
	</dict>
</dict>
</plist>
"#,
            label = LABEL,
            exe = xml_escape(&exe),
            flag = LOGIN_FLAG,
            log = xml_escape(&config::path_str(&c.log_file)),
            home = xml_escape(config::home()),
        )
    } else {
        format!(
            "[Desktop Entry]\nType=Application\nName=Shellder\nComment=Keeps ssh ControlMaster connections alive\nExec={} {}\nX-GNOME-Autostart-enabled=true\n",
            exec_quote(&exe),
            LOGIN_FLAG
        )
    };
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    log::info(format!(
        "login item written to {} -> {exe}",
        config::path_str(&file)
    ));
    #[cfg(target_os = "macos")]
    if bootstrap {
        launchctl(&["bootout".into(), format!("{}/{}", domain(), LABEL)]);
        let rc = launchctl(&["bootstrap".into(), domain(), config::path_str(&file)]);
        if rc != 0 {
            return Err(format!("launchctl bootstrap failed (rc {rc})"));
        }
    }
    #[cfg(not(target_os = "macos"))]
    if bootstrap {
        let _ = std::process::Command::new(&exe).arg("--background").spawn();
    }
    Ok(())
}

/// Remove the entry. The running instance (if any) is left alone so that
/// disabling "start at login" does not kill the running app.
pub fn uninstall(bootout: bool) {
    #[cfg(target_os = "macos")]
    if bootout {
        launchctl(&["bootout".into(), format!("{}/{}", domain(), LABEL)]);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = bootout;
    let _ = std::fs::remove_file(entry_file());
    log::info("login item removed");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(exec: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName=Shellder\nExec={exec}\n")
    }

    #[test]
    fn the_program_of_an_exec_line_is_read_whole() {
        for exe in [
            "/usr/bin/shellder",
            "/home/a b/My Apps/shellder.AppImage",
            "/opt/odd \"name\"/$HOME/`x`/back\\slash/100%/shellder",
        ] {
            let text = entry(&format!("{} {LOGIN_FLAG}", exec_quote(exe)));
            assert_eq!(desktop_program(&text).as_deref(), Some(exe), "{text}");
        }
    }

    #[test]
    fn entries_written_by_hand_or_by_older_copies_are_read() {
        // As older copies wrote it: quotes, an escaped quote, no more.
        let text = entry(r#""/home/a b/x \"y\"/shellder" --login"#);
        assert_eq!(
            desktop_program(&text).as_deref(),
            Some(r#"/home/a b/x "y"/shellder"#)
        );
        // The value's escape for a space, in quotes (unquoted, it would
        // split the argument like any space).
        let text = entry(r#""/home/a\sb/shellder" --login"#);
        assert_eq!(
            desktop_program(&text).as_deref(),
            Some("/home/a b/shellder")
        );
        // Spaces around the key, another group first with its own Exec.
        let text =
            "[Desktop Action New]\nExec=/wrong\n\n[Desktop Entry]\nExec = /usr/bin/shellder %u\n";
        assert_eq!(desktop_program(text).as_deref(), Some("/usr/bin/shellder"));
        assert_eq!(desktop_program("[Desktop Entry]\nName=x\n"), None);
    }

    #[test]
    fn the_plist_program_is_unescaped() {
        let text = "<key>ProgramArguments</key>\n<array>\n<string>/Apps/A &amp; B/shellder</string>\n<string>--login</string>";
        assert_eq!(plist_program(text).as_deref(), Some("/Apps/A & B/shellder"));
    }
}
