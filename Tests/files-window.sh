#!/bin/sh
# The Files window's model without a window: two throwaway sshd's in Docker,
# the app's own sources built with a test main that drives FilesModel the way
# the panes do (listings, drops in both directions and between two hosts, new
# folders and files, deleting, queueing, cancelling, the transfer list).
# See AGENTS.md.
# Usage: Tests/files-window.sh
set -eu
DIR=/tmp/shjt-files
export SHELLDER_SSH_CONFIG=$DIR/config SHELLDER_STATE_DIR=$DIR SHELLDER_PREFS_SUITE=local.shellder.test

cleanup() {
    ssh -F $DIR/config -O exit shjt-files >/dev/null 2>&1 || true
    ssh -F $DIR/config -O exit shjt-files-b >/dev/null 2>&1 || true
    docker rm -f shjt-files shjt-files-b >/dev/null 2>&1 || true
    rm -rf $DIR
}
trap cleanup EXIT
cleanup
mkdir -p $DIR/here
ssh-keygen -q -t ed25519 -N '' -f $DIR/id -C shjt-files
for pair in "shjt-files 2225" "shjt-files-b 2226"; do
    set -- $pair
    docker run -d --name $1 -p 127.0.0.1:$2:22 alpine sh -c \
        'apk add -q openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e' >/dev/null
    cat >> $DIR/config <<CFG
Host $1
    HostName 127.0.0.1
    Port $2
    User t
    IdentityFile $DIR/id
    IdentitiesOnly yes
    ControlMaster auto
    ControlPath $DIR/cm-%C
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
CFG
done
for name in shjt-files shjt-files-b; do
    for i in $(seq 1 60); do docker exec $name pgrep -x sshd >/dev/null 2>&1 && break; sleep 1; done
    docker exec $name sh -c 'mkdir -p /home/t/.ssh && chmod 700 /home/t/.ssh && chown t:t /home/t/.ssh'
    docker exec -i $name sh -c 'cat > /home/t/.ssh/authorized_keys && chmod 600 /home/t/.ssh/authorized_keys && chown t:t /home/t/.ssh/authorized_keys' < $DIR/id.pub
done
# A tree with a hidden entry, a directory with a space in its name, a
# symlinked directory and something big enough to show progress.
docker exec shjt-files su t -c 'cd $HOME && mkdir -p Shellder/sub "my dir" && echo one > Shellder/one.txt \
    && echo two > Shellder/sub/two.txt && echo h > Shellder/.hidden && ln -sf Shellder link \
    && dd if=/dev/zero of=Shellder/big.bin bs=1M count=200 2>/dev/null \
    && dd if=/dev/zero of=huge.bin bs=1M count=600 2>/dev/null'
docker exec shjt-files su t -c 'cd $HOME && echo remote > clash.txt && echo remote > two.txt'
docker exec shjt-files-b su t -c 'cd $HOME && mkdir -p Shellder'
printf 'local\n' > $DIR/here/clash.txt
printf 'local\n' > $DIR/here/two.txt
ssh -F $DIR/config -M -f -N shjt-files
ssh -F $DIR/config -M -f -N shjt-files-b

cat > $DIR/main.swift <<'SWIFT'
import AppKit

setbuf(stdout, nil)
_ = NSApplication.shared
NSApp.setActivationPolicy(.accessory)
let dir = ProcessInfo.processInfo.environment["SHELLDER_STATE_DIR"]!
let here = dir + "/here"
let hostA = FileSource.host("shjt-files")
let hostB = FileSource.host("shjt-files-b")
var failures: [String] = []

func pump(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }
func check(_ what: String, _ ok: Bool, _ detail: String = "") {
    print("\(ok ? "ok  " : "FAIL") \(what)\(detail.isEmpty ? "" : ": \(detail)")")
    if !ok { failures.append(what) }
}
/// Wait for a drop to turn into a copy and for that copy to end.
@discardableResult
func settle(_ model: FilesModel) -> [Double] {
    var waited = 0.0
    while model.active == nil && waited < 5 { pump(0.1); waited += 0.1 }
    var seen: [Double] = []
    while model.active != nil {
        pump(0.25)
        if let p = model.active?.progress, seen.last != p { seen.append(p) }
    }
    pump(0.5)
    return seen
}
/// Wait for the whole queue, including copies that never start because
/// the answer to the question was Skip.
func settleAll(_ model: FilesModel) {
    var waited = 0.0
    while (model.active != nil || model.transfers.contains { $0.state == .waiting }) && waited < 60 {
        pump(0.2)
        waited += 0.2
    }
    pump(0.6)
}

