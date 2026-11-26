# Notes for agents working on Shellder

Shellder is a desktop app plus CLI (one binary, built with Tauri 2 from
`src-tauri/`, the pages in `ui/`) that keeps `ssh -M` ControlMaster
connections alive and answers ssh's prompts itself: every ssh it starts (a
master, a Console window's terminal, a Files window's session) gets a
pseudo-terminal as its controlling terminal, and the questions ssh writes to
`/dev/tty` are answered on that pty. Secrets live in one keychain item, the
"shellder vault" (the login keychain on macOS, the Secret Service on Linux).
The user manual is `docs/manual.md`. macOS and Linux are the targets and
build. A host without a ControlPath, or any host where `core::platform`
says the OpenSSH has no ControlMaster (Windows), gets the same master
without `-M` and without a socket (`ssh::multiplexed`): kept alive for its
port forwards, established when ssh prints "Authenticated to host"
(LogLevel VERBOSE) instead of when `ssh -O check` answers, no health check
but its own process. Console and Files windows cannot go through it and
log in on their own. The Windows build itself is not done
(the Unix pty, `flock`, signals and `ps` in `core` need their ConPTY and
Win32 counterparts), nor is a mobile build (no ssh binary to run there; it
would need an in-process ssh client).

## Layout

* `src-tauri/src/core/`: everything below the windows, no Tauri in it.
  `config` (paths, tunables, the deadlines), `log`, `prefs` (one JSON file,
  written by one writer at a time; one that does not parse is kept as
  `prefs.json.unreadable` by the next write), `catalog`
  (ssh config parsing, `ssh -G`), `platform` (what this OS's ssh can do),
  `ssh` (running ssh/scp, `spawn_asking` which gives a process its asking
  pty, the master), `pty` (the controlling terminal and the reader that
  answers questions), `prompt` (what a question is, and its `Origin`:
  master, terminal, session, or an action of the Console run on a
  terminal of its own, `ssh::run_on_terminal`, so that the host's own
  question on it, sudo's or polkit's, is answered like ssh's: never in
  a tab), `daemon` (one thread, the switch/lock
  semantics), `reconnect` (the rule a locked host is brought back by: the
  waits between tries, fixed or doubling, and the give-up; a default rule
  in the prefs and a host's own, each a `Partial` whose unset fields fall
  back on the rule below; the daemon reads it afresh at every failure and
  `ui/reconnect.js` draws the rows both Settings and the host's page
  edit it with), `term` (a Console window's terminal, `ssh -t host tmux
  attach`: a second pty for its screen; a tmux the Console starts is
  told it takes links, `-T hyperlinks`, and turns `allow-passthrough`
  on when it is off, since tmux keeps a program's OSC 9/777/99
  notifications to itself otherwise), `monitor` (the Console's
  readings, tmux/screen/VNC sessions included: one shell script, its
  output parsed; a directory, the scripts in `.sh` files embedded with
  `include_str!` and joined into one line, each line tagged with the part
  it reads: a card of the Status tab (`PARTS`) or a list of the other
  tabs (`TAB_PARTS`), read only when the page names it; a reading is
  given up after `config::READING_DEADLINE`),
  `session` (a host's `ssh host sh` kept open, the Files listings when no
  master is up; callers that come during its login wait for that login
  instead of starting their own), `keychain`, `totp`, `autostart` (LaunchAgent / XDG autostart),
  `remotefs` (`ls` through the master or the session, this machine's
  files; a command through the master is `ssh -T host sh` with the
  command on its stdin, never given to the login shell, which may be a
  tcsh or a fish), `transfer` (one sftp/scp with progress, resumable; it logs in on
  its own, with an asking pty, when no master is up; a resumed upload
  uses `put -a` only on a file cut short, since sftp fails it on one
  missing or whole, and is copied afresh otherwise), `shellint` (the
  shell integration: the scripts in `src-tauri/shell-integration/`, one
  each for bash, zsh and fish and `setup.sh` that installs them and
  starts the shell, embedded with `include_str!`; a host gets them
  base64 in a one-line `sh -c` ssh command, this machine from the state
  directory; `term::Launch` says whether a terminal runs a command or a
  shell, in which directory, with or without them).
* `src-tauri/src/app/`: `model` (the state every window draws from, and the
  `PromptHandler` that answers ssh; `TerminalView` spells out the
  terminals' settings with their defaults), `files` (the Files model, one
  per host: the Files window's two panes, `left` and `right`, and a
  third, `tab`, that is a Console's Files tab's own, so the folder shown,
  the folders open, the selection and the order of one are not the
  other's; a view opening starts its own panes only (`files_open` with
  `tab`); what is the host's is shared: the copies and their questions,
  and a directory read again in every pane that shows it; the Console's
  uploads and downloads name both ends (`copy`, `files_upload`,
  `files_download`), no pane pointed at a folder for them, and its
  button for the window sends the window's pane on the host to the
  tab's folder, `show_in_window`),
  `terminals` (the Console windows' terminals, by window label; each
  one's reader waits for its page, `Flow`), `commands`
  (what the pages may call), `windows` (the windows with terminals, a
  Console's and the quick terminal, are always built transparent and
  unpainted, the page painting what is opaque, and take the click that
  brings them forward (`accept_first_mouse`), which console.js's
  `watchFirstClick` turns into the keyboard for the terminal under it
  and swallows anywhere else (AppKit kept that click, and WebKit gave
  the keyboard back to the column or a field, so a click on a terminal
  took two), and
  `apply_terminal_effects` puts the macOS blur on or off live: the window
  server's `CGSSetWindowBackgroundBlurRadius`, private but what every
  terminal with the look uses, not an AppKit material, which tints and
  all but hides the backdrop in the light appearance, and
  `match_corners` sets such a window's corner radius again through
  AppKit's private setter, since on macOS 26 the window server clips the
  pixels to the system corner but blurs within the old one, a sliver of
  blurred backdrop showing outside a see-through terminal's bottom
  corners otherwise;
  `toggle_quick_terminal` is the global shortcut's: the
  `quick` window, the Console page with `quick=1`, undecorated and
  always on top, sized to the pointer's screen and slid in from its
  top, centred and sized as last dragged (`quick_width`,
  `quick_height` in the prefs, shares of the screen), hidden rather
  than closed, by Esc at the prompt (`quick_hide`) or on losing focus;
  put away with the keyboard, it first makes the application it came
  over active again and orders out once ours has let go
  (`hide_quick`, `put_away_quick`), since a window of an active app put
  away has AppKit bring another of its windows forward, the main
  window over that application;
  a window of ours that takes the keyboard just as it comes (the app's
  activation handing it to the Console that had it last) gives it back
  (`quick_keeps_keyboard`); the window is built shown and key, just
  above the screen's edge where nothing of it is seen (`quick_place`,
  `build_unpainted` with `shown`), since putting the web view into a
  window makes the app active (wry does), and an app made active with
  no key window has AppKit key the Console that had the keyboard last
  and bring it forward over the application the key was pressed in;
  its terminals' background is the one last
  chosen with a Console group's button (`console-term-theme` in
  localStorage);
  `console_showing` skips it, or switching a Console to Local would
  find it), `tray`, `updater`, `notices` (the Console's
  notifications of the system's: each carries its window label and tab
  number, a click brings that tab up through `windows::bring_up` and
  the page's `notice-clicked`, a tab looked at takes its own away
  (`notices_seen`), and no more than a dozen are left standing; on
  macOS they are `NSUserNotification`s with a delegate of the app's
  (`ShellderNoticeDelegate`), since the notification plugin reports no
  click on one without buttons, and they show as banners with the app
  in front too; on Linux they go through the plugin, whose click is the
  `default` action),
  `sound` (the bell's and the notifications' sound: a file, the
  system's alert sound for none; `sounds` lists the system's, the
  macOS Sounds folders or the Linux sound themes; on macOS a file of a
  Sounds folder goes to the notification centre by name, played with
  the notification and quiet in a Focus, any other is played by the
  app with `NSSound`, on Linux by the first of `pw-play`, `paplay`,
  `canberra-gtk-play`, `aplay` found). The
  quick terminal's key is
  registered in `app/mod.rs` (`apply_quick_terminal`) at start and when
  its own settings change (a refusal shows in the settings window as
  `quickKeyError`); `disable_press_and_hold` there sets
  `ApplePressAndHoldEnabled` off for the app, so a held key repeats.
  A Console page's close goes through its own `onCloseRequested`
  (asking about running commands), which ends in `destroy()`:
  `core:window:allow-destroy` must stay in the capability, or no
  Console window closes.
* `src-tauri/src/cli.rs`: the subcommands. `src/main.rs` picks the mode.
* `src-tauri/capabilities/default.json` names the window labels (`main`,
  `files-*`, `console-*`, …) that may call the app at all: a window with a
  label it does not cover gets every `invoke` refused and its page stays
  blank, so a new kind of window goes in there first.
* `ui/`: plain HTML/JS/CSS, no bundler, no npm. `shellder.js` is the glue
  (`invoke`, events, `t()` translations, DOM helpers, icons, popup menus);
  one page per window (`index`, `settings`, `prompt`, `files`, `console`);
  `filesview.js` is the file trees and copies strip that the Files window
  (two panes) and the Console's Files tab (one pane, the model's `tab`)
  both draw (a row's
  *Rename…* is the model's `rename`: never over a name that is taken,
  the panes going on under the new path, and `files-renamed` sent to
  every window, which a Console's preview tabs follow);
  `ports.js` is a host's forwards and listed ports, drawn by the Console's
  Ports tab and the main window's Tools card (the same rows, kept in
  localStorage per host, which every window shares);
  `preview.js` is the Console's preview tab for a file of that tree (text,
  Markdown, a web page in a sandboxed frame, PDF, image, video, audio;
  the bytes come through `files_read`, base64 over the master or the
  session, raw over the IPC, up to a limit per kind; a tab on screen
  follows its file through `files_stamp`, inode, size and time, and
  draws a change into a box laid over the old one unseen until it is
  at the reader's place; ⌘F searches a text, a Markdown page or a PDF,
  every match kept as a number, its place in the text, and the ones
  in view marked with the CSS Highlight API, ranges that leave the
  content as it is; a PDF's are found in pdf.js's text of every page and
  marked on the pages whose words are laid out; a copy from a PDF goes
  through `pdfcopy.js`: the lines read from pdf.js's words with where
  they lie, the ones that begin a paragraph told by that, the rest
  joined to the line before, a word cut at a line's end put together
  unless the pages the copy is from have it with its hyphen inside a
  line (their words as laid out: nothing is read for a copy)). Translations
  are `ui/lang/<lang>.json`, keyed by the English text, `{name}`
  placeholders. `ui/icons.js` is generated by `scripts/icons.py` from
  Lucide. `ui/themes.js` is the terminals' palettes, the built-in ones
  and the user's theme files (`<state>/themes/*`, Ghostty's format,
  read by `terminal_themes`, written by `theme_write`, renamed by
  `theme_rename` and deleted by `theme_delete`, which emit
  `themes-changed` to every window so the Consoles read them again); `ui/consoleprefs.js` the Console's
  settings window (`termprefs` label, the gear in a Console's bar),
  whose pages are `ui/termprefs.js`'s (every row a `set_setting` of
  a `terminal.*` key or the quick terminal's; `themePage` is the Theme
  page, the palettes listed with swatches beside the one chosen colour
  by colour, a colour changed written to its theme file, a built-in
  palette copied to a file first) and `keys.js`'s `keysView`. `ui/vendor/` is
  checked in with each licence: xterm.js (MIT) and
  its addons as published on npm (`@xterm/xterm`,
  `addon-unicode11`, `addon-clipboard` (OSC 52, given a write-only
  provider: a host must never be able to read the clipboard),
  `addon-web-links`,
  `addon-search`, `addon-webgl` (loaded by console.js the first time a
  terminal wants WebGL; its blendFunc is wrapped there
  to blend the alpha channel with ONE / ONE_MINUS_SRC_ALPHA: as the
  addon has it, SRC_ALPHA for alpha too, a see-through light background
  came out solid white and every glyph edge on a coloured cell got a
  pale fringe, since the canvas pixels fell short of their alpha and
  WebKit composites the canvas as premultiplied; on a one-to-one display
  the page also reports a pixel ratio of two so the glyph atlas is drawn
  large and scaled down, see `REAL_DPR`); the UMD builds, so a `<script>`
  tag is
  enough; take a newer one from
  `https://cdn.jsdelivr.net/npm/@xterm/xterm@VERSION/lib/xterm.js`,
  `css/xterm.css` and `@xterm/addon-NAME@VERSION/lib/addon-NAME.js`, the
  addons' versions being the ones published with that xterm);
  marked (MIT, `marked@VERSION/lib/marked.esm.js`), whose HTML
  `preview.js` rebuilds element by element from a list of tags it keeps,
  since a file from a host must not run anything in a window that can
  call the app; highlight.js (BSD-3-Clause,
  `@highlightjs/cdn-assets@VERSION/es/highlight.min.js`, the ES module
  with the common languages, and `es/languages/NAME.min.js` for
  dockerfile, nginx and apache as `highlight-NAME.min.js`, each loaded
  with `import()` when a file of that language is shown; its HTML is
  rebuilt the way marked's is, text and spans with its classes); and pdf.js (Apache-2.0, the legacy build for the oldest
  WebKit the app runs on: `pdfjs-dist@VERSION/legacy/build/pdf.min.mjs`
  and `pdf.worker.min.mjs`, both from the same version, loaded with
  `import()` only when a PDF is shown; a document is let go with its
  loading task's `destroy()`: this version's document has none, and
  calling it once kept a closed tab's button on screen).
* `scripts/`: `icon-source.png` (the app icon's source), `icons.py`,
  `menubar-template` (a small Rust crate that makes the menu bar picture),
  `latest-json.sh` (the updater's manifest, used by the release job).
* `Tests/`: shell scripts that need Docker, see below. `docs/`: the manual
  and its pictures, and the documentation site around them:
  `docs/build.sh` builds it into `build/site` (`--serve` serves it under
  `/Shellder/`, its path on GitHub Pages; `--api` adds rustdoc's
  reference of the crate, private items included, under `api/`;
  `--front` builds the front page and its demo alone, without mdBook),
  and `.github/workflows/docs.yml` publishes what is on `main`. The site
  is the front page `docs/site/` (plain HTML and CSS, both languages in
  the one page, an element with a `lang` showing in its language only;
  dressed in `ui/app.css`'s colours) and its demo, which is most of the
  page: the app's own pages, not pictures of them. The build copies
  `ui/` to `app/` and puts `docs/site/demo/shim.js` in front of every
  page there, a `window.__TAURI__` whose calls go to
  `docs/site/demo/backend.js`: hosts whose switch connects after a
  moment (`db-staging` never does, and is tried again while locked),
  readings that move, a file tree with copies that fill a bar, the log,
  and a shell of a handful of commands that sends the shell
  integration's marks, so the tab marks, the progress bar and the
  password offer are the page's own doing. It starts from
  `docs/site/demo/data.js`, written by `docs/shots/shoot.py --demo`
  from the same data as the pictures, never by hand. The backend is
  the front page's, one for all windows (a page of `app/` opened on
  its own loads one for itself); what crosses between it and a page is
  copied, and an event is sent after the call that caused it, as over
  the app's IPC: a page told of its own click while it handles the
  mouse coming down redraws under it. `docs/site/demo/desk.js` is the
  desk the windows stand on, each a frame at the size the app gives
  the window, the one clicked in front, drawn 1200 points wide and
  scaled down on a narrower page; on a Mac the pages get the `mac`
  class and the desk draws the traffic lights, elsewhere a title bar,
  and the keys are the visitor's platform's (`keys.js`). Its "Show
  me" buttons do a step in the Console by its own buttons and keys.
  A browser keeps ⌘T, ⌘W and the like for itself, so the page names
  no step that needs them. Then
  two mdBook books, `docs/en` and `docs/zh`, a page having the same file
  name in both: the button `docs/theme/lang.js` puts in a book's bar
  goes to the other language's page of that name, to that book's first
  page when there is none. The books' pages are one per window or
  feature, written from `docs/manual.md` (which is in neither book) in
  the names of `docs/terms.py`: the one list of what every part of the
  app is called, in both languages, from which `python3 docs/terms.py`
  writes the two Terms pages; a page names a part by its term there and
  by no other word (*tab* is what a Console's right side holds, what
  its side bar shows is a *panel*), and quotes a button's label as the
  interface has it. A page shows lines of a source file with
  `{{#include}}` (`reference.md`), read when the book is built. The
  pictures are `docs/images`, copied to the site's `images/`
  (`../images/…` from a page), and the ones of the windows are taken
  by `docs/shots/shoot.py` (macOS, swiftc, cargo; `--keep-data` and
  scene names to retake some), with no window of the app's and nothing
  on the desktop: `shoot.swift` draws a page of `ui/` in the system's
  WebKit in a window on no screen, at two pixels a point, over
  `stub.js`, a stand-in for the app that answers the page's calls with
  what the real code says of hosts that do not exist
  (`src-tauri/tests/doc_data.rs`, ignored unless the script runs it:
  the model's state of a made-up ssh config, the readings' parser on
  its fixture, the Files model on a small tree; this machine's paths
  and user are taken out before a page sees them) and what
  `scenes.py` adds (`dress`: masters up, credentials, copies, a
  terminal's words). A scene of `scenes.py` is a page, the steps that
  bring it to what the picture shows, and the parts it names, each an
  expression for its element; the script writes
  `docs/images/<lang>/<scene>.png` and
  `docs/<lang>/src/figures/<scene>.html`, the picture with a numbered
  pin on each part and the parts' names from `terms.py` underneath,
  which a page takes in with `{{#include figures/<scene>.html}}`. A
  Console page in such a window is told it has the keyboard
  (`document.hasFocus`), or it swallows the first click as the one
  that brought the window forward. A scene's `ends` names what
  its picture ends under, for a panel shorter than its window (the
  Status panel's). The README's two pictures are scenes of the same
  script with `window` (`front-main`, `front-console`, in both
  languages): the page drawn under the mac
  class and put by `frame.swift` into a window's shape, its round
  corners, edge and traffic lights drawn there, on a blurred backdrop
  with a shadow; no window of the app's is captured for them. The
  pictures and figures are checked in: the site's build takes none.

## Tests

`cargo test` in `src-tauri` runs the unit tests (prompt classification,
`ls` parsing, TOTP vectors, the keychain's decisions, the catalog parser)
and the integration tests in `src-tauri/tests/`:

* `pty_prompt.rs`: a shell script asking on `/dev/tty` is answered through
  the pty while its stdout stays on the pipe. Needs nothing; CI runs it.
* `keychain_access.rs`: the "Try again" for a keychain that refused
  (`Model::request_keychain_access`) comes back, under
  `SHELLDER_NO_KEYCHAIN` so no keychain is touched: it deadlocked on the
  model's own state once. Needs nothing; CI runs it.
* `cli.rs`: the subcommands, run as the binary itself with a state
  directory and an ssh config of its own and the keychain off: what they
  print and how they exit, a lock taking the jump host along and an
  unlock the hosts behind it, and a secret piped in that ends up in no
  file. Needs `ssh` (for `ssh -G`); CI runs it.
* `ssh_master.rs`: a real master against a throwaway `sshd` on a high port,
  run as this user: the host key confirmation and the key passphrase both
  come down the pty, the socket comes up, and commands through it run in
  `sh`, a name with quotes and backslashes intact. Needs `/usr/sbin/sshd` and
  `ssh-keygen`; a password login needs root, so it is not covered.
* `keepalive.rs`: a host without a ControlPath against the same throwaway
  sshd (`tests/common/mod.rs`): the master starts without `-M`, comes up on
  the "Authenticated to" line, has no socket to check.
* `session.rs`: the same throwaway sshd (`tests/common/mod.rs`) for a host
  without a ControlPath: the session logs in once and runs commands; the
  copies (sftp logging in on its own) go up and down, a file and a
  directory, go on from a cut copy, report failures and can be cancelled;
  the Console's readings (`monitor::fetch`) come through the session; and
  a `Terminal` gets a shell whose output comes back and which follows a
  resize. Its ignored test does the listings and copies against the
  BusyBox host of `Tests/files-window.sh`, without a ControlPath.
* `transfer_resume.rs`: a copy cut short is completed by a resumed one, in
  both directions and for a directory, and the Files model's "continue"
  does not ask whether the name is taken. Goes through this machine's own
  sshd (Remote Login on, your key in `authorized_keys`), skipped otherwise.
* `files_window.rs`: eight tests against trees on this machine, which
  `cargo test` runs (a folder shut whole, Back bringing the folders
  back as they were open, a refresh keeping the folders open deeper
  down, listings that overlap, waiting copies taken off the queue, a
  rename keeping the row selected and its folders open, refused for a
  name that is taken, a pane inside the folder renamed going on under
  the new path, a
  drop or upload landing in the folder it is on, the one shown too, and
  a drop's question about a taken name: *Apply to all* for that drop
  only, an answer to a question no longer asked ignored, no going on
  from a copy that was replacing a file, the folder copied into read
  again, and the Console's tab with a tree of its own beside the
  window's: neither moved by what the other does, a tab opened starting
  no pane of the window's, an upload and a download queued between the
  places they name); the
  other ignored unless
  `Tests/files-window.sh` runs it. The
  script brings two sshd's up in Docker (BusyBox `ls` on one, GNU on the
  other, this machine's `ls` for the BSDs) and the test drives `FilesModel`
  the way the panes do: listings, drops both ways and between hosts, new
  folders and files, renaming (GNU's mv and BusyBox's), deleting,
  queueing, cancelling, dates and sort orders. Deleting on this machine
  is the Trash, which the `trash` crate reaches by scripting the Finder
  (`osascript`): in a session that may not, those steps hang.
  The script then runs `session.rs`'s ignored test against the BusyBox
  host without its ControlPath (its `ash` reads input in chunks, which the
  session's protocol must be safe against).
* `monitor`'s ignored unit test runs the reading script in plain
  `alpine` (BusyBox) and `debian` (dash) containers and parses what they
  say: `cargo test --lib monitor -- --ignored`, with Docker.

`cargo bench` in `src-tauri` runs the performance tests, two programs
with the small timer in `benches/harness` (no test harness, no crate for
it; the `bench` profile is the release one, so the first build takes a
couple of minutes). `parsing` starts no process: a question told apart, a
listing of ten thousand files, a host's readings, an ssh config of two
thousand hosts, a file's base64, the prefs. `pipes` does: the lines a pty
reader takes before a question, a terminal's output, a listing and a
reading of this machine, and against the throwaway sshd of
`tests/common` (skipped without one) a session's login and its commands,
the same through a master, a file read, an upload and a download. Each
prints the median of its runs and is held to a budget several times
what it takes on an ordinary machine, so that a budget is missed by work
that got slower in kind, not by a busy machine; one missed makes the
run exit 1, `SHELLDER_BENCH_LAX=1` only prints, and `cargo bench --bench
pipes -- session` runs the measurements with that word in their name.
The lines of `pipes` that begin "plain" are the same work without the
app, printed for the line above each to be read against and held to
nothing: the bytes on a pipe instead of a pty, ssh and scp as a script
runs them, an askpass program answering (a login of ours takes some
50 ms longer than plain ssh's there, each of its two questions being
known for one only after 20 ms of silence; a terminal's output is the
pty's own pace, the reader being a bare loop). A
reading of a Mac takes a second and a half whatever carries it: the
script samples the CPU with `top -l 2 -s 1` there. CI does not run them.

`Tests/render-bench.sh` measures what drawing a terminal costs, on macOS
with `swiftc` and `python3`, nothing of the app built or started: the
page in `Tests/render-bench/` gives xterm.js the same output the way
console.js does (base64 in 16 KB pieces, `atob`, `term.write`) with no
renderer (the terminal never opened: the parser alone), the DOM renderer
and the WebGL one, for plain lines, coloured ones, wide characters and a
full screen redrawn, and says the rate, the share of the time that went
into drawing, the frames and the longest stall between two.
`webkit.swift` runs it in the system's WebKit in a window on no screen
(so what the window server does with the pixels, the see-through window
and its blur, is not in it) and pushes one run in through
`evaluateJavaScript`, as an event of the app's arrives, at the pace a
pty gives a flood: whether the page keeps up. It holds nothing to a
budget; the page opened in a browser by hand measures that browser.

`Tests/gui-bench.sh` measures the same in the app's own window, where
the bytes take the real way (the pty, `term.rs`'s reader, a `term-data`
event a piece, console.js, xterm.js, the window server). It builds the
`probe` profile (`cargo build --profile probe`: the release build with
debug assertions, so that SHELLDER_PROBE is honoured), starts that
binary isolated (its own state directory and ssh config, the keychain
off) and serves it `Tests/gui-bench/probe.js`, which has the main window
open this machine's Console and there opens tabs through the page's
`window.__shellder.run`, each playing a file between two marks (an OSC
7777 of its own, which xterm hands the probe when it has parsed that
far): the rate, the frames, the longest stall, how far behind the
terminal is when the last piece has come, and a key's way round (written
to the pty, echoed, parsed, drawn), for the DOM and the WebGL renderer,
opaque and see-through with the blur. The windows open on a display that
is no screen (`display.swift`: CoreGraphics' CGVirtualDisplay, by name,
which the window server composites at 60 Hz), placed there through the
state directory's `window-state.json`, so the desktop keeps what it
shows; the app coming up takes the keyboard for a moment and the
application that had it is put in front again (`open -b`).
`GUI_BENCH_SCREEN=1` opens them on the real screen. A run that fails
says what the page said. The window takes 20 to 35 MB a second where a
pty gives 65: the page says what it parsed (`term_ack`, every 256 KB)
and the terminal's reader hands on no more while a megabyte is out
(`terminals::Flow`), which is what keeps a flood from queueing up in the
page until xterm throws it away at 50 MB ("write data discarded") and
the terminal within some 30 ms of its program; a page not on screen
(`term_hold`) is not waited for, its timers running late, nor one that
says nothing for ten seconds. A tab whose
command ends well is closed by the page at once, with what xterm had
not parsed, so the files' script stays (`exec sleep`) until the probe
closes its tab.

`Tests/pdf-copy.sh` runs `Tests/pdf-copy/test.mjs` in node (its own
test runner, no npm): lines set as a page sets them, in columns, and
what a copy of them gives (`paragraphStarts` and `joined` of
`ui/pdfcopy.js`, which know no DOM): paragraphs flush and ragged, a
list's items, a program's lines told by their letters all one width
(pdf.js calls TeX's typewriter a serif), a paragraph going on in the
next column and on the next page, the hyphens. `linesOf`, which reads
the lines from pdf.js's words, needs a web view and is not run; CI does
not run the script.

`Tests/drop-unlocked.sh` drives the built binary and a throwaway sshd in
Docker: an unlocked host whose master the server closes right after login
must switch off with the error, not retry. `Tests/linux-build.sh` is the
Linux build from nothing: a bare Ubuntu container (24.04 and 26.04 by
default) gets `./build.sh --deps`, runs the unit tests CI runs and a full
`./build.sh`, and the AppImage has to answer `help`. About a quarter of an
hour per version, most of it compiling the Tauri CLI; the sources are
copied in, not mounted, since a directory mounted from macOS is
case-insensitive and appimagetool's `.DirIcon` link collides there. Prefer
adding a test over repeating steps in a conversation.

## Build

```bash
./build.sh              # cargo tauri build, wraps it as build/Shellder.app (macOS) or build/*.deb, *.AppImage (Linux)
./build.sh --debug      # unsigned debug build, no updater artifacts
./build.sh --install    # also copies to ~/Applications and relaunches (kills the running app!)
./build.sh --deps       # only installs what a build needs (Rust, Tauri CLI, the Linux packages)
```

* `build.sh` installs what is missing and nothing else: Rust with rustup,
  the Tauri CLI, and on Linux the development packages through apt, dnf,
  pacman or zypper (the lists are `linux_packages` in `build.sh`; CI runs
  `./build.sh --deps` first). A build that finds everything asks nothing.
* `build.sh` signs with the first Apple Development / Developer ID identity
  in the keychain, or the one in `SHELLDER_SIGN_IDENTITY`. There is no ad hoc
  fallback for a release build; `SHELLDER_SIGN_IDENTITY=-` asks for it,
  `--debug` builds are left unsigned. Keep the bundle id `local.shellder`:
  the vault item trusts it.
* The updater artifacts (`Shellder.app.tar.gz` + `.sig`, `.AppImage` +
  `.sig`) are only made when `TAURI_SIGNING_PRIVATE_KEY` is set (the
  minisign key whose public half is in `tauri.conf.json`). Without it the
  build skips them.
* Icons are generated from `scripts/icon-source.png` by `cargo tauri icon`
  into `src-tauri/icons/` (ignored by git), except the menu bar picture
  `tray.png`, which is checked in and comes from the small Rust tool in
  `scripts/menubar-template` (the command is in `docs/manual.md`). The faded
  copy shown while no master is up is made at run time (`app/tray.rs`).
* Never run `--install` while testing: it does `pkill -x shellder`.

## CI (`.github/workflows/build.yml`)

* Runs on every push to `main`, every PR, every `v*` tag and manual
  dispatch. A job has forty minutes.
* Matrix: `macos-26` (GA, blocking), `xcode-27` (preview, `continue-on-error`),
  `ubuntu-24.04` (builds the release), `ubuntu-26.04`.
* "Dependencies" is `./build.sh --deps`: the apt packages on Ubuntu and the
  Tauri CLI, the same list a developer's build installs. The rust-cache key
  includes `matrix.runner`: the cache carries `~/.cargo/bin`, and a Tauri
  CLI built on one Ubuntu does not run on the other.
* The "Icons" step runs `cargo tauri icon` before the unit tests: the
  icons are not checked in, and `tauri::generate_context!()` reads them at
  compile time, so without that step `cargo test` fails to compile.
* "Unit tests" (`cargo test --lib --test pty_prompt --test keychain_access
  --test cli`)
  runs on ordinary commits only: a `v*` tag run and a `[release] ...`
  commit on `main` skip it, so a release build does not repeat tests that
  the commits before it already passed.
* macOS imports the maintainer's Apple Development certificate from
  `SHELLDER_SIGN_P12` / `SHELLDER_SIGN_P12_PASSWORD` into a throwaway
  keychain; the job fails without them, so pull requests from forks do not
  build. `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`) signs the updater
  artifacts on both platforms. The keychain is deleted at the end of the job.
* "Verify bundle" only checks the bundle's structure: on macOS codesign
  `--verify --strict`, a `TeamIdentifier` in the signature (not ad hoc),
  `plutil -lint`, the icns, and `shellder help` runs; on Linux the deb and
  AppImage exist and the deb's metadata reads. No functional test runs on CI.
* A `v*` tag publishes a GitHub Release with the macOS zip, the Linux deb
  and AppImage, the signed updater artifacts and `latest.json`
  (`scripts/latest-json.sh`), which is what the app's updater reads.

## One binary, two modes

`src/main.rs` picks the mode from argv:

1. Any argument: CLI subcommand (`shellder help` lists them: `status`,
   `hosts`, `lock`, `unlock`, `add-secret`, `del-secret`, `install`,
   `uninstall`). Works without the app running, but there is no command
   that prints a secret or logs in with one.
2. No arguments: GUI (`--background` starts without a window; `--login` is
   what the login item passes, the only launch that honours "start
   silently").

There is no askpass helper: nothing outside the app ever handles a prompt,
so there is no socket and no peer verification.

## Isolation hooks

Set these on the binary you launch. Run
`build/Shellder.app/Contents/MacOS/shellder` directly so the variables are
inherited (`open` would not pass them).

| Variable | Effect | Default |
| --- | --- | --- |
| `SHELLDER_SSH_CONFIG` | ssh config to use, passed to ssh as `-F` | `~/.ssh/config` |
| `SHELLDER_STATE_DIR` | `prefs.json`, `app.lock`, `vault.stamp`, TOTP replay markers, `shellder.log`, and `window-state.json` (the window positions, which otherwise live where tauri-plugin-window-state keeps them, keyed by the bundle id) | `~/Library/Application Support/shellder` (macOS), `$XDG_STATE_HOME/shellder` (Linux) |
| `SHELLDER_UPDATE_API` | URL of the updater's `latest.json` | the latest GitHub release's `latest.json` |
| `SHELLDER_NO_CONTROLMASTER` | `1`: the app as a platform without ControlMaster sees it (masters without a socket; Console and Files log in on their own) | unset |
| `SHELLDER_NO_KEYCHAIN` | `1`: the vault reads as empty and is never written, so a test copy (an unsigned debug build above all) raises no keychain dialog on the user's screen | unset |
| `SHELLDER_PROBE` | a URL (debug builds only): every page loads `probe.js` from it once drawn, a script of yours that reads the real web view (computed styles, what a wheel does, `invoke` calls) and reports back, say with an `<img>` GET to the same server; for a session that has no screen to look at | unset |

Things that are **not** isolated:

* **The keychain vault.** `shellder add-secret` writes to the user's real
  vault. Use throwaway host aliases (for example `shjt-*`) and run
  `shellder del-secret HOST KIND` when done. Never delete the vault item
  itself (`shellder:vault`), see below.
* **The login item.** `shellder install` writes
  `~/Library/LaunchAgents/local.shellder.plist` (macOS) or
  `~/.config/autostart/local.shellder.desktop` (Linux). Do not run it from a
  test copy.
`app.lock` is an `flock` in the state dir, so a test GUI with its own
`SHELLDER_STATE_DIR` runs beside the user's app and writes its own
`shellder.log` there. A GUI that finds the lock taken (a second copy with
the same state dir) exits at once with "shellder is already running"; only
a process started by `app::restart` (marked `SHELLDER_RELAUNCH=1`) waits
up to ten seconds for the old one to let go. A GUI instance truncates its log at start (previous
content moves to `.log.old`), CLI runs append.

## End-to-end test against a local sshd (Docker)

The recipe that has worked:

1. `docker run -d --name shjt -p 127.0.0.1:2222:22 alpine sh -c 'apk add openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e'`
   (or an image with `google-authenticator` PAM for TOTP tests).
2. Write a throwaway ssh config with a `Host` entry: `HostName 127.0.0.1`,
   `Port 2222`, `User t`, `ControlMaster auto`, `ControlPath /tmp/shjt/cm-%C`,
   `StrictHostKeyChecking no`, `UserKnownHostsFile /dev/null`.
3. Store the password: `printf 'pw\n' | SHELLDER_SSH_CONFIG=... shellder add-secret HOST password`
   (stdin is read when it is not a TTY).
4. Lock the host (`shellder lock HOST` with the same env), start the GUI
   binary with the same env plus `--background`, then check
   `ssh -F <config> -O check HOST` and the log for `master established`.
   There is no CLI login: only an ssh the app started gets an answer.
5. Clean up: `del-secret`, `docker rm -f shjt`, `rm -rf /tmp/shjt`.

Without Docker, `cargo test --test ssh_master` covers the prompt path with a
key passphrase and a host key confirmation against a throwaway sshd started
as the current user (password authentication needs root, so it is not
covered that way).

Gotchas:

* ControlPath must be short (unix socket paths are limited to about 104
  bytes). Use `/tmp/shjt`, not a deep scratch directory.
* ProxyJump: two sshd containers on one docker network, only the jump host
  published to 127.0.0.1. Alpine's sshd ships `AllowTcpForwarding no`; sed it
  to `yes` and `kill -HUP 1` in the container, or the hop fails with
  "stdio forwarding failed". Hosts whose jump hosts go round in a loop
  are switched off with that error, not left waiting.
* `pkill -f` inside `docker exec sh -c '...'` matches its own shell, anchor
  the pattern with `^`.
* Wrong secrets: `NumberOfPasswordPrompts=1` means one failed login per
  attempt, and after three quick failures the daemon backs off to ten
  minutes. Real servers may ban the client IP after a few failures, so never
  point tests at a real host with a guessed password.
* Host key confirmations need the app (it shows the fingerprint). Use
  `StrictHostKeyChecking no` in the test config.
* **Nothing the app started may outlive it.** macOS 26 (loginwindow's
  `quitsupport`) keeps an app in the running list after its process
  exited while a process of its coalition is alive, and after half a
  minute tells the user it "is running in the background". A ssh of
  ours that became the master under a `ControlMaster auto` config and
  forked a ControlPersist master was such a process: every ssh the app
  starts gets `ControlPersist=no` (`ssh::base_args`), and
  `Model::shutdown` waits for the terminals and sessions to be reaped
  before the process exits. `log show --predicate 'subsystem ==
  "com.apple.launchservices" AND category == "quitsupport"'` names the
  pid that held the app up. An app that ends without that shutdown (a
  kill -9, a `cargo tauri dev` rebuild) leaves its terminals, sessions
  and copies behind: the pty hangup never reaches them once nobody
  holds the asking pty, and a remote shell on a pty survives the end of
  its input, so they wait for ever, each holding a session on the user's
  own master until its MaxSessions is full and every new tab logs in on
  its own. So every ssh, scp and sftp the app starts carries `-o
  Tag=shellder` (`ssh::base_args`, `ssh::TAG`), and `ssh::kill_orphans`
  (at the daemon's start and when a Console window opens) hangs up every
  tagged process whose parent is init, SIGTERM for a `-M` master, SIGHUP
  for the rest; the tag says nothing about what a process is,
  `is_our_master` looks for `-M` or ppid 1 besides.
* **How a question is recognised.** The pty reader logs every complete line
  as `SSH host: line` and treats text left without a newline for 20 ms,
  ending in `:` or `?`, as a question (`pty::quiet_for`: the pty is the
  process's controlling terminal and nothing else, so what comes down
  it was written to `/dev/tty`, and ssh writes only its questions
  there, each whole in one write, a host key's with the echo on as a
  secret's with it off; on an action's pty, which is ssh's standard
  streams and carries a host's output in the network's pieces, 200 ms);
  the whole text since the last answer
  is the prompt (so a host key's fingerprint lines are part of it). A
  `PROMPT` line in the log says what was asked and how it was answered.
  The reader shows the text as a terminal would (a backspace takes a
  character off) and drops the echo of its own answer, a line of stars
  (sudo's pwfeedback) included: nothing says how long a password was.
  ssh's stderr stays on a pipe and never reaches the pty. A question left
  unanswered (cancelled, a stored secret the server rejected) ends a
  master's attempt through the daemon and an action through its handler;
  a terminal's or a session's ssh is hung up by the reader, or it would
  wait at the question for ever.
* **Windows are built hidden** (`windows::build`) and come on screen when
  their page calls `window_ready` (`pageReady()` in `shellder.js`, after
  the first render); `present` only records the wish until then, with a
  three-second fallback for a page that never gets there. The window is
  painted in app.css's `--bg` for the appearance, and the page gets the
  language as `window.__SHELLDER_LANG` so `startPage()` fetches the
  translation and the state side by side. tauri-plugin-window-state
  restores the saved size and position while the window is built, but
  not its visibility (`StateFlags::VISIBLE` is off: restored, it would
  show the window there and then) nor its decorations
  (`StateFlags::DECORATIONS` is off: the plugin keeps them per window
  label, and a label last closed in immersive mode, which takes them
  away, came back without its title bar under a page drawing its bars;
  the mode is not kept, a Console opens with its bars). A size set from code is
  not held to `min_inner_size` (that binds the user's dragging only), so a window
  restored below its minimum is put back to its default size
  (`windows::keep_above_minimum`, a warning in the log).
* **Popups go through `popup()` (or `menu()`) in `shellder.js`.** A
  `position: fixed` box placed at its button is no wider than the room
  between the button and the window's right edge, so a menu opened from
  a button near that edge (a card title's +) wraps its lines and then
  measures as fitting: it hugs the edge, squeezed, instead of moving
  left. `popup()` measures the box at the window's left edge first, then
  places it. A page's own copy of that placement code got this wrong
  once; do not write another.
* **The scroll bars are the page's own.** WebKit's have no width
  (app.css); `shellder.js` draws a thumb (`.thumb`, `pointer-events:
  none`) over an area's right edge while it scrolls, and the pointer in
  that edge's last 12 pixels (`THUMB_EDGE`) brings it out and takes the
  press: a drag moves it, a press beside it jumps there, and the press's
  mouse events and click never reach the element under it. So a control
  flush with a scrolling area's right edge loses those pixels to the
  bar, as under a system scroll bar, unless it lies over the area
  rather than in it (a column's resize handle). The terminals keep
  xterm's bar. A thumb that ignored the pointer, as it once did, could
  not be dragged at all.
* **A page's rule reaches the views drawn in it.** `body.console` is the
  Console's whole page, so `.console .head`, written for the Status
  tab's card, was also the rule of a box named `head` in the Files
  tab's tree (`filesview.js` is drawn in two pages): a column, the sort
  arrow under its title, while the Files window showed one line. A view
  drawn in more than one page (`filesview.js`, `ports.js`, `preview.js`)
  names its boxes after itself (`colhead`) and states the layout it
  depends on (`flex-flow: row nowrap`); a page's rule names the card it
  is for (`.console .crow.head`, `.console .gauge .bar`), never a short
  name alone. The file list's column widths are read from a table of
  the view's own cells laid out off screen (`columnWidths`), not
  reckoned from fonts and paddings that app.css may change.
* A Console window's label comes from the host it opened on, but the
  window can be switched to another host (`windows::switch_console`
  sends the page `console-host` and it switches in place: the column
  reads the other host, the tabs stay, each on the host it runs on,
  `term_open` taking the host per tab), so the label does not say which
  host a window shows:
  `windows::SHOWING` does, and `show_console` looks there, taking a free
  label (`console-HOST-2`) when the host's own is still held by a window
  that switched away.
* **A page never gets an HTML drop on macOS.** wry hands every drag to
  Tauri's drag-drop handler, which answers that it took it, so WebKit is
  not told of the drop and `ondrop` never fires, for the page's own
  drags too (the ghost shows, nothing lands). What the page gets is the
  webview's native drag-drop event, at a position, with the desktop's
  file paths or none. Its position is in the page's CSS pixels (wry
  gives AppKit's points, and GTK's widget coordinates, which Tauri
  calls physical without scaling them): `dropPoint` in `shellder.js`,
  never divided by the pixel ratio, which put every drop on a Retina
  screen at half the pointer's distance from the corner.
  `filesview.js` (`nativeDrops`) remembers the rows
  a drag of its own carries and lands them from that event, on a pane
  or on what the page's `dropOutside` names (the Console's terminals).
  The HTML handlers stay for the platforms that deliver them, and
  `dropRows` keeps the two from acting twice.
* The pty's slave side is held by `MasterProc` (a `Terminal`, a `Session`)
  for the process's lifetime. Closing it is what ends the reader thread (on
  macOS a pty whose slave nobody holds reads as hung up at once), so a
  master that is gone from the daemon takes its reader with it.
* A `Terminal` has two ptys: the controlling one for the questions, as
  everywhere, and one for ssh's standard streams, which is the screen. A
  resize sets the screen's size first, then the controlling terminal's:
  the latter is what sends ssh its SIGWINCH, and ssh then reads the size
  from its stdin, the screen.
* A `Session` is `ssh -T host sh` with stdin/stdout on pipes. Every command
  is followed by a `printf` of a marker line with the exit status, and
  nothing is written to the shell before the previous marker arrived: that
  is what keeps a BusyBox `ash` (chunked input) from swallowing what
  follows. Commands run in a subshell (`exit`, `cd` stay inside). Files
  never go over it: a copy is sftp/scp with an asking pty of its own. A
  command given a deadline (`run_within`: the Console's readings, the
  listings) that passes it loses the session, whose ssh is killed (the
  markers cannot be put back in step), and the next command logs in
  again; a session whose shell is found gone when a command is written
  is opened again once.

## Keychain

* One item: service `shellder:vault`, account `shellder`, a JSON object
  `{host: {password|passphrase|totp: secret}}`. On Linux the same item in
  the Secret Service (`keyring` crate); a session without a default keyring
  reads as an empty vault, and the first save creates one.
* On macOS updates go through `SecItemUpdate`. The app never deletes or
  re-creates the item: a `SecItemDelete` of an item another signature made
  is a second password dialog, and has also failed with
  `errSecInvalidOwnerEdit (-25244)`, so do not "fix" the vault by deleting
  it. The first read under a new signature is the one dialog ("wants to use
  your confidential information"); "Always Allow" with the password puts
  that signature on the item for good.
* The app caches the vault for the whole run. Every write touches
  `vault.stamp` in the state dir with the writer's pid, and the app drops
  its cache when another process wrote it, so `shellder add-secret` still
  shows up within a few seconds.
* A refused read is remembered for `keychain::REFUSAL_TTL` (five minutes)
  and repeated without another dialog. An item whose JSON does not parse
  is taken as a refusal too, never as an empty vault. Writes (`set`, `delete`) always ask
  the keychain afresh and fail when it refuses, they never write over a
  vault they could not read.
* The keychain partition is `teamid:` plus the Team ID of the signature, so a
  build signed by another team (or unsigned, as `--debug` builds are)
  triggers a keychain password dialog, or a refusal where no dialog is
  possible (an ssh session: "User interaction is not allowed"). To poke at
  the vault without dialogs, sign the test binary with the same Apple
  Development identity (`build.sh` does, keeping the identifier
  `local.shellder`).
* A keychain dialog blocks the app until answered. From a script `kill -9`
  the test copy to dismiss it.

## Driving a test GUI by hand

1. Copy `build/Shellder.app` to `/tmp/shjt/ShellderTest.app` and change
   `CFBundleIdentifier` in its `Info.plist` to `local.shellder.test` so
   window lookup does not resolve to the user's own running Shellder; re-sign
   with the Apple Development identity and `--identifier local.shellder`
   (keeps the vault readable).
2. Launch it with the `SHELLDER_*` variables (`open --env VAR=value ...` when
   it must be visible to System Events; a GUI started from a sandboxed shell
   is invisible to the desktop automation tools).
3. The pages are ordinary web pages: a change in `ui/` only needs the window
   reopened, not a rebuild, when running `cargo tauri dev` from `src-tauri`.
4. Quit it through the tray menu or ⌘Q; `kill` (SIGTERM) also stops the
   masters cleanly.

Quirks of the desktop automation tools:

* The status item is not a window; only display-scope screenshots and clicks
  reach it, and clicks are refused unless the test app is frontmost. A
  screenshot dismisses an open status menu, so confirm menu paths with a
  temporary log line instead of a picture.
* The tool activates the app right before a click, so judge
  activation-dependent behaviour from a log line, not from the tool run.
  Its double click is two separate clicks.
* Posting CGEvents from a scratch binary is dropped (no accessibility grant).
* The test copy is unreachable while a relaunch (language, update) is in
  flight and again once an update replaced it, since the release bundle
  carries the real bundle id: put `local.shellder.test` back and re-sign
  before driving it further.

## Testing the updater

`src-tauri/src/app/updater.rs` is tauri-plugin-updater: it reads
`SHELLDER_UPDATE_API` (a `latest.json` with `version`, `notes`, `pub_date`
and `platforms.<target>.{url, signature}`) and installs the artifact for
this platform after checking its minisign signature against the public key
in `tauri.conf.json`. To try it without a release: build with
`TAURI_SIGNING_PRIVATE_KEY` set to a key of your own, bump `version` in
`tauri.conf.json` and `Cargo.toml` for the second build, serve its
`target/release/bundle/macos/Shellder.app.tar.gz` and `.sig` with
`python3 -m http.server`, and write a `latest.json` next to them
(`scripts/latest-json.sh`). Start the first build with
`SHELLDER_UPDATE_API=http://127.0.0.1:PORT/latest.json`; the log shows
`update: version X is available` a few seconds in, "Install and relaunch" in
Settings replaces the bundle and the relaunched process logs a fresh
`daemon starting`.

## Conventions

* Commit messages: `[component] description`, English. Do not commit or push
  unless asked.
* Commit messages name no agent: no `Co-Authored-By` trailer, no "Generated
  with" line, nothing that says which tool wrote the change.
* A release is a `v*` tag on a commit of its own, `[release] x.y.z`,
  that changes nothing but the version number (`src-tauri/Cargo.toml`,
  `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`). Never tag a commit
  that also carries code, docs or CI changes, and never put such changes
  into the release commit: CI skips the unit tests on release builds, so
  anything that rides along in that commit ships untested.
* Develop on `main` in the main checkout, not on a branch or in a git
  worktree: the maintainer tests with `cargo tauri dev` from that checkout,
  so a change that lives anywhere else is invisible to them. If a session
  was started in a worktree anyway, finish by committing there, rebasing
  onto `main` and fast-forwarding `main` in the main checkout.
* Do not add migrations for prefs or the vault format.
* Never log secret values or their lengths.
* No personal data (real host names, accounts, Team IDs) in the repository.
* `cargo fmt` and `cargo clippy --all-targets` before handing over.
