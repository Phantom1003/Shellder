# Sessions, jobs and containers

Three panels of the side bar list things on the host that a terminal can be opened in: the Sessions panel has the tmux and screen sessions, the Jobs panel your Slurm and PBS jobs, the Containers panel the Docker or Podman containers. A click on a row opens it in a terminal tab, or brings up the tab it is already in.

{{#include figures/console-sessions.html}}

## The Sessions panel

The panel has a section for each tool the host has, tmux first and then GNU screen. Each section title carries a + for a new session, and under it a session row for each session.

### Session rows

A tmux session row shows the session's name, how many windows it has, how many clients are attached and when it was last active, with a window chip for each of its windows. A screen session row shows the session's name, its pid and whether it is attached.

Each row also shows the session's current directory. For tmux it is the active window's. For screen it is the shell's, read from `/proc`, so on Linux only.

| Control | What it does |
|---|---|
| Click on the row | Attaches the session in the temporary tab (`tmux attach`, `screen -x`), or brings up the tab it is already in. |
| Double click on the row | Attaches the session in a kept tab. |
| Folder button | Shows the session's directory in the [Files panel](console-files.md). |
| Terminal button | Opens a shell of the host in that directory, in a tab named after it. |
| Pencil | Gives the session another name (`tmux rename-session`, `screen -X sessionname`). |
| Cross | Ends the session, with everything in it (`tmux kill-session`, `screen -X quit`), after asking. |

### Temporary and kept tabs

A session opens the way a file of the Files panel does:

- A single click uses the temporary tab of the group that has the keyboard focus. Its name is in italics.
- The next session clicked takes that tab over. The client in it detaches; the session it showed lives on.
- A double click on the row, or on the tab itself, keeps the tab.

Closing a session tab only detaches the client. More about tabs is in [Tabs, groups and splits](console-tabs.md).

### A new session

The + on the tmux or screen section title opens the new-session dialog. It asks for three things, all optional:

| Field | What it is |
|---|---|
| *Name* | The session's name. With none, the tool numbers the session. |
| *Directory* | The directory the session starts in; the home directory when empty. The folder button picks one from the folder picker, a tree of the host's folders (for *Local*, this machine's own folder dialog). |
| *Command* | A command to run in the session instead of the shell. The session ends when the command does; `cmd; exec $SHELL` keeps a shell after it. |

*Open* makes the session and attaches it in a new tab.

## The Jobs panel

The Jobs panel is there where the host has a Slurm or PBS client. It lists your jobs, a job row each.

| Job | A click opens |
|---|---|
| A running Slurm job | A shell on its nodes (`srun --jobid … --overlap --pty bash`). |
| A running PBS job | A shell on its first node (`ssh` from the login node). |
| A queued job | Nothing: only a running job can be entered. |

Each job row shows the job's working directory (Slurm's `WorkDir`, PBS's `PBS_O_WORKDIR`), queued jobs too, with the same folder and terminal buttons as a session row.

The + on the panel's title starts an interactive job in a tab: `srun --pty bash` for Slurm, `qsub -I` for PBS. Where the host has both schedulers, it is a menu of the two.

## The Containers panel

The Containers panel is there where the host has Docker or Podman. It lists the containers, a container row each. A click on a row opens a shell inside the container (`docker exec -it`).

A container's bind mounts are listed on its row by their host side, with the folder and terminal buttons: the folder button shows the directory in the Files panel, the terminal button opens a shell on the host there. With several bind mounts, each button opens a menu of them.