/// Wait for a new folder, a new file or a delete to report back: the line
/// it writes replaces whatever the one before it left there.
func settleNotice(_ model: FilesModel) -> String {
    let before = model.notice
    var waited = 0.0
    while model.notice == before && waited < 5 { pump(0.1); waited += 0.1 }
    pump(0.6)
    return model.notice
}
func names(_ model: FilesModel, _ pane: FilesModel.Pane) -> [String] {
    model.rows(pane).map(\.item.name)
}
/// What a drop hands the model: the pasteboard of the dragging session,
/// with one item per dragged row.
func dragged(_ payloads: String...) -> NSPasteboard {
    let board = NSPasteboard(name: NSPasteboard.Name("local.shellder.test.drag"))
    board.clearContents()
    board.writeObjects(payloads.map { payload -> NSPasteboardItem in
        let item = NSPasteboardItem()
        item.setString(payload, forType: .string)
        return item
    })
    return board
}
func draggedFromFinder(_ path: String) -> NSPasteboard {
    let board = NSPasteboard(name: NSPasteboard.Name("local.shellder.test.drag"))
    board.clearContents()
    board.writeObjects([URL(fileURLWithPath: path) as NSURL])
    return board
}

let model = FilesModel(host: "shjt-files")
model.setHosts(["shjt-files"])
model.start()
pump(5)

// MARK: the trees

check("the host opens on the left, in ~/Shellder",
      model.source(.left) == hostA && model.root(.left).hasSuffix("/Shellder"), model.root(.left))
check("this Mac opens on the right", model.source(.right) == .local && model.root(.right) == Config.home,
      model.root(.right))
check("the machine menu offers what is connected", model.hosts == ["shjt-files"], "\(model.hosts)")
model.setHosts(["shjt-files", "shjt-files-b"])
check("and picks up a host connected while the window is open",
      model.hosts == ["shjt-files", "shjt-files-b"], "\(model.hosts)")
check("it lists what is there", names(model, .left) == ["sub", "big.bin", "one.txt"], "\(names(model, .left))")
check("dot entries are out of the way", !names(model, .left).contains(".hidden"))
model.toggleHidden(.left)
pump(1)
check("until the pane is asked for them", names(model, .left).contains(".hidden"), "\(names(model, .left))")
model.toggleHidden(.left)
pump(1)

if let sub = model.rows(.left).first(where: { $0.item.name == "sub" }) {
    model.toggle(.left, sub.item)
    pump(2)
    check("a directory opens under its own line",
          names(model, .left) == ["sub", "two.txt", "big.bin", "one.txt"], "\(names(model, .left))")
    model.toggle(.left, sub.item)
    pump(0.3)
    check("and folds shut again", names(model, .left) == ["sub", "big.bin", "one.txt"])
}

model.up(.left)
pump(2)
check("up one level reaches the home directory", model.root(.left) == "/home/t", model.root(.left))
check("a symlinked directory is listed as one", names(model, .left).contains("link"), "\(names(model, .left))")
check("the Go menu offers the home directory and ~/Shellder",
      model.shortcuts(.left).contains { $0.path == "/home/t" } &&
      model.shortcuts(.left).contains { $0.path == "/home/t/Shellder" },
      "\(model.shortcuts(.left).map(\.path))")
check("and this Mac's own places", model.shortcuts(.right).contains { $0.path == Config.home },
      "\(model.shortcuts(.right).map(\.path))")

model.setRoot(.right, here)
pump(1)
check("a pane goes where it is told",
      model.root(.right) == here && names(model, .right) == ["clash.txt", "two.txt"],
      "\(model.root(.right)) \(names(model, .right))")

// MARK: copies

_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/Shellder")), into: here, on: .right)
let down = settle(model)
check("dragging a directory over copies it here", model.transfers.last?.state == .done,
      "\(model.transfers.last?.state ?? .waiting)")
check("the pane shows what arrived", names(model, .right).contains("Shellder"), "\(names(model, .right))")
check("with progress along the way", !down.isEmpty && down.allSatisfy { $0 > 0 && $0 < 1 }, "\(down)")
check("and every byte of it", LocalFS.bytes(here + "/Shellder") == RemoteFS.bytes("shjt-files", "/home/t/Shellder"))

_ = model.accept(dragged(FilesModel.payload(.local, here + "/Shellder")), into: "/home/t/my dir", on: .left)
settle(model)
check("dragging it back copies it to the host", model.transfers.last?.state == .done)
check("into a directory whose name has a space",
      RemoteFS.bytes("shjt-files", "/home/t/my dir/Shellder") == LocalFS.bytes(here + "/Shellder"))

_ = model.accept(draggedFromFinder(here + "/Shellder/one.txt"), into: "/home/t/my dir", on: .left)
settle(model)
check("a file dragged in from the Finder uploads too",
      RemoteFS.bytes("shjt-files", "/home/t/my dir/one.txt") == 4)

// MARK: the other pane can be another host

