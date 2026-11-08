//! The Console's notifications of the system's (`commands::notify`): a
//! command done, the bell, a program's own. Each knows the window and the
//! tab it came from, so that a click on it brings that tab up (the page
//! is sent `notice-clicked`), and a tab looked at takes its own away from
//! the notification centre: they told of something the user has now seen.
//! No more than [`KEPT`] are left standing (a program that keeps
//! notifying would fill the notification centre): an older one is taken
//! away when a new one comes.
//!
//! On macOS they are the notification centre's own (`NSUserNotification`,
//! the app's delegate): the notification plugin waits for a click only on
//! a notification with buttons, and reports none on one without, and its
//! delegate has a notification of an app in front shown nowhere but in
//! the list. Ours shows it as a banner then too, since the tab it tells
//! of is out of sight (the page makes a toast of the others). Elsewhere
//! they go through the plugin, which reports a click on any.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Emitter};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_notification::NotificationExt;

use super::{sound, windows};
use crate::core::{log, prefs};

/// A notification shown and not yet clicked or taken away.
struct Shown {
    id: i32,
    window: String,
    tab: Option<u64>,
}

static SHOWN: Mutex<Vec<Shown>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicI32 = AtomicI32::new(1);
static APP: OnceLock<AppHandle> = OnceLock::new();

/// How many notifications are left standing at most.
const KEPT: usize = 12;

fn shown<T>(f: impl FnOnce(&mut Vec<Shown>) -> T) -> T {
    f(&mut SHOWN.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Listen to the clicks on the notifications. A notification shown before
/// this ran reports nothing, so it runs at start.
pub fn listen(app: &AppHandle) {
    let _ = APP.set(app.clone());
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(mac::install);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let h = app.clone();
        let _ = app.notification().on_action(move |done| {
            if done.action_id() != "tap" {
                return;
            }
            if let Some(n) = done.notification() {
                clicked(&h, n.id());
            }
        });
    }
}

/// A click on notification `id`: its window comes up (the quick terminal
/// drops in as its key would bring it) and the page is told which tab to
/// show. One of an earlier run, or one taken away meanwhile, is not
/// known: the click has made the app active, nothing more.
fn clicked(app: &AppHandle, id: i32) {
    let found = shown(|s| {
        let i = s.iter().position(|x| x.id == id)?;
        Some(s.remove(i))
    });
    let Some(Shown { window, tab, .. }) = found else {
        log::info(format!("notification {id} clicked, not one of those shown"));
        return;
    };
    log::info(format!(
        "notification {id} clicked: {window} tab {}",
        tab.map_or("none".into(), |t| t.to_string())
    ));
    let h = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !windows::bring_up(&h, &window) {
            return;
        }
        if let Some(tab) = tab {
            let _ = h.emit_to(&window, "notice-clicked", serde_json::json!({ "tab": tab }));
        }
    });
}

/// Show a notification from `window`'s tab `tab`.
pub fn show(
    app: &AppHandle,
    window: &str,
    tab: Option<u64>,
    title: &str,
    body: &str,
) -> Result<(), String> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    // Its sound: one of the system's by name, the notification centre's
    // to play; a file of the user's own, played here.
    let file = prefs::load()
        .terminal
        .notice_sound
        .filter(|s| !s.is_empty());
    let named = file.as_deref().and_then(sound::system_name);
    #[cfg(target_os = "macos")]
    {
        let (title, body, named) = (title.to_string(), body.to_string(), named.clone());
        app.run_on_main_thread(move || mac::deliver(id, &title, &body, named.as_deref()))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut b = app.notification().builder().id(id).title(title).body(body);
        if let Some(n) = &named {
            b = b.sound(n);
        }
        b.show().map_err(|e| e.to_string())?;
    }
    if let (Some(f), None) = (file, named) {
        let _ = app.run_on_main_thread(move || sound::play(&f));
    }
    let old: Vec<i32> = shown(|s| {
        s.push(Shown {
            id,
            window: window.to_string(),
            tab,
        });
        let over = s.len().saturating_sub(KEPT);
        s.drain(..over).map(|x| x.id).collect()
    });
    withdraw(app, old);
    Ok(())
}

/// The tab `tab` of `window` was looked at (or closed): its notifications
/// go.
pub fn seen(app: &AppHandle, window: &str, tab: u64) {
    let gone: Vec<i32> = shown(|s| {
        let (gone, kept): (Vec<Shown>, Vec<Shown>) = std::mem::take(s)
            .into_iter()
            .partition(|x| x.window == window && x.tab == Some(tab));
        *s = kept;
        gone.into_iter().map(|x| x.id).collect()
    });
    withdraw(app, gone);
}

/// Take notifications away from the notification centre.
fn withdraw(app: &AppHandle, ids: Vec<i32>) {
    if ids.is_empty() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(move || mac::withdraw(&ids));
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Err(e) = app.notification().remove_active(ids) {
            log::warn(format!("notifications not taken away: {e}"));
        }
    }
}

