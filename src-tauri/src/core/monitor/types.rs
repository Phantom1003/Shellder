//! What a reading of a host is, as the Console page gets it.

use serde::Serialize;

/// Ticks of `/proc/stat`'s first line: the window takes the difference
/// between two snapshots for a percentage, since a single reading only
/// says what happened since boot. `total` is user, nice, system, idle,
/// iowait, irq, softirq and steal; the guest ticks the line ends with
/// are already inside user and nice, so they are not added again.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CpuTicks {
    pub busy: u64,
    pub total: u64,
}

/// Something that fills up: bytes used of bytes there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Gauge {
    pub total: u64,
    pub used: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub filesystem: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    /// On a removable drive (USB, a card, a disc): it can be unmounted
    /// and the drive ejected from the Console.
    pub removable: bool,
    /// The drive it is on (`/dev/sdb`, `/dev/disk4`), what an eject takes.
    pub drive: Option<String>,
}

/// A volume on a removable drive that is not mounted: a row with a
/// mount button.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub device: String,
    pub drive: Option<String>,
    pub label: String,
    /// What is on it as the host names it (`vfat`, `APFS Volume`).
    pub fstype: String,
    pub size: u64,
}

/// An area of swap on a Linux host, as `/proc/swaps` lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapArea {
    pub path: String,
    /// `file` or `partition`.
    pub kind: String,
    pub size: u64,
    pub used: u64,
    /// One the Console made (`SWAP_MARK` in its name): it has a button
    /// that removes it.
    pub temporary: bool,
}

/// A network interface with an address: its traffic counters, whose
/// difference between two readings is the rate the page shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub name: String,
    /// IPv4 and global IPv6, with the prefix length; loopback and
    /// link-local addresses are left out.
    pub addresses: Vec<String>,
    /// Bytes since boot, when the host counts.
    pub received: Option<u64>,
    pub sent: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub index: u32,
    pub name: String,
    /// Percent busy, as nvidia-smi reports it.
    pub utilization: Option<f64>,
    pub memory: Option<Gauge>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxWindow {
    pub index: u32,
    pub name: String,
    pub active: bool,
    /// What runs in the active pane, and where.
    pub command: String,
    pub path: String,
    pub panes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxSession {
    pub name: String,
    /// Unix time.
    pub activity: Option<f64>,
    /// How many clients are attached to it (one of them may be this
    /// window's).
    pub attached: u32,
    pub windows: Vec<TmuxWindow>,
    /// The active window's current directory.
    pub path: String,
}

/// A GNU screen session, as `screen -ls` lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSession {
    /// `pid.name`: what `screen -x` is given, unambiguous.
    pub id: String,
    pub name: String,
    pub pid: u32,
    pub attached: bool,
    /// The current directory of the shell in it (Linux, through /proc).
    pub path: String,
}

/// A VNC server running on the host: an Xvnc, Xtigervnc, x11vnc… process,
/// or macOS's Screen Sharing (Remote Management serves the same), which
/// launchd listens for and starts when a viewer connects.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VncSession {
    /// Empty for Screen Sharing: it is the system's, not a user's.
    pub user: String,
    /// None for Screen Sharing: no process until a viewer connects.
    pub pid: Option<u32>,
    /// The program, without its path; "Screen Sharing" on macOS.
    pub program: String,
    /// The X display it serves (`:1`), when it says.
    pub display: Option<u32>,
    /// Where a viewer connects: `-rfbport`, or 5900 plus the display.
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    /// The host side of its bind mounts: where its files are on the host.
    pub mounts: Vec<String>,
}

/// The containers, and which tool lists them (`docker` or `podman`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Containers {
    pub tool: String,
    pub list: Vec<Container>,
}

/// A Slurm job of the user, from `squeue`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlurmJob {
    pub id: String,
    pub name: String,
    /// RUNNING, PENDING, …
    pub state: String,
    /// Time used so far, as squeue writes it.
    pub time: String,
    pub nodes: u32,
    /// The node list of a running job, the reason of a waiting one.
    pub reason: String,
    pub partition: String,
    /// The job's working directory (`%Z`).
    pub path: String,
}

/// A PBS (OpenPBS, PBS Pro, Torque) job of the user, from `qstat -f`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PbsJob {
    pub id: String,
    pub name: String,
    /// R, Q, H, E, …
    pub state: String,
    /// resources_used.walltime, empty before it runs.
    pub time: String,
    pub nodes: u32,
    /// The first node of exec_host, where a shell for the job goes.
    pub node: String,
    pub queue: String,
    /// Where it was submitted from: `init_work_dir` (Torque), or
    /// `PBS_O_WORKDIR` in its `Variable_List`.
    pub path: String,
}

/// A TCP port something on the host listens on: what a port forward
/// reaches.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub port: u16,
    /// What it is bound to: `0.0.0.0`, `127.0.0.1`, `*`, `::`…
    pub address: String,
    /// The program, when the host says (ss and netstat only name the
    /// user's own).
    pub process: String,
    pub pid: Option<u32>,
}

/// Someone logged in: a line of `who`, or a session of logind.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub user: String,
    /// The terminal; none for logind's ssh sessions.
    pub tty: String,
    /// When, the date in the local form of the host.
    pub since: String,
    /// Where from, when the host says.
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: u32,
    pub user: String,
    pub cpu: f64,
    pub memory: f64,
    pub command: String,
}

/// A reading of a host. What a part left unread (`PARTS`, `TAB_PARTS`)
/// would have filled stays at its default: empty, or None.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub hostname: String,
    /// "Ubuntu 24.04.1 LTS", "macOS 15.1", or what `uname` says.
    pub os: String,
    pub kernel: String,
    /// Seconds since boot.
    pub uptime: Option<f64>,
    pub load: Option<[f64; 3]>,
    pub cpus: Option<u32>,
    pub cpu_ticks: Option<CpuTicks>,
    /// A percentage the host gave outright (macOS's `top`), when there are
    /// no ticks to difference.
    pub cpu_percent: Option<f64>,
    pub memory: Option<Gauge>,
    pub swap: Option<Gauge>,
    /// The swap areas of a Linux host; None where swap cannot be added
    /// by hand (macOS makes its own).
    pub swap_files: Option<Vec<SwapArea>>,
    pub disks: Vec<Disk>,
    /// Volumes on removable drives that are not mounted.
    pub volumes: Vec<Volume>,
    pub network: Vec<Interface>,
    pub gpus: Vec<Gpu>,
    /// None: no tmux on the host (or the sessions not read, see
    /// `read_parts`). Some(empty): tmux, but no session.
    pub tmux: Option<Vec<TmuxSession>>,
    /// The same for GNU screen.
    pub screen: Option<Vec<ScreenSession>>,
    /// None: neither docker nor podman on the host.
    pub docker: Option<Containers>,
    /// None: no Slurm client on the host.
    pub slurm: Option<Vec<SlurmJob>>,
    /// None: no PBS client on the host.
    pub pbs: Option<Vec<PbsJob>>,
    pub vnc: Vec<VncSession>,
    pub processes: Vec<Process>,
    pub logins: Vec<Login>,
    pub users: Option<u32>,
    /// The logins are logind's sessions, one an ssh connection (the
    /// app's own master too), not utmp's terminals.
    pub login_sessions: bool,
    pub listeners: Vec<Listener>,
    /// Which of the tabs' parts (`TAB_PARTS`) this reading has: the
    /// fields of one not among them say nothing about the host.
    pub read_parts: Vec<String>,
}
