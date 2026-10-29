//! A copy that goes on from where it stopped, against this machine's own
//! sshd (Remote Login): a file the destination has a third of is
//! completed, one it has whole is left alone, a directory gets what is
//! missing or short, and the Files model's "continue" does the same without
//! asking whether the name is taken. Skipped unless `ssh 127.0.0.1` logs
//! this user in with a key and no questions.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use shellder::app::files::{self, FilesModel, Pane, TransferState};
use shellder::core::ssh::{self, FileSource};
use shellder::core::transfer::{TransferJob, TransferRun};

const HOST: &str = "shjt-resume";

fn ssh(d: &str, args: &[&str]) -> bool {
    Command::new("ssh")
        .args(["-F", &format!("{d}/config")])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

/// Bytes that are not all alike, so a copy that landed in the wrong place
/// or order would show.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn truncate(path: &str, to: usize) {
    let mut data = std::fs::read(path).unwrap();
    data.truncate(to);
    std::fs::write(path, data).unwrap();
}

fn same(a: &str, b: &str) -> bool {
    std::fs::read(a).unwrap() == std::fs::read(b).unwrap()
}

fn copy(job: &TransferJob) -> ssh::Output {
    let run = TransferRun::new();
    let total = TransferRun::total(job);
    run.run(job, total, |_| {})
}

fn job(
    from: FileSource,
    source: &str,
    to: FileSource,
    destination: &str,
    resume: bool,
) -> TransferJob {
    TransferJob {
        from,
        to,
        source: source.into(),
        destination: destination.into(),
        resume,
    }
}