/// The notification centre's own notifications, on the main thread.
#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::CString;

    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol};
    use objc2::{class, define_class, msg_send, ClassType};

    use crate::core::log;

    /// A notification's identifier: this run's (the process's) and its
    /// number, so that a click on one left from an earlier run is not
    /// taken for this run's of the same number.
    fn identifier(id: i32) -> String {
        format!("{}{id}", prefix())
    }

    fn prefix() -> String {
        format!("shellder-{}-", std::process::id())
    }

    fn id_of(ident: &str) -> Option<i32> {
        ident.strip_prefix(&prefix())?.parse().ok()
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the
        // delegate has no Drop.
        #[unsafe(super(NSObject))]
        #[name = "ShellderNoticeDelegate"]
        struct Delegate;

        impl Delegate {
            #[unsafe(method(userNotificationCenter:didActivateNotification:))]
            fn did_activate(&self, center: &AnyObject, n: &AnyObject) {
                let ident = unsafe { ns_string(msg_send![n, identifier]) };
                let _: () = unsafe { msg_send![center, removeDeliveredNotification: n] };
                if let (Some(id), Some(app)) = (id_of(&ident), super::APP.get()) {
                    super::clicked(app, id);
                }
            }

            /// Shown as a banner while the app is in front too: the tab
            /// it tells of is out of sight.
            #[unsafe(method(userNotificationCenter:shouldPresentNotification:))]
            fn should_present(&self, _center: &AnyObject, _n: &AnyObject) -> Bool {
                Bool::YES
            }
        }

        unsafe impl NSObjectProtocol for Delegate {}
    );

    fn center() -> Option<*mut AnyObject> {
        let c: *mut AnyObject = unsafe {
            msg_send![
                class!(NSUserNotificationCenter),
                defaultUserNotificationCenter
            ]
        };
        (!c.is_null()).then_some(c)
    }

    /// The delegate set on the notification centre, for the app's life
    /// (the centre does not keep it).
    pub fn install() {
        autoreleasepool(|_| {
            let Some(c) = center() else {
                log::warn("no notification centre: the clicks on notifications go unheard");
                return;
            };
            let d: Retained<Delegate> = unsafe { msg_send![Delegate::class(), new] };
            let _: () = unsafe { msg_send![c, setDelegate: &*d] };
            std::mem::forget(d);
        });
    }

    pub fn deliver(id: i32, title: &str, body: &str, sound: Option<&str>) {
        autoreleasepool(|_| unsafe {
            let Some(c) = center() else {
                log::warn(format!(
                    "notification not shown ({title}): no notification centre"
                ));
                return;
            };
            let n: Retained<AnyObject> = msg_send![class!(NSUserNotification), new];
            let _: () = msg_send![&*n, setIdentifier: ns(&identifier(id))];
            let _: () = msg_send![&*n, setTitle: ns(title)];
            let _: () = msg_send![&*n, setInformativeText: ns(body)];
            if let Some(s) = sound {
                let _: () = msg_send![&*n, setSoundName: ns(s)];
            }
            let _: () = msg_send![c, deliverNotification: &*n];
        });
    }

    /// Take this run's notifications `ids` away; those gone already
    /// (cleared by hand, clicked) are not an error.
    pub fn withdraw(ids: &[i32]) {
        autoreleasepool(|_| unsafe {
            let Some(c) = center() else {
                return;
            };
            let delivered: *mut AnyObject = msg_send![c, deliveredNotifications];
            if delivered.is_null() {
                return;
            }
            let count: usize = msg_send![delivered, count];
            let mut gone = Vec::new();
            for i in 0..count {
                let n: *mut AnyObject = msg_send![delivered, objectAtIndex: i];
                if id_of(&ns_string(msg_send![n, identifier])).is_some_and(|id| ids.contains(&id)) {
                    gone.push(n);
                }
            }
            log::info(format!(
                "{} of {} notification(s) taken away",
                gone.len(),
                ids.len()
            ));
            for n in gone {
                let _: () = msg_send![c, removeDeliveredNotification: n];
            }
        });
    }

    /// An autoreleased `NSString` of the text (a NUL in it, which no
    /// notification needs, cut there).
    fn ns(s: &str) -> *mut AnyObject {
        let s = s.split('\0').next().unwrap_or("");
        let c = CString::new(s).unwrap_or_default();
        unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    /// An `NSString` as text; nil as none.
    unsafe fn ns_string(s: *mut AnyObject) -> String {
        if s.is_null() {
            return String::new();
        }
        let p: *const std::os::raw::c_char = unsafe { msg_send![s, UTF8String] };
        if p.is_null() {
            return String::new();
        }
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn only_this_runs_identifiers_are_ours() {
            assert_eq!(id_of(&identifier(7)), Some(7));
            // An earlier run's (no process is 0).
            assert_eq!(id_of("shellder-0-7"), None);
            assert_eq!(id_of(""), None);
            assert_eq!(id_of(&format!("{}x", prefix())), None);
        }
    }
}