model.setSource(.right, hostB)
pump(4)
check("a pane can be pointed at another host",
      model.source(.right) == hostB && model.root(.right) == "/home/t/Shellder", model.root(.right))
_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/Shellder/one.txt")), into: "/home/t/Shellder", on: .right)
settle(model)
check("and a copy between two hosts goes through", model.transfers.last?.state == .done,
      "\(model.transfers.last?.state ?? .waiting)")
check("landing on the second host", RemoteFS.bytes("shjt-files-b", "/home/t/Shellder/one.txt") == 4)
check("the same host on both sides refuses the drag",
      !model.canAccept(dragged(FilesModel.payload(hostB, "/home/t")), on: .right))

// MARK: new folders, new files, deleting

model.selected[.right] = []
model.createDirectory(.right, named: "made here")
check("a new folder on a host reports where it went", settleNotice(model).contains("made here"), model.notice)
check("and shows up in the pane", names(model, .right).contains("made here"), "\(names(model, .right))")
model.createFile(.right, named: "note.txt")
_ = settleNotice(model)
check("a new file lands in the same place", names(model, .right).contains("note.txt"), "\(names(model, .right))")
model.createFile(.right, named: "note.txt")
_ = settleNotice(model)
check("a name that is taken is refused", model.noticeIsError, model.notice)
model.createDirectory(.right, named: "bad/name")
check("and so is a name with a slash in it", model.noticeIsError, model.notice)

model.delete(.right, ["/home/t/Shellder/made here"])
_ = settleNotice(model)
check("deleting on a host says it is gone", !model.noticeIsError && model.notice.contains("made here"), model.notice)
check("and it is", !names(model, .right).contains("made here"), "\(names(model, .right))")

model.setSource(.right, .local)
pump(2)
model.setRoot(.right, here)
pump(1.5)
model.createDirectory(.right, named: "made on this Mac")
_ = settleNotice(model)
check("a new folder on this Mac too", names(model, .right).contains("made on this Mac"), "\(names(model, .right))")
model.delete(.right, [here + "/made on this Mac"])
_ = settleNotice(model)
check("and deleting it here means the Trash",
      model.notice.contains("Trash") && !FileManager.default.fileExists(atPath: here + "/made on this Mac"),
      model.notice)

// MARK: the queue, stopping a copy, and what the list keeps

_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/Shellder/big.bin")), into: here, on: .right)
_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/Shellder/one.txt")), into: here, on: .right)
var waited = 0.0
while model.active == nil && waited < 5 { pump(0.05); waited += 0.05 }
check("a second drop waits for the first", model.queued == 1, "queued \(model.queued)")
while model.active != nil { pump(0.2) }
pump(0.5)
check("and both of them land", names(model, .right).contains("big.bin") && names(model, .right).contains("one.txt"),
      "\(names(model, .right))")

_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/huge.bin")), into: here, on: .right)
waited = 0
while model.active == nil && waited < 5 { pump(0.05); waited += 0.05 }
model.cancel()
while model.active != nil { pump(0.1) }
check("a copy can be stopped in the middle", model.transfers.last?.state == .cancelled,
      "\(model.transfers.last?.state ?? .waiting)")
let partial = LocalFS.bytes(here + "/huge.bin")
check("and it really stopped", partial < 600 * 1024 * 1024, "\(partial) bytes of 600 MB")
let onDisk = Set(((try? FileManager.default.contentsOfDirectory(atPath: here)) ?? [])
    .filter { !$0.hasPrefix(".") })
check("and the pane shows what is really there, half a file or none",
      Set(names(model, .right)) == onDisk, "pane \(names(model, .right)) disk \(onDisk)")

_ = model.accept(dragged(FilesModel.payload(.local, here + "/Shellder/one.txt")), into: "/etc", on: .left)
settle(model)
if case .failed(let why)? = model.transfers.last?.state {
    check("a refused copy keeps what scp said", why.lowercased().contains("permission"), why)
} else {
    check("a refused copy keeps what scp said", false, "\(model.transfers.last?.state ?? .waiting)")
}

let states = model.transfers.map(\.state)
check("the list kept every copy, in order",
      states.count == 8 && states.filter { $0 == .done }.count == 6
        && states.contains(.cancelled) && states.contains { if case .failed = $0 { return true }; return false },
      "\(states)")
check("a finished copy remembers its size and when it ended",
      model.transfers.first?.total ?? 0 > 0 && model.transfers.first?.endedAt != nil)
model.clearHistory()
check("clearing the list empties it", model.transfers.isEmpty, "\(model.transfers.count) left")

// MARK: a name that is already taken, and asking for a copy again

