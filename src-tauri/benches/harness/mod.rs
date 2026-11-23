//! The timer of the performance tests: runs a piece of work, prints how
//! long it took (the median of several runs) or how fast it moved its
//! bytes, and holds it to a budget. A budget is several times what the
//! work takes on an ordinary machine, so that it is missed by a change
//! that made the work slower in kind (a listing read twice, a loop gone
//! quadratic), not by a busy machine. `finish` exits with 1 when one was
//! missed; `SHELLDER_BENCH_LAX=1` only prints.
#![allow(dead_code)]

use std::hint::black_box;
use std::time::{Duration, Instant};

pub struct Bench {
    /// Only the measurements whose name contains this are run.
    filter: Option<String>,
    lax: bool,
    ran: usize,
    missed: Vec<String>,
}

/// A piece of work is run until a sample lasts this long, so that the
/// clock's own cost is nothing beside it.
const SAMPLE: Duration = Duration::from_millis(40);
const SAMPLES: usize = 9;

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// "412 ns", "3.20 µs", "18.4 ms", "2.31 s".
pub fn brief(d: Duration) -> String {
    let ns = d.as_nanos() as f64;
    if ns < 1e3 {
        format!("{ns:.0} ns")
    } else if ns < 1e6 {
        format!("{:.2} µs", ns / 1e3)
    } else if ns < 1e9 {
        format!("{:.2} ms", ns / 1e6)
    } else {
        format!("{:.2} s", ns / 1e9)
    }
}

impl Bench {
    /// `cargo bench --bench NAME -- WORDS`: the first argument that is
    /// not a flag (cargo passes `--bench`) picks the measurements.
    pub fn from_args(title: &str) -> Bench {
        let filter = std::env::args().skip(1).find(|a| !a.starts_with('-'));
        println!("\n{title}");
        Bench {
            filter,
            lax: std::env::var_os("SHELLDER_BENCH_LAX").is_some(),
            ran: 0,
            missed: vec![],
        }
    }

    pub fn wants(&self, name: &str) -> bool {
        self.filter.as_deref().map_or(true, |f| name.contains(f))
    }

    fn line(&mut self, name: &str, measured: String, budget: String, within: bool) {
        self.ran += 1;
        let mark = if within { "" } else { "   OVER BUDGET" };
        println!("  {name:<58} {measured:>12}   ({budget}){mark}");
        if !within {
            self.missed.push(name.to_string());
        }
    }

    /// One call of `work`, which is called over and over: the median of
    /// the samples, each long enough to be timed.
    pub fn each<R>(&mut self, name: &str, budget: Duration, mut work: impl FnMut() -> R) {
        if !self.wants(name) {
            return;
        }
        let mut calls = 1u32;
        loop {
            let t = Instant::now();
            for _ in 0..calls {
                black_box(work());
            }
            if t.elapsed() >= SAMPLE || calls >= 1 << 24 {
                break;
            }
            calls *= 2;
        }
        let samples = (0..SAMPLES)
            .map(|_| {
                let t = Instant::now();
                for _ in 0..calls {
                    black_box(work());
                }
                t.elapsed() / calls
            })
            .collect();
        let took = median(samples);
        self.line(
            name,
            brief(took),
            format!("under {}", brief(budget)),
            took <= budget,
        );
    }

    /// Work that times itself, its preparations left out: `work` says how
    /// long the part that counts took, and is run `runs` times.
    pub fn timed(
        &mut self,
        name: &str,
        runs: usize,
        budget: Duration,
        mut work: impl FnMut() -> Duration,
    ) {
        if !self.wants(name) {
            return;
        }
        let took = median((0..runs).map(|_| work()).collect());
        self.line(
            name,
            brief(took),
            format!("under {}", brief(budget)),
            took <= budget,
        );
    }

    /// Bytes moved: `work` says how many and how long they took, and the
    /// rate is held to at least `floor` MB a second.
    pub fn rate(
        &mut self,
        name: &str,
        runs: usize,
        floor: f64,
        mut work: impl FnMut() -> (usize, Duration),
    ) {
        if !self.wants(name) {
            return;
        }
        let mut rates: Vec<f64> = (0..runs)
            .map(|_| {
                let (bytes, took) = work();
                bytes as f64 / 1_048_576.0 / took.as_secs_f64().max(1e-9)
            })
            .collect();
        rates.sort_by(|a, b| a.total_cmp(b));
        let rate = rates[rates.len() / 2];
        self.line(
            name,
            format!("{rate:.1} MB/s"),
            format!("at least {floor} MB/s"),
            rate >= floor,
        );
    }

    /// The same work done without the app (plain ssh, a pipe), for the
    /// line above it to be read against: printed, held to nothing.
    pub fn compare_timed(&mut self, name: &str, runs: usize, mut work: impl FnMut() -> Duration) {
        if !self.wants(name) {
            return;
        }
        let took = median((0..runs).map(|_| work()).collect());
        self.line(name, brief(took), "for comparison".into(), true);
    }

    pub fn compare_rate(
        &mut self,
        name: &str,
        runs: usize,
        mut work: impl FnMut() -> (usize, Duration),
    ) {
        if !self.wants(name) {
            return;
        }
        let mut rates: Vec<f64> = (0..runs)
            .map(|_| {
                let (bytes, took) = work();
                bytes as f64 / 1_048_576.0 / took.as_secs_f64().max(1e-9)
            })
            .collect();
        rates.sort_by(|a, b| a.total_cmp(b));
        let rate = rates[rates.len() / 2];
        self.line(
            name,
            format!("{rate:.1} MB/s"),
            "for comparison".into(),
            true,
        );
    }

    /// What could not be measured here, and why.
    pub fn skipped(&self, what: &str, why: &str) {
        println!("  {what}: skipped, {why}");
    }

    pub fn finish(self) -> ! {
        if self.ran == 0 {
            println!("  nothing is named like that");
        }
        if self.missed.is_empty() || self.lax {
            std::process::exit(0);
        }
        eprintln!("\nover budget: {}", self.missed.join("; "));
        std::process::exit(1);
    }
}

/// A directory of the measurements' own, which is also the state
/// directory (prefs, log) and holds the ssh config, so that nothing of
/// the user's is read or written. Set before anything asks `config`.
pub fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("shjt-bench-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_str().unwrap().to_string();
    std::env::set_var("SHELLDER_STATE_DIR", &d);
    std::env::set_var("SHELLDER_SSH_CONFIG", format!("{d}/config"));
    std::env::set_var("SHELLDER_NO_KEYCHAIN", "1");
    d
}
