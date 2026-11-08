//! The Console's sounds: the bell's (`terminal.bellSound`) and the one a
//! notification comes with (`terminal.noticeSound`). A sound is a file,
//! one of the system's (`sounds` lists them, for the settings) or the
//! user's own; the bell without one is the system's alert sound.
//!
//! On macOS a notification's sound of the system's is the notification
//! centre's to play (`system_name`), which keeps it quiet with the
//! notification when a Focus does; a file of the user's own the
//! notification centre cannot play, and the app plays it itself.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::core::log;

/// A sound of the system's, for the settings' lists: its name (the
/// file's) and its file.
#[derive(Clone, serde::Serialize)]
pub struct Sound {
    pub name: String,
    pub path: String,
}

/// The kinds of sound file listed and offered: what AppKit plays, and
/// the Ogg the Linux sound themes ship.
pub const KINDS: &[&str] = &[
    "aiff", "aif", "caf", "wav", "mp3", "m4a", "oga", "ogg", "flac",
];

/// The folders of the system's sounds, the user's own first: on macOS
/// the Sounds folders the notification centre takes a sound's name from,
/// on Linux the XDG sound themes' stereo sounds.
fn dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        let mut v: Vec<PathBuf> = home.map(|h| h.join("Library/Sounds")).into_iter().collect();
        v.push("/Library/Sounds".into());
        v.push("/System/Library/Sounds".into());
        v
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut roots: Vec<PathBuf> = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".local/share")))
            .into_iter()
            .collect();
        let shared =
            std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        roots.extend(
            shared
                .split(':')
                .filter(|d| !d.is_empty())
                .map(PathBuf::from),
        );
        let mut v = Vec::new();
        for r in roots {
            let Ok(themes) = std::fs::read_dir(r.join("sounds")) else {
                continue;
            };
            let mut themes: Vec<PathBuf> = themes
                .flatten()
                .map(|e| e.path().join("stereo"))
                .filter(|p| p.is_dir())
                .collect();
            themes.sort();
            v.extend(themes);
        }
        v
    }
}

fn is_sound(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KINDS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The system's sounds by name, one per name (the first folder's, the
/// one the notification centre would play by that name).
pub fn sounds() -> Vec<Sound> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_sound(p))
            .collect();
        files.sort();
        for p in files {
            let Some(name) = p.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if seen.insert(name.to_string()) {
                out.push(Sound {
                    name: name.to_string(),
                    path: p.to_string_lossy().into_owned(),
                });
            }
        }
    }
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

/// The name the notification centre plays `path` by, for a notification
/// that comes with it: a file of a Sounds folder (macOS), the one listed
/// by that name. None for any other file, and on Linux, where the
/// notification servers that play a sound at all play a theme's by its
/// name, not necessarily the file picked: the app plays those itself.
pub fn system_name(path: &str) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let name = Path::new(path).file_stem()?.to_str()?;
    sounds()
        .into_iter()
        .any(|s| s.path == path)
        .then(|| name.to_string())
}

/// Play `sound`: a sound file; the system's alert sound for none, and in
/// place of a file that does not play (gone, or not a sound). On the
/// main thread: AppKit's and GTK's.
pub fn play(sound: &str) {
    if sound.is_empty() {
        return beep();
    }
    if play_file(sound) {
        return;
    }
    thread_local! {
        static SAID: std::cell::RefCell<HashSet<String>> = Default::default();
    }
    if SAID.with(|s| s.borrow_mut().insert(sound.to_string())) {
        log::warn(format!(
            "sound {sound} does not play, the alert sound instead"
        ));
    }
    beep();
}

/// The system's alert sound: the one picked in the Sound settings, at
/// their volume, on macOS; the desktop's bell on Linux.
fn beep() {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "AppKit", kind = "framework")]
        unsafe extern "C" {
            fn NSBeep();
        }
        unsafe { NSBeep() };
    }
    #[cfg(target_os = "linux")]
    {
        // GDK is linked already: the web view is GTK's.
        unsafe extern "C" {
            fn gdk_display_get_default() -> *mut std::ffi::c_void;
            fn gdk_display_beep(display: *mut std::ffi::c_void);
        }
        unsafe {
            let d = gdk_display_get_default();
            if !d.is_null() {
                gdk_display_beep(d);
            }
        }
    }
}

/// An `NSSound` of the file, loaded once and kept (a sound let go while
/// it plays stops), started again from its beginning when it is still
/// playing. False when it does not load or play.
#[cfg(target_os = "macos")]
fn play_file(path: &str) -> bool {
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static LOADED: RefCell<HashMap<String, Retained<AnyObject>>> = RefCell::new(HashMap::new());
    }
    if !Path::new(path).is_file() {
        return false;
    }
    let Ok(c) = std::ffi::CString::new(path) else {
        return false;
    };
    objc2::rc::autoreleasepool(|_| {
        LOADED.with(|loaded| {
            let mut loaded = loaded.borrow_mut();
            if !loaded.contains_key(path) {
                // A settings window's tries would keep every sound tried.
                if loaded.len() >= 16 {
                    loaded.clear();
                }
                let snd: Option<Retained<AnyObject>> = unsafe {
                    let name: *mut AnyObject =
                        msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
                    if name.is_null() {
                        return false;
                    }
                    let a: Allocated<AnyObject> = msg_send![class!(NSSound), alloc];
                    msg_send![a, initWithContentsOfFile: name, byReference: Bool::YES]
                };
                let Some(snd) = snd else {
                    return false;
                };
                loaded.insert(path.to_string(), snd);
            }
            let snd = &loaded[path];
            unsafe {
                let playing: Bool = msg_send![&**snd, isPlaying];
                if playing.as_bool() {
                    let _: Bool = msg_send![&**snd, stop];
                }
                let ok: Bool = msg_send![&**snd, play];
                ok.as_bool()
            }
        })
    })
}

/// The first of the desktop's players found (PipeWire's, PulseAudio's,
/// libcanberra's, ALSA's for a WAV), waited for on a thread of its own.
#[cfg(not(target_os = "macos"))]
fn play_file(path: &str) -> bool {
    use std::process::{Command, Stdio};
    if !Path::new(path).is_file() {
        return false;
    }
    let players: [(&str, &[&str]); 4] = [
        ("pw-play", &[]),
        ("paplay", &[]),
        ("canberra-gtk-play", &["-f"]),
        ("aplay", &["-q"]),
    ];
    for (cmd, args) in players {
        let child = Command::new(cmd)
            .args(args)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut c) = child {
            std::thread::spawn(move || {
                let _ = c.wait();
            });
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_ones_own_is_played_by_the_app() {
        assert_eq!(system_name("/tmp/shellder-test/ding.aiff"), None);
        assert_eq!(system_name(""), None);
    }

    /// macOS has its sounds in /System/Library/Sounds, which the
    /// notification centre plays by name.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_systems_sounds_go_to_the_notification_centre_by_name() {
        let list = sounds();
        let glass = list
            .iter()
            .find(|s| s.name == "Glass")
            .expect("Glass among the system's sounds");
        assert_eq!(system_name(&glass.path).as_deref(), Some("Glass"));
        assert!(list
            .windows(2)
            .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
    }
}