func text(_ path: String) -> String {
    ((try? String(contentsOfFile: path, encoding: .utf8)) ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
}
model.setRoot(.right, here)
pump(1.5)
model.clearHistory()
var asked = 0
model.askReplace = { _, _ in
    asked += 1
    return .skip
}
_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/clash.txt")), into: here, on: .right)
settleAll(model)
check("a copy that would write over something asks first", asked == 1, "asked \(asked) times")
check("and Skip leaves what was there alone",
      model.transfers.last?.state == .skipped && text(here + "/clash.txt") == "local",
      "\(model.transfers.last?.state ?? .waiting) \(text(here + "/clash.txt"))")

let skipped = model.transfers.last!
model.askReplace = { _, _ in
    asked += 1
    return .replace
}
model.retry(skipped)
settleAll(model)
check("a copy can be asked for again", model.transfers.count == 2 && model.transfers.last?.state == .done,
      "\(model.transfers.map(\.state))")
check("and Replace writes over it", text(here + "/clash.txt") == "remote", text(here + "/clash.txt"))

// one answer for a whole queue
try? "local".write(toFile: here + "/clash.txt", atomically: true, encoding: .utf8)
asked = 0
model.askReplace = { _, _ in
    asked += 1
    return .replaceAll
}
_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/clash.txt"),
                         FilesModel.payload(hostA, "/home/t/two.txt")), into: here, on: .right)
settleAll(model)
check("apply to all answers for the rest of the queue", asked == 1, "asked \(asked) times")
check("and both of them went over the old ones",
      text(here + "/clash.txt") == "remote" && text(here + "/two.txt") == "remote",
      "\(text(here + "/clash.txt")) \(text(here + "/two.txt"))")
model.askReplace = { _, _ in .replace }
model.clearHistory()

// MARK: more than one at a time

model.setRoot(.left, "/home/t/Shellder")
pump(2)
model.setRoot(.right, here)
pump(1.5)
_ = model.accept(dragged(FilesModel.payload(hostA, "/home/t/Shellder/one.txt"),
                         FilesModel.payload(hostA, "/home/t/Shellder/sub")), into: here, on: .right)
settle(model)
settle(model)
check("a drag of two rows queues both",
      model.transfers.suffix(2).allSatisfy { $0.state == .done }
        && names(model, .right).contains("sub") && names(model, .right).contains("one.txt"),
      "\(names(model, .right))")

model.selected[.left] = ["/home/t/Shellder/one.txt", "/home/t/Shellder/big.bin"]
check("what is selected comes back in the order the rows are drawn",
      model.selection(.left) == ["/home/t/Shellder/big.bin", "/home/t/Shellder/one.txt"],
      "\(model.selection(.left))")
check("and the copy arrow is ready for it", model.canCopySelection(from: .left))

// The pane knows the paths it is showing (a local one may be spelled
// /private/tmp), so the rows themselves say what to delete.
model.selected[.right] = Set(model.rows(.right)
    .filter { ["one.txt", "sub"].contains($0.item.name) }
    .map(\.item.path))
check("two rows can be selected at once", model.selection(.right).count == 2, "\(model.selection(.right))")
model.delete(.right, model.selection(.right))
_ = settleNotice(model)
check("deleting several says how many", model.notice.contains("2 items"), model.notice)
check("and they are all gone",
      !names(model, .right).contains("sub") && !names(model, .right).contains("one.txt"),
      "\(names(model, .right))")

// MARK: drops that mean nothing, and where one lands

check("a pane refuses a drag from its own side",
      !model.canAccept(dragged(FilesModel.payload(.local, here)), on: .right)
        && model.canAccept(dragged(FilesModel.payload(.local, here)), on: .left))
check("and text that is not one of our rows",
      !model.canAccept(dragged("hello"), on: .left) && !model.canAccept(dragged("hello"), on: .right))
check("a file from the Finder is for a host's side only",
      model.canAccept(draggedFromFinder(here), on: .left) && !model.canAccept(draggedFromFinder(here), on: .right))
check("a drop on a directory goes into it",
      model.destination(for: FileItem(name: "sub", path: "/home/t/Shellder/sub", isDir: true, size: 0), on: .left)
        == "/home/t/Shellder/sub")
check("a drop on a file goes into the directory around it",
      model.destination(for: FileItem(name: "one.txt", path: "/home/t/Shellder/one.txt", isDir: false, size: 4), on: .left)
        == "/home/t/Shellder")

model.setRoot(.left, "/home/t/nope")
pump(2)
check("a directory that is not there shows the error", (model.errors[.left] ?? "").contains("No such file"),
      model.errors[.left] ?? "none")

print(failures.isEmpty ? "PASS: the Files window's model does what the panes ask of it"
                       : "FAIL: \(failures.count) check(s) failed")
exit(failures.isEmpty ? 0 : 1)
SWIFT
swiftc -o $DIR/filestest Sources/shellder/App/*.swift Sources/shellder/App/Tools/*.swift \
    Sources/shellder/Core/*.swift $DIR/main.swift
$DIR/filestest
