//! Checks GitHub Releases for a newer Shellder and installs it in place,
//! through Tauri's updater (a signed `latest.json` next to the release
//! assets, see .github/workflows/build.yml). Automatic checks run shortly
//! after launch and once a day. Nothing is installed without a click on
//! "Install and relaunch".

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

use super::model::{Model, UpdateView};
use crate::core::config;
use crate::core::log;
use crate::core::prefs;

pub const INTERVAL: Duration = Duration::from_secs(24 * 3600);

static FOUND: Mutex<Option<Update>> = Mutex::new(None);

/// How long the download's progress may stand while the whole percent
/// stays the same.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The download's progress, told to the windows only when the whole
/// percent changed or `PROGRESS_EVERY` has passed: a bundle comes in
/// thousands of chunks, and each told would be a state for every window.
#[derive(Default)]
struct Progress {
    received: u64,
    told: Option<(Option<u64>, Instant)>,
}

impl Progress {
    /// A chunk came in: the fraction to tell, when there is one to tell.
    fn step(&mut self, chunk: usize, total: Option<u64>, now: Instant) -> Option<Option<f64>> {
        self.received += chunk as u64;
        let fraction = total.map(|t| (self.received as f64 / t as f64).min(1.0));
        let percent = fraction.map(|f| (f * 100.0).floor() as u64);
        if let Some((p, at)) = self.told {
            if p == percent && now.duration_since(at) < PROGRESS_EVERY {
                return None;
            }
        }
        self.told = Some((percent, now));
        Some(fraction)
    }
}

fn release_page(version: &str) -> String {
    format!(
        "https://github.com/{}/releases/tag/v{version}",
        config::RELEASE_REPO
    )
}

/// Check at launch and daily, while the setting is on.
pub fn schedule(app: &AppHandle, model: &Arc<Model>) {
    let app = app.clone();
    let model = model.clone();
    std::thread::Builder::new()
        .name("shellder-updater".into())
        .spawn(move || {
            // The launch has enough to do, look a few seconds later.
            std::thread::sleep(Duration::from_secs(5));
            loop {
                if prefs::load().auto_update() {
                    check(&app, &model, false);
                }
                std::thread::sleep(INTERVAL);
            }
        })
        .expect("updater thread");
}

fn busy(model: &Model) -> bool {
    matches!(
        model.update_view().state.as_str(),
        "checking" | "downloading" | "installing"
    )
}

pub fn check(app: &AppHandle, model: &Arc<Model>, manual: bool) {
    if busy(model) {
        return;
    }
    if !manual && model.update_view().state == "available" {
        return;
    }
    model.set_update(UpdateView {
        state: "checking".into(),
        ..Default::default()
    });
    let app = app.clone();
    let model = model.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let url = url::Url::parse(&config::get().update_api).map_err(|e| e.to_string())?;
            let updater = app
                .updater_builder()
                .endpoints(vec![url])
                .map_err(|e| e.to_string())?
                .build()
                .map_err(|e| e.to_string())?;
            updater.check().await.map_err(|e| e.to_string())
        }
        .await;
        match result {
            Ok(Some(u)) => {
                log::info(format!("update: version {} is available", u.version));
                let view = UpdateView {
                    state: "available".into(),
                    version: Some(u.version.clone()),
                    page: Some(release_page(&u.version)),
                    notes: u.body.clone(),
                    ..Default::default()
                };
                *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = Some(u);
                model.set_update(view);
            }
            Ok(None) => {
                log::info("update: up to date");
                model.set_update(UpdateView {
                    state: "upToDate".into(),
                    ..Default::default()
                });
            }
            Err(e) => {
                log::warn(format!("update: check failed: {e}"));
                model.set_update(UpdateView {
                    state: "failed".into(),
                    message: Some(e),
                    ..Default::default()
                });
            }
        }
    });
}

/// Download the release found by the last check, install it over this
/// bundle and start the new copy.
pub fn install(app: &AppHandle, model: &Arc<Model>) {
    let Some(update) = FOUND.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    let version = update.version.clone();
    let page = release_page(&version);
    model.set_update(UpdateView {
        state: "downloading".into(),
        version: Some(version.clone()),
        page: Some(page.clone()),
        progress: None,
        ..Default::default()
    });
    let app = app.clone();
    let model = model.clone();
    tauri::async_runtime::spawn(async move {
        let m = model.clone();
        let v = version.clone();
        let p = page.clone();
        let mut progress = Progress::default();
        let result = update
            .download_and_install(
                move |chunk, total| {
                    let Some(fraction) = progress.step(chunk, total, Instant::now()) else {
                        return;
                    };
                    m.set_update(UpdateView {
                        state: "downloading".into(),
                        version: Some(v.clone()),
                        page: Some(p.clone()),
                        progress: fraction,
                        ..Default::default()
                    });
                },
                {
                    let m = model.clone();
                    let v = version.clone();
                    let p = page.clone();
                    move || {
                        m.set_update(UpdateView {
                            state: "installing".into(),
                            version: Some(v.clone()),
                            page: Some(p.clone()),
                            ..Default::default()
                        })
                    }
                },
            )
            .await;
        match result {
            Ok(()) => {
                log::info(format!("update: {version} installed, relaunching"));
                model.shutdown();
                super::restart(&app)
            }
            Err(e) => {
                log::error(format!("update: install failed: {e}"));
                model.set_update(UpdateView {
                    state: "failed".into(),
                    message: Some(e.to_string()),
                    ..Default::default()
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_progress_is_told_once_a_percent() {
        let mut p = Progress::default();
        let start = Instant::now();
        let told = (0..10_000)
            .filter_map(|_| p.step(10, Some(100_000), start))
            .count();
        // 0 % with the first chunk, then each whole percent up to 100.
        assert_eq!(told, 101);

        // Without a total, once every PROGRESS_EVERY.
        let mut p = Progress::default();
        let told = (0..1000u64)
            .filter_map(|i| p.step(10, None, start + Duration::from_millis(i)))
            .count();
        assert_eq!(told, 10);
    }
}
