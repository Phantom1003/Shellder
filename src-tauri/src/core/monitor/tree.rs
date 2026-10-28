//! The Console's process tree: what runs in the session of the window's
//! active tab, read with `tree.sh`.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::super::config::READING_DEADLINE;
use super::super::remotefs::{quote, remote};
use super::script::{common, fresh_marker, join, marker, TREE};
use super::{fields, output, Sections};

/// What the Console's process tree is rooted at: the session of the
/// window's active tab. The roots come from the host: a tmux session's
/// panes (`tmux list-panes`), a screen session's own process (its id is
/// `pid.name`), a container's init (`docker inspect`; on a Docker Desktop
/// the container's processes are in a VM and not in this `ps` at all),
/// and for anything else (a plain shell, an interactive job) the ssh
/// logins of this account: the shells under an `sshd` session process.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeRoot {
    Tmux(String),
    Screen(u32),
    Container { tool: String, name: String },
    Logins,
}

impl TreeRoot {
    /// What the page asks for, by the tab's kind and name: a kind it
    /// cannot root at is the logins.
    pub fn new(kind: &str, name: &str) -> TreeRoot {
        match kind {
            "tmux" => TreeRoot::Tmux(name.to_string()),
            "screen" => match name.split_once('.').and_then(|(p, _)| p.parse().ok()) {
                Some(pid) => TreeRoot::Screen(pid),
                None => TreeRoot::Logins,
            },
            "docker" | "podman" => TreeRoot::Container {
                tool: kind.to_string(),
                name: name.to_string(),
            },
            _ => TreeRoot::Logins,
        }
    }

    /// The piece of `tree.sh` that prints the root pids under `S roots`;
    /// `logins` leaves the choice to `parse_tree`.
    fn tag(&self) -> &'static str {
        match self {
            TreeRoot::Tmux(_) => "tmux",
            TreeRoot::Screen(_) => "screen",
            TreeRoot::Container { .. } => "container",
            TreeRoot::Logins => "logins",
        }
    }

    /// The script's arguments, `$1` and on.
    fn args(&self) -> String {
        match self {
            TreeRoot::Tmux(name) => quote(name),
            TreeRoot::Screen(pid) => pid.to_string(),
            TreeRoot::Container { tool, name } => format!("{} {}", quote(name), quote(tool)),
            TreeRoot::Logins => quote(""),
        }
    }
}

/// A process of the tree, in the order the page draws them: a root,
/// then its children, each with its own, by pid.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRow {
    pub pid: u32,
    pub ppid: u32,
    /// Levels under its root: 0 for a root.
    pub depth: u32,
    pub children: u32,
    pub user: String,
    /// Its terminal, or empty.
    pub tty: String,
    /// None where `ps` has no such column (BusyBox).
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
    /// The program's name: the first word of the command line without
    /// its directory, what the signal dialog names.
    pub command: String,
    pub args: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tree {
    pub rows: Vec<TreeRow>,
}

/// The tree's script rooted at `root`, under `marker`.
fn script(root: &TreeRoot, marker: &str) -> String {
    let piece = |tag: &str| join(TREE, |t| t == tag);
    format!(
        "{} {} {} {}",
        common(marker),
        piece("me"),
        piece(root.tag()),
        piece("ps")
    )
}

fn wrapped(root: &TreeRoot, marker: &str) -> String {
    format!("sh -c '{}' sh {}", script(root, marker), root.args())
}

/// The command that reads the tree rooted at `root`, as the remote login
/// shell gets it, its sections started by the plain marker that
/// `parse_tree` reads.
pub fn tree_command(root: &TreeRoot) -> String {
    wrapped(root, &marker(""))
}

/// Read the tree rooted at `root` on `host`, given up as a reading is.
pub fn tree(host: &str, root: &TreeRoot) -> Result<Tree, String> {
    let marker = fresh_marker();
    let out = remote::shell_within(host, &wrapped(root, &marker), READING_DEADLINE);
    let text = output(out, &marker)?;
    Ok(read(&Sections::new(&text, &marker)))
}

/// The program's name in a `ps` command line: `sshd: alice@pts/0` is
/// sshd, `/usr/bin/python3 train.py` python3, `[kthreadd]` kthreadd,
/// `-zsh` (a login shell) zsh.
fn program(args: &str) -> String {
    let word = args.split_whitespace().next().unwrap_or("");
    let word = word
        .trim_end_matches(':')
        .trim_matches(|c| c == '[' || c == ']')
        .trim_start_matches('-');
    word.rsplit('/').next().unwrap_or(word).to_string()
}

fn is_sshd(args: &str) -> bool {
    program(args).starts_with("sshd")
}

