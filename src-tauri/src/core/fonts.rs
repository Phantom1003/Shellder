//! The fonts installed on this machine, for the Console's settings to
//! pick a terminal font from: every family by name, the monospaced ones
//! told apart (the `post` table's fixed-pitch flag, which the terminal
//! fonts set), read from the system's font folders with fontdb.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Family {
    pub name: String,
    pub monospaced: bool,
}

/// What the Console's settings get: the families, and the one the
/// terminals use when none is chosen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fonts {
    pub default_family: String,
    pub families: Vec<Family>,
}

/// The families and the default, read together (`families`).
pub fn fonts() -> Fonts {
    Fonts {
        default_family: default_family(),
        families: families(),
    }
}

/// The family the terminals' default stack (ui/console.js's
/// `FONT_STACK`, `ui-monospace` first, the generic `monospace` last)
/// comes out as: the system's monospace font, which on macOS is SF
/// Mono (WebKit's `ui-monospace` there), and on Linux what fontconfig
/// matches for `monospace`, or that word itself when it cannot say.
pub fn default_family() -> String {
    #[cfg(target_os = "macos")]
    {
        "SF Mono".to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "monospace"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.split(',').next().map(|f| f.trim().to_string()))
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| "monospace".to_string())
    }
}

/// The font families on this machine, sorted by name without regard to
/// case, each once. A family with one monospaced face counts as
/// monospaced. Reads every font file's name table, so it takes a moment:
/// call it off the main thread.
pub fn families() -> Vec<Family> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut out: Vec<Family> = Vec::new();
    for face in db.faces() {
        let Some((name, _)) = face.families.first() else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        match out.iter_mut().find(|f| f.name == name) {
            Some(f) => f.monospaced |= face.monospaced,
            None => out.push(Family {
                name: name.to_string(),
                monospaced: face.monospaced,
            }),
        }
    }
    sort(&mut out);
    out
}

fn sort(families: &mut [Family]) {
    families.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_machines_families_once_and_in_order() {
        let fams = families();
        assert!(!fams.is_empty(), "no fonts found on this machine");
        for w in fams.windows(2) {
            assert!(
                w[0].name.to_lowercase() <= w[1].name.to_lowercase(),
                "{:?} before {:?}",
                w[0].name,
                w[1].name
            );
            assert_ne!(w[0].name, w[1].name);
        }
        assert!(fams.iter().all(|f| !f.name.starts_with('.')));
    }

    #[test]
    fn names_a_default() {
        assert!(!default_family().is_empty());
    }

    #[test]
    fn sorts_without_regard_to_case() {
        let mut v = vec![
            Family {
                name: "menlo".into(),
                monospaced: true,
            },
            Family {
                name: "Arial".into(),
                monospaced: false,
            },
            Family {
                name: "Menlo".into(),
                monospaced: true,
            },
        ];
        sort(&mut v);
        let names: Vec<&str> = v.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Arial", "Menlo", "menlo"]);
    }
}
