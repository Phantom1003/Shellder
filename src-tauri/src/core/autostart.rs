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
        // The first <string> after ProgramArguments.
        let after = text.split("<key>ProgramArguments</key>").nth(1)?;
        let s = after.split("<string>").nth(1)?;
        Some(s.split("</string>").next()?.to_string())
    } else {
        let line = text.lines().find(|l| l.starts_with("Exec="))?;
        let exec = line.trim_start_matches("Exec=");
        Some(unquote(exec.split(' ').next().unwrap_or("")))
    }
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').replace("\\ ", " ")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
            "[Desktop Entry]\nType=Application\nName=Shellder\nComment=Keeps ssh ControlMaster connections alive\nExec=\"{}\" {}\nX-GNOME-Autostart-enabled=true\n",
            exe.replace('"', "\\\""),
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
