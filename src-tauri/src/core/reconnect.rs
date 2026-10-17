//! The reconnect rule: how long a locked host waits between the attempts
//! that bring a dropped connection back, and when it stops trying. One
//! rule in the prefs is the default for every host, and a host may have
//! one of its own. A field a rule leaves unset takes the value from the
//! rule below it: a host's from the default one, the default one from the
//! built-in values in `config`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{config, prefs};

/// How the waits between attempts follow each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Growth {
    /// Every wait is the same, the first.
    Fixed,
    /// Each wait is twice the one before, up to the longest; after
    /// `config::MAX_QUICK_FAILURES` quick failures in a row, the longest
    /// (a wrong password must not hammer the server).
    Doubling,
}

impl Growth {
    pub fn parse(s: &str) -> Option<Growth> {
        match s {
            "fixed" => Some(Growth::Fixed),
            "doubling" => Some(Growth::Doubling),
            _ => None,
        }
    }
    pub fn raw(self) -> &'static str {
        match self {
            Growth::Fixed => "fixed",
            Growth::Doubling => "doubling",
        }
    }
}

/// A rule with every field decided, as the daemon follows it and the
/// pages show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub growth: Growth,
    /// The first wait (the only one, when fixed), in seconds.
    pub first: u64,
    /// The longest wait, in seconds.
    pub max: u64,
    /// Stop trying once the connection has been down this long, in
    /// seconds; 0 is never.
    pub give_up: u64,
}

/// Bounds a rule is kept within: a wait of at least a second, no longer
/// than a day; a give-up of at most a week.
pub const FIRST_MAX: u64 = 3600;
pub const WAIT_MAX: u64 = 86_400;
pub const GIVE_UP_MAX: u64 = 7 * 86_400;

impl Rule {
    pub const DEFAULT: Rule = Rule {
        growth: Growth::Doubling,
        first: config::BACKOFF_MIN.as_secs(),
        max: config::BACKOFF_MAX.as_secs(),
        give_up: 0,
    };

    /// The wait after the `failures`th failure in a row (counted from 1;
    /// 0 is a drop after a connection that held, which starts over).
    pub fn wait(&self, failures: u32) -> Duration {
        let n = failures.max(1);
        let secs = match self.growth {
            Growth::Fixed => self.first,
            Growth::Doubling if n >= config::MAX_QUICK_FAILURES => self.max,
            Growth::Doubling => self
                .first
                .saturating_mul(1u64 << (n - 1).min(40))
                .min(self.max),
        };
        Duration::from_secs(secs)
    }

    pub fn gives_up(&self) -> Option<Duration> {
        (self.give_up > 0).then(|| Duration::from_secs(self.give_up))
    }

    /// "5s → 10m, doubling", "every 30s", "…, gives up after 1h": the log's
    /// words for a rule.
    pub fn describe(&self) -> String {
        let mut s = match self.growth {
            Growth::Fixed => format!("every {}", brief(self.first)),
            Growth::Doubling => format!("{} → {}, doubling", brief(self.first), brief(self.max)),
        };
        if self.give_up > 0 {
            s.push_str(&format!(", gives up after {}", brief(self.give_up)));
        }
        s
    }
}

/// "5s", "10m", "1h", "90m".
fn brief(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

/// What the prefs keep: only the fields chosen. `apply` fills the rest
/// from the rule below and keeps the result within bounds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Partial {
    /// "fixed" or "doubling".
    pub growth: Option<String>,
    pub first: Option<u64>,
    pub max: Option<u64>,
    pub give_up: Option<u64>,
}