/// Whether the user column of `ps` names `me`, where there is no uid to
/// go by: procps cuts a name longer than the column to `jinyanx+`,
/// BusyBox to its first eight letters.
fn same_user(column: &str, me: &str) -> bool {
    if column == me {
        return true;
    }
    let cut = column.strip_suffix('+').unwrap_or(column);
    cut.chars().count() >= 7 && me.chars().count() > cut.chars().count() && me.starts_with(cut)
}

/// The tree out of the script's output, as `tree_command` marks it.
pub fn parse_tree(text: &str) -> Tree {
    read(&Sections::new(text, &marker("")))
}

fn read(s: &Sections) -> Tree {
    let me_lines = s.get("me");
    let me = me_lines
        .first()
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    let own: Vec<u32> = me_lines
        .get(1)
        .map(|l| fields(l).iter().filter_map(|f| f.parse().ok()).collect())
        .unwrap_or_default();
    let my_uid: Option<u32> = s.get("uid").first().and_then(|l| l.trim().parse().ok());
    let root_lines = s.get("roots");
    let logins = root_lines
        .first()
        .map(|l| l.trim() == "logins")
        .unwrap_or(false);
    let mut roots: Vec<u32> = root_lines
        .iter()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .filter(|p| *p != 0)
        .collect();
    // The columns as the header names them; the command line is the last
    // one and takes the rest of the line.
    let ps = s.get("ps");
    let Some(header) = ps.first() else {
        return Tree::default();
    };
    let names = fields(header);
    let col = |want: &[&str]| names.iter().position(|n| want.contains(n));
    let (Some(ipid), Some(ippid), Some(iuser)) = (col(&["PID"]), col(&["PPID"]), col(&["USER"]))
    else {
        return Tree::default();
    };
    let iuid = col(&["UID"]);
    let itty = col(&["TT", "TTY"]);
    let icpu = col(&["%CPU"]);
    let imem = col(&["%MEM"]);
    let fixed = names.len() - 1;
    let mut procs: Vec<TreeRow> = Vec::new();
    let mut uids: HashMap<u32, u32> = HashMap::new();
    for l in &ps[1..] {
        let f = fields(l);
        if f.len() <= fixed {
            continue;
        }
        let (Ok(pid), Ok(ppid)) = (f[ipid].parse::<u32>(), f[ippid].parse::<u32>()) else {
            continue;
        };
        if let Some(uid) = iuid.and_then(|i| f[i].parse().ok()) {
            uids.insert(pid, uid);
        }
        let args = f[fixed..].join(" ");
        let tty = itty
            .map(|i| f[i])
            .filter(|t| !t.starts_with('?') && *t != "-")
            .unwrap_or("");
        procs.push(TreeRow {
            pid,
            ppid,
            depth: 0,
            children: 0,
            user: f[iuser].to_string(),
            tty: tty.to_string(),
            cpu: icpu.and_then(|i| f[i].parse().ok()),
            memory: imem.and_then(|i| f[i].parse().ok()),
            command: program(&args),
            args,
        });
    }
    let mut index: HashMap<u32, usize> = HashMap::new();
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for (i, p) in procs.iter().enumerate() {
        index.entry(p.pid).or_insert(i);
        if p.ppid != p.pid {
            kids.entry(p.ppid).or_default().push(p.pid);
        }
    }
    for list in kids.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    let by_pid = |pid: u32| index.get(&pid).map(|&i| &procs[i]);
    if logins {
        // This account's processes started by an sshd session process:
        // the login shells, a session's `sh`. By uid where `ps` gave one.
        let mine = |p: &TreeRow| match (my_uid, uids.get(&p.pid)) {
            (Some(me), Some(uid)) => me == *uid,
            _ => same_user(&p.user, &me),
        };
        roots = procs
            .iter()
            .filter(|p| mine(p) && !is_sshd(&p.args))
            .filter(|p| by_pid(p.ppid).is_some_and(|q| is_sshd(&q.args)))
            .map(|p| p.pid)
            .collect();
    }
    // The script's own shell and whatever the login shell forked to run
    // it are not the session's doing; the sshd process it hangs from is
    // everyone's parent and stays.
    let skip: HashSet<u32> = own
        .iter()
        .copied()
        .filter(|p| by_pid(*p).is_some_and(|q| !is_sshd(&q.args)))
        .collect();
    let mut rows: Vec<TreeRow> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    // Depth first, a root and everything under it before the next, the
    // children by pid; a stack rather than recursion, however deep the
    // host's tree goes.
    let mut stack: Vec<(u32, u32)> = roots.iter().rev().map(|&r| (r, 0)).collect();
    while let Some((pid, depth)) = stack.pop() {
        if skip.contains(&pid) || seen.contains(&pid) {
            continue;
        }
        let Some(p) = by_pid(pid) else {
            continue;
        };
        seen.insert(pid);
        let children: Vec<u32> = kids
            .get(&pid)
            .map(|list| list.iter().copied().filter(|c| !skip.contains(c)).collect())
            .unwrap_or_default();
        rows.push(TreeRow {
            depth,
            children: children.len() as u32,
            ..p.clone()
        });
        stack.extend(children.iter().rev().map(|&c| (c, depth + 1)));
    }
    Tree { rows }
}