/// The master `ssh -M -f` left behind, closed when the test ends.
struct Master;
impl Drop for Master {
    fn drop(&mut self) {
        let _ = Command::new("ssh")
            .args(ssh::base_args())
            .args(["-O", "exit", HOST])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn a_copy_goes_on_from_where_it_stopped() {
    let dir = std::env::temp_dir().join(format!("shjt-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_str().unwrap().to_string();
    let cp_dir = format!("/tmp/shjt-cm-{}", std::process::id());
    std::fs::create_dir_all(&cp_dir).unwrap();
    std::env::set_var("SHELLDER_SSH_CONFIG", format!("{d}/config"));
    std::env::set_var("SHELLDER_STATE_DIR", &d);

    // This user's own keys (ssh's defaults, not read from any config) and
    // the system's sshd on port 22; the host key goes into a file of its
    // own so the user's known_hosts is not touched.
    let user = std::env::var("USER").unwrap();
    std::fs::write(
        format!("{d}/config"),
        format!(
            "Host {HOST}\n    HostName 127.0.0.1\n    Port 22\n    User {user}\n    BatchMode yes\n\
             ControlMaster auto\n    ControlPath {cp_dir}/cm-%C\n    StrictHostKeyChecking accept-new\n    UserKnownHostsFile {d}/known_hosts\n    LogLevel ERROR\n"
        ),
    )
    .unwrap();
    if !ssh(
        &d,
        &[
            "-o",
            "ControlMaster=no",
            "-o",
            "ConnectTimeout=5",
            HOST,
            "true",
        ],
    ) {
        eprintln!("ssh 127.0.0.1 does not log this user in without a question (Remote Login off, or no key of yours in ~/.ssh/authorized_keys), skipping");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&cp_dir);
        return;
    }
    assert!(
        ssh(&d, &["-M", "-f", "-N", HOST]),
        "the master did not come up"
    );
    let _master = Master;

    // "Remote" and "local" are two directories on this disk, reached through
    // the daemon and directly: that is what lets the test cut a copy short.
    let remote = format!("{d}/remote");
    let local = format!("{d}/local");
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::create_dir_all(&local).unwrap();
    let host = FileSource::Host(HOST.into());
    let mb = 1024 * 1024;

    // MARK: a download a third of which is there
    let name = "we \"ird\" name.bin";
    let src = format!("{remote}/{name}");
    let dst = format!("{local}/{name}");
    std::fs::write(&src, noise(24 * mb, 1)).unwrap();
    let out = copy(&job(host.clone(), &src, FileSource::Local, &local, false));
    assert_eq!(out.status, 0, "download: {}", out.text());
    assert!(same(&src, &dst), "a plain download lands whole");
    truncate(&dst, 8 * mb);
    let out = copy(&job(host.clone(), &src, FileSource::Local, &local, true));
    assert_eq!(out.status, 0, "resumed download: {}", out.text());
    assert!(same(&src, &dst), "a cut-short download is completed");

    // MARK: one that is there whole is left alone
    let other = format!("{local}/other.bin");
    std::fs::write(&other, noise(24 * mb, 2)).unwrap();
    std::fs::rename(&other, &dst).unwrap();
    let out = copy(&job(host.clone(), &src, FileSource::Local, &local, true));
    assert_eq!(out.status, 0, "resume over a whole copy: {}", out.text());
    assert_eq!(
        std::fs::read(&dst).unwrap(),
        noise(24 * mb, 2),
        "a whole copy is not written over by a resume"
    );
    let out = copy(&job(host.clone(), &src, FileSource::Local, &local, false));
    assert_eq!(out.status, 0, "{}", out.text());
    assert!(same(&src, &dst), "but a plain copy does write over it");

    // MARK: an upload a third of which is there
    let up_src = format!("{local}/up.bin");
    let up_dst = format!("{remote}/up.bin");
    std::fs::write(&up_src, noise(24 * mb, 3)).unwrap();
    let out = copy(&job(
        FileSource::Local,
        &up_src,
        host.clone(),
        &remote,
        false,
    ));
    assert_eq!(out.status, 0, "upload: {}", out.text());
    assert!(same(&up_src, &up_dst), "a plain upload lands whole");
    truncate(&up_dst, 8 * mb);
    let out = copy(&job(
        FileSource::Local,
        &up_src,
        host.clone(),
        &remote,
        true,
    ));
    assert_eq!(out.status, 0, "resumed upload: {}", out.text());
    assert!(same(&up_src, &up_dst), "a cut-short upload is completed");

    // MARK: a directory: one file short, one missing, one whole
    let tree = format!("{remote}/tree");
    std::fs::create_dir_all(format!("{tree}/sub")).unwrap();
    std::fs::write(format!("{tree}/a.bin"), noise(6 * mb, 4)).unwrap();
    std::fs::write(format!("{tree}/sub/b.bin"), noise(6 * mb, 5)).unwrap();
    std::fs::write(format!("{tree}/c.bin"), noise(6 * mb, 6)).unwrap();
    let out = copy(&job(host.clone(), &tree, FileSource::Local, &local, false));
    assert_eq!(out.status, 0, "directory download: {}", out.text());
    let landed = format!("{local}/tree");
    truncate(&format!("{landed}/a.bin"), 2 * mb);
    std::fs::remove_file(format!("{landed}/sub/b.bin")).unwrap();
    let out = copy(&job(host.clone(), &tree, FileSource::Local, &local, true));
    assert_eq!(out.status, 0, "resumed directory download: {}", out.text());
    for f in ["a.bin", "sub/b.bin", "c.bin"] {
        assert!(
            same(&format!("{tree}/{f}"), &format!("{landed}/{f}")),
            "{f} is whole after the resume"
        );
    }

    // MARK: the Files model: "continue" does not ask about the name
    let _ = shellder::core::prefs::update(|p| p.file_sorts.clear());
    let model: Arc<FilesModel> = files::for_host(HOST);
    model.set_hosts(vec![HOST.into()]);
    model.set_source(Pane::Left, host.clone());
    model.set_source(Pane::Right, FileSource::Local);
    std::thread::sleep(Duration::from_secs(2));
    let here = format!("{local}/here");
    std::fs::create_dir_all(&here).unwrap();
    model.set_root(Pane::Left, &remote, false);
    model.set_root(Pane::Right, &here, false);
    std::thread::sleep(Duration::from_secs(1));

    // A copy that fails (its source is not there yet) is one the list
    // offers to continue. By the time it is asked for again the source is
    // there and the destination has a third of it.
    let late = format!("{remote}/late.bin");
    model.accept(vec![(host.clone(), late.clone())], &here, Pane::Right);
    let (state, asked) = settle(&model);
    assert!(
        matches!(state, Some(TransferState::Failed { .. })),
        "a copy of what is not there fails: {state:?}"
    );
    assert!(!asked, "nothing was there, nothing to ask");
    let data = noise(24 * mb, 7);
    std::fs::write(&late, &data).unwrap();
    let dst = format!("{here}/late.bin");
    std::fs::write(&dst, &data[..8 * mb]).unwrap();
    let first = model.snapshot().transfers[0].clone();
    assert!(first.can_resume, "a copy to this machine can be continued");
    assert!(model.snapshot().can_retry.contains(&first.id));
    model.retry(first.id);
    let (state, asked) = settle(&model);
    assert_eq!(state, Some(TransferState::Done), "the continued copy lands");
    assert!(
        !asked,
        "a continued copy is not asked whether the name is taken"
    );
    let again = model.snapshot().transfers.last().cloned().unwrap();
    assert!(
        again.id != first.id && again.job.resume,
        "and it was a resume"
    );
    assert!(same(&late, &dst), "the cut-short file is whole again");

    // A fresh drop onto that name, on the other hand, asks.
    model.accept(vec![(host.clone(), late.clone())], &here, Pane::Right);
    let (state, asked) = settle(&model);
    assert!(asked, "a plain copy onto a taken name asks first");
    assert_eq!(state, Some(TransferState::Skipped));

    drop(_master);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cp_dir);
}

/// Wait for a drop to turn into a copy and for that copy to end; says what
/// it ended as and whether the window was asked about a taken name on the
/// way (the question is left unanswered, so the copy is then skipped).
fn settle(m: &FilesModel) -> (Option<TransferState>, bool) {
    let t = Instant::now();
    let mut asked = false;
    loop {
        let s = m.snapshot();
        if let Some(q) = &s.question {
            asked = true;
            m.answer_replace(q.id, files::ReplaceAnswer::Skip);
        }
        let busy = s.active.is_some()
            || s.transfers
                .iter()
                .any(|t| t.state == TransferState::Waiting)
            || s.question.is_some();
        if !busy && t.elapsed() > Duration::from_millis(500) {
            break;
        }
        assert!(
            t.elapsed() < Duration::from_secs(120),
            "the copy never ended"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
    (
        m.snapshot().transfers.last().map(|t| t.state.clone()),
        asked,
    )
}
