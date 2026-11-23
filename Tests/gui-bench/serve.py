#!/usr/bin/env python3
"""The other end of Tests/gui-bench.sh: writes the output the terminals
are to show into DIR, serves probe.js to the app (SHELLDER_PROBE) with
what it needs to know in front of it, prints the lines the probe says,
and ends when it says DONE (0), or after SECONDS without that (1).

    serve.py DIR PORT MEGABYTES SECONDS
"""
import http.server
import json
import os
import sys
import threading
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
COLS, ROWS = 120, 40


def plain(i):
    return "the quick brown fox jumps over the lazy dog 0123456789".ljust(62, "x") + "\r\n"


def coloured(i):
    return (
        f"\x1b[1;32m   Compiling\x1b[0m crate-{i % 97} v0.{i % 13}.2 "
        f"\x1b[38;5;244m(/home/t/src/crate-{i % 97})\x1b[0m "
        f"\x1b[38;2;255;128;0mwarning\x1b[0m: unused variable \x1b[1m`x{i % 31}`\x1b[0m "
        f"\x1b[4;34msrc/lib.rs:{i % 900}\x1b[0m\r\n"
    )


def wide(i):
    return f"终端里的中文输出，宽字符占两格 🙂 日本語のテキスト 한국어 {i % 1000} 文件列表与日志\r\n"


def screen(i):
    """A full-screen program redrawing itself: every row addressed and
    painted, nothing scrolling."""
    s = "\x1b[H"
    for r in range(1, ROWS + 1):
        text = f"{i:6} │ " + f"row {r} of the screen " * 6
        s += f"\x1b[{r};1H\x1b[48;5;{(i + r) % 6 + 17}m\x1b[38;5;{(i * 7 + r) % 200 + 20}m{text[:COLS - 1]}\x1b[0m"
    return s


def write(path, megabytes, line):
    """`line(i)` over and over up to the size, ended where a line ends."""
    block = "".join(line(i) for i in range(2000)).encode()
    want = int(megabytes * 1048576)
    with open(path, "wb") as f:
        for _ in range(max(1, want // len(block))):
            f.write(block)
    return os.path.getsize(path)


def main():
    directory, port, megabytes, seconds = sys.argv[1], int(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4])
    works = []
    for name, file, share, line in [
        ("plain lines", "plain.bin", 1, plain),
        ("coloured lines", "coloured.bin", 0.5, coloured),
        ("wide characters (CJK, emoji)", "wide.bin", 0.5, wide),
        ("a full screen redrawn", "screen.bin", 0.5, screen),
    ]:
        size = write(os.path.join(directory, file), megabytes * share, line)
        works.append({"name": name, "file": os.path.join(directory, file), "bytes": size})
    # The marks the probe times a run by: an OSC of its own before and
    # after the file, which xterm hands it when it has parsed that far.
    with open(os.path.join(directory, "play.sh"), "w") as f:
        # The tab stays until the probe closes it: one whose command
        # ends well is closed by the page at once, with whatever xterm
        # had not parsed yet, the second mark too.
        f.write("printf '\\033]7777;start\\007'\ncat \"$1\"\nprintf '\\033]7777;end\\007'\nexec sleep 3600\n")
    bench = {"origin": f"http://127.0.0.1:{port}", "debug": bool(os.environ.get("GUI_BENCH_DEBUG")), "works": works, "play": os.path.join(directory, "play.sh")}
    done = threading.Event()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            url = urllib.parse.urlparse(self.path)
            if url.path == "/probe.js":
                with open(os.path.join(HERE, "probe.js"), "rb") as f:
                    body = f"window.__BENCH = {json.dumps(bench)};\n".encode() + f.read()
                self.send_response(200)
                self.send_header("Content-Type", "text/javascript; charset=utf-8")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            if url.path == "/say":
                line = urllib.parse.parse_qs(url.query).get("l", [""])[0]
                if line == "DONE":
                    done.set()
                else:
                    print(line, flush=True)
            self.send_response(204)
            self.end_headers()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    if not done.wait(seconds):
        print("the probe did not finish in time", file=sys.stderr)
        sys.exit(1)


main()