/// Every script of this file, for the tests of what a script may carry.
#[cfg(test)]
pub(super) fn every_script() -> Vec<String> {
    [
        TreeRoot::Tmux("x".into()),
        TreeRoot::Screen(1),
        TreeRoot::Container {
            tool: "docker".into(),
            name: "x".into(),
        },
        TreeRoot::Logins,
    ]
    .iter()
    .map(|r| script(r, &fresh_marker()))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE_LINUX: &str = include_str!("fixtures/tree-linux.txt");

    #[test]
    fn the_tree_of_the_logins_is_the_shells_under_sshd() {
        let t = parse_tree(TREE_LINUX);
        let rows: Vec<(u32, u32, &str)> = t
            .rows
            .iter()
            .map(|r| (r.pid, r.depth, r.command.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (950, 0, "zsh"),
                (960, 1, "python3"),
                (970, 0, "zsh"),
                (980, 1, "vim")
            ],
            "alice's two login shells and what runs in them: not bob's, not the tmux panes, not the script's own shells"
        );
        assert_eq!(t.rows[0].children, 1);
        assert_eq!(t.rows[0].tty, "pts/0");
        assert_eq!(t.rows[1].cpu, Some(95.0));
        assert_eq!(t.rows[1].memory, Some(12.5));
        assert_eq!(t.rows[1].args, "python3 train.py --epochs 3");
    }

    #[test]
    fn the_tree_of_a_tmux_session_hangs_from_its_panes() {
        let text = TREE_LINUX.replace("logins", "1210\n1230\n");
        let t = parse_tree(&text);
        let rows: Vec<(u32, u32)> = t.rows.iter().map(|r| (r.pid, r.depth)).collect();
        assert_eq!(rows, vec![(1210, 0), (1220, 1), (1230, 0)]);
        let gone = TREE_LINUX.replace("logins", "");
        assert!(
            parse_tree(&gone).rows.is_empty(),
            "a session that is gone has no panes"
        );
    }

    /// A name longer than procps's user column (`averyve+`) or BusyBox's
    /// (`averyver`): the uid says whose a process is, and without one the
    /// cut name still matches, but not another user's short one.
    #[test]
    fn a_long_user_name_still_finds_its_logins() {
        let procps = "
@@shellder-console:me
averyverylongusername
9100 9099
@@shellder-console:uid
1000
@@shellder-console:roots
logins
@@shellder-console:ps
    PID    PPID   UID USER     TT       %CPU %MEM COMMAND
      1       0     0 root     ?         0.0  0.1 /sbin/init
    800       1     0 root     ?         0.0  0.2 sshd: /usr/sbin/sshd -D
    901     800  1000 averyve+ ?         0.0  0.1 sshd: averyverylongusername@pts/0
    950     901  1000 averyve+ pts/0     0.0  0.2 -bash
    960     950  1000 averyve+ pts/0     1.0  0.3 vim
   2000     800  1001 averyve+ ?         0.0  0.1 sshd: averyverylongothername@pts/1
   2010    2000  1001 averyve+ pts/1     0.0  0.2 -bash
   9099     901  1000 averyve+ ?         0.0  0.1 bash -c sh -c ...
   9100    9099  1000 averyve+ ?         0.0  0.1 sh -c S(){ ...
";
        let pids = |t: Tree| t.rows.iter().map(|r| r.pid).collect::<Vec<_>>();
        assert_eq!(
            pids(parse_tree(procps)),
            vec![950, 960],
            "by uid: not the other account whose name is cut the same"
        );
        let busybox = "
@@shellder-console:me
averyverylongusername
9100 9099
@@shellder-console:roots
logins
@@shellder-console:ps
PID   PPID  USER     TT     COMMAND
    1     0 root     ?      /usr/sbin/sshd -D -e
    5     1 averyver ?      sshd: averyverylongusername@pts/0
    6     5 averyver pts/0  -ash
    7     5 bob      pts/1  -ash
 9099     6 averyver pts/0  sh -c sh -c ...
 9100  9099 averyver pts/0  sh -c S(){ ...
";
        assert_eq!(pids(parse_tree(busybox)), vec![6]);
        assert!(same_user("averyve+", "averyverylongusername"));
        assert!(same_user("averyver", "averyverylongusername"));
        assert!(same_user("alice", "alice"));
        assert!(
            !same_user("alice", "alicebob"),
            "not cut: a name of its own"
        );
        assert!(!same_user("averyver", "averyve"));
        assert!(!same_user("bob+", "bobby"));
    }

    /// What the script printed over ssh for an account with a long name
    /// and a login besides: Debian's procps, where another long name is
    /// cut to the same `averyve+` and the uid tells them apart, and
    /// Alpine's BusyBox, `averyver` and no uid column.
    #[test]
    fn long_names_on_real_hosts() {
        let rows = |t: Tree| t.rows.iter().map(|r| (r.pid, r.depth)).collect::<Vec<_>>();
        let debian = parse_tree(include_str!("fixtures/tree-debian.txt"));
        assert_eq!(rows(debian), vec![(1073, 0), (1075, 1)]);
        let alpine = parse_tree(include_str!("fixtures/tree-alpine.txt"));
        assert_eq!(rows(alpine.clone()), vec![(30, 0)]);
        assert_eq!(alpine.rows[0].user, "averyver");
    }

    #[test]
    fn the_tree_reads_the_columns_the_header_names() {
        let text = "
@@shellder-console:me
t
7 1
@@shellder-console:roots
logins
@@shellder-console:ps
PID   PPID  USER     TT     COMMAND
    1     0 root     ?      /usr/sbin/sshd -D -e
    5     1 t        ?      sshd: t@pts/0
    6     5 t        pts/0  -ash
    7     6 t        pts/0  sh -c S(){ ...
    8     7 t        pts/0  ps -eo pid,ppid,user,tty,args
";
        let t = parse_tree(text);
        assert_eq!(
            t.rows.len(),
            1,
            "BusyBox: no cpu column, the script's own shell left out"
        );
        assert_eq!(
            (t.rows[0].pid, t.rows[0].cpu, t.rows[0].memory),
            (6, None, None)
        );
        let mac = "
@@shellder-console:me
alice
30 20
@@shellder-console:uid
501
@@shellder-console:roots
logins
@@shellder-console:ps
  PID  PPID   UID USER             TTY       %CPU %MEM ARGS
    1     0     0 root             ??         0.0  0.0 /sbin/launchd
   10     1     0 root             ??         0.0  0.0 /usr/sbin/sshd -i
   20    10   501 alice            ??         0.0  0.0 sshd-session: alice@ttys001
   25    20   501 alice            ttys001    0.0  0.0 -zsh
   30    20   501 alice            ??         0.0  0.0 sh -c S(){ ...
";
        let t = parse_tree(mac);
        assert_eq!(t.rows.len(), 1);
        assert_eq!((t.rows[0].pid, t.rows[0].tty.as_str()), (25, "ttys001"));
        let tmux = tree_command(&TreeRoot::new("tmux", "it's"));
        assert!(tmux.starts_with("sh -c 'S(){"));
        assert!(tmux.contains("tmux list-panes -s -t \"=$1\" -F \"#{pane_pid}\""));
        assert!(tmux.ends_with("' sh 'it'\\''s'"), "{tmux}");
        assert!(tree_command(&TreeRoot::new("podman", "web"))
            .ends_with("\"$2\" inspect -f \"{{.State.Pid}}\" \"$1\" 2>/dev/null; S ps; ps -eo pid,ppid,uid,user,tty,pcpu,pmem,args 2>/dev/null || ps -eo pid,ppid,user,tty,pcpu,pmem,args 2>/dev/null || ps -eo pid,ppid,user,tty,args 2>/dev/null; true' sh 'web' 'podman'"));
        assert!(tree_command(&TreeRoot::Screen(4242)).ends_with("echo \"$1\"; S ps; ps -eo pid,ppid,uid,user,tty,pcpu,pmem,args 2>/dev/null || ps -eo pid,ppid,user,tty,pcpu,pmem,args 2>/dev/null || ps -eo pid,ppid,user,tty,args 2>/dev/null; true' sh 4242"));
        assert_eq!(TreeRoot::new("screen", "4242.work"), TreeRoot::Screen(4242));
        assert_eq!(TreeRoot::new("shell", ""), TreeRoot::Logins);
        assert_eq!(TreeRoot::new("preview", "/etc/hosts"), TreeRoot::Logins);
    }

    /// A long chain and a loop in what `ps` said: every process once, in
    /// a time that does not grow with the square of the table.
    #[test]
    fn a_deep_tree_and_a_loop() {
        let mut text = String::from(
            "@@shellder-console:me\nme\n1 1\n@@shellder-console:roots\n10\n@@shellder-console:ps\nPID PPID USER TT COMMAND\n",
        );
        let n = 50_000;
        for pid in 10..10 + n {
            text.push_str(&format!("{pid} {} me ? sh\n", pid - 1));
        }
        // 10's parent is the last of the chain: a loop.
        text.push_str(&format!("9 {} me ? sh\n", 9 + n));
        let t = parse_tree(&text);
        assert_eq!(t.rows.len(), n as usize + 1);
        assert_eq!(t.rows.last().map(|r| r.depth), Some(n));
    }
}