impl Partial {
    pub fn apply(&self, base: Rule) -> Rule {
        let growth = self
            .growth
            .as_deref()
            .and_then(Growth::parse)
            .unwrap_or(base.growth);
        let first = self.first.unwrap_or(base.first).clamp(1, FIRST_MAX);
        let max = self.max.unwrap_or(base.max).clamp(first, WAIT_MAX);
        let give_up = self.give_up.unwrap_or(base.give_up).min(GIVE_UP_MAX);
        Rule {
            growth,
            first,
            max,
            give_up,
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Partial::default()
    }
}

/// The default rule, as the prefs have it.
pub fn default_rule(p: &prefs::Prefs) -> Rule {
    p.reconnect.apply(Rule::DEFAULT)
}

/// Every host that has a rule of its own, decided.
pub fn host_rules(p: &prefs::Prefs) -> BTreeMap<String, Rule> {
    let base = default_rule(p);
    p.host_reconnect
        .iter()
        .map(|(h, r)| (h.clone(), r.apply(base)))
        .collect()
}

/// The rule a host follows, read afresh from the prefs.
pub fn for_host(host: &str) -> Rule {
    let p = prefs::load();
    let base = default_rule(&p);
    p.host_reconnect
        .get(host)
        .map(|r| r.apply(base))
        .unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubling_reaches_the_longest_wait_at_the_third_quick_failure() {
        let r = Rule::DEFAULT;
        assert_eq!(r.wait(0), Duration::from_secs(5));
        assert_eq!(r.wait(1), Duration::from_secs(5));
        assert_eq!(r.wait(2), Duration::from_secs(10));
        assert_eq!(r.wait(3), Duration::from_secs(600));
        assert_eq!(r.wait(50), Duration::from_secs(600));
        let short = Rule {
            max: 8,
            ..Rule::DEFAULT
        };
        assert_eq!(short.wait(2), Duration::from_secs(8));
    }

    #[test]
    fn fixed_waits_the_same_every_time() {
        let r = Rule {
            growth: Growth::Fixed,
            first: 30,
            max: 600,
            give_up: 0,
        };
        for n in 0..10 {
            assert_eq!(r.wait(n), Duration::from_secs(30));
        }
        assert_eq!(r.gives_up(), None);
    }

    #[test]
    fn partial_fills_in_from_the_base_and_stays_within_bounds() {
        let p = Partial {
            growth: Some("fixed".into()),
            first: Some(0),
            max: None,
            give_up: Some(10 * GIVE_UP_MAX),
        };
        let r = p.apply(Rule::DEFAULT);
        assert_eq!(r.growth, Growth::Fixed);
        assert_eq!(r.first, 1);
        assert_eq!(r.max, 600);
        assert_eq!(r.give_up, GIVE_UP_MAX);
        // The longest wait is never shorter than the first.
        let p = Partial {
            first: Some(900),
            max: Some(60),
            ..Partial::default()
        };
        let r = p.apply(Rule::DEFAULT);
        assert_eq!((r.first, r.max), (900, 900));
        // An unknown growth keeps the base's.
        let p = Partial {
            growth: Some("sideways".into()),
            ..Partial::default()
        };
        assert_eq!(p.apply(Rule::DEFAULT), Rule::DEFAULT);
        assert!(Partial::default().is_empty());
    }

    #[test]
    fn a_host_rule_falls_back_on_the_default_rule() {
        let mut p = prefs::Prefs::default();
        p.reconnect.first = Some(20);
        p.host_reconnect.insert(
            "a".into(),
            Partial {
                growth: Some("fixed".into()),
                ..Partial::default()
            },
        );
        assert_eq!(default_rule(&p).first, 20);
        let rules = host_rules(&p);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules["a"].growth, Growth::Fixed);
        assert_eq!(rules["a"].first, 20);
        assert_eq!(rules["a"].wait(5), Duration::from_secs(20));
    }

    #[test]
    fn describe() {
        assert_eq!(Rule::DEFAULT.describe(), "5s → 10m, doubling");
        let r = Rule {
            growth: Growth::Fixed,
            first: 90,
            max: 90,
            give_up: 3600,
        };
        assert_eq!(r.describe(), "every 1m 30s, gives up after 1h");
    }
}
