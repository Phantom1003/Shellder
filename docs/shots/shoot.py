#!/usr/bin/env python3
"""Takes the documentation's pictures and writes the figures that name
their parts: every scene of scenes.py, in English and in Chinese.

    docs/shots/shoot.py [--keep-data] [SCENE ...]

Nothing of the app is started and no window comes on screen: the pages
of ui/ are drawn by the system's WebKit in a window that is on no screen
(shoot.swift), over a stand-in for the app (stub.js) that answers their
calls with data the real code made for hosts that do not exist
(src-tauri/tests/doc_data.rs: the model's state of a made-up ssh config,
the readings' parser on its fixture, the Files model on a small tree).
macOS, with swiftc and cargo.

Writes docs/images/<lang>/<scene>.png and docs/<lang>/src/figures/
<scene>.html, the picture with a numbered pin on every part and the
parts' names underneath, taken from docs/terms.py. A page of a book has
`{{#include figures/<scene>.html}}` where the figure goes. The scenes
with `window` (front-main, front-console) are the README's pictures:
the page as a macOS window, with its corners and traffic lights, on a
backdrop (frame.swift), and no figure. `--keep-data` does not run the
test again when its files are there. `--demo` takes no picture: it
writes docs/site/demo/data.js, the same data for the front page's demo,
where the pages of ui/ run in the visitor's browser over
docs/site/demo/backend.js.
"""
import http.server
import json
import os
import subprocess
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(ROOT, "build", "shots")
DATA = os.path.join(WORK, "data")
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)
import terms  # noqa: E402
import scenes  # noqa: E402

LANGS = {"en": "en", "zh": "zh-Hans"}


def make_data():
    os.makedirs(DATA, exist_ok=True)
    subprocess.run(
        ["cargo", "test", "--quiet", "--test", "doc_data", "--", "--ignored"],
        cwd=os.path.join(ROOT, "src-tauri"), env={**os.environ, "DOC_DATA_OUT": DATA}, check=True,
    )


def load():
    """The test's files as one object, with nothing of this machine left
    in them: its paths become a made-up user's."""
    root = open(os.path.join(DATA, "ROOT")).read()
    work = os.path.join(os.path.realpath(DATA), "work")
    home = os.path.expanduser("~")
    user = os.path.basename(home)
    doc = {}
    for name in ("state", "reading", "files"):
        text = open(os.path.join(DATA, name + ".json")).read()
        for old, new in [
            (root, "/home/alice"),
            (os.path.join(work, "config"), "/Users/alice/.ssh/config"),
            (os.path.join(work, "state"), "/Users/alice/Library/Application Support/shellder"),
            (os.path.join(DATA, "work", "config"), "/Users/alice/.ssh/config"),
            (os.path.join(DATA, "work", "state"), "/Users/alice/Library/Application Support/shellder"),
            (home, "/Users/alice"),
        ]:
            text = text.replace(json.dumps(old)[1:-1], new)
        text = text.replace(f'"{user}"', '"alice"')
        assert user not in text, f"{name}.json still names this machine's user"
        doc[name] = json.loads(text)
    scenes.dress(doc)
    doc["scenes"] = {
        s["name"]: {
            "setup": s.get("setup", ""), "before": s.get("before"), "settle": s.get("settle", 500), "crop": s.get("crop"), "ends": s.get("ends"),
            "state": s.get("state"), "terminals": s.get("terminals"), "debug": s.get("debug"),
            "mac": bool(s.get("window")),
            "parts": {p[0]: p[1] for p in s["parts"]},
        }
        for s in scenes.SCENES
    }
    return doc


def write_demo(doc):
    """What the front page's demo starts from: the app's state as the
    pictures have it, a host's reading, the log and the forwards, and the
    time they are of, which the demo moves to the visitor's."""
    out = {k: doc[k] for k in ("state", "reading", "log", "forwards")}
    out["now"] = int(scenes.NOW)
    path = os.path.join(ROOT, "docs", "site", "demo", "data.js")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write("// Written by docs/shots/shoot.py --demo, from what the app's own code says\n"
                "// of hosts that do not exist (src-tauri/tests/doc_data.rs): not by hand.\n"
                "window.__SHELLDER_DEMO = " + json.dumps(out, ensure_ascii=False, indent=1) + ";\n")
    print("wrote " + os.path.relpath(path, ROOT))


def serve(doc):
    """The pages of ui/ with the stand-in put in front of each."""
    data = ("window.__DOC = " + json.dumps(doc, ensure_ascii=False) + ";\n").encode()
    inject = b'<script src="/__shots/data.js"></script><script src="/__shots/stub.js"></script>'

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **k):
            super().__init__(*a, directory=os.path.join(ROOT, "ui"), **k)

        def log_message(self, *a):
            pass

        def send(self, body, kind):
            self.send_response(200)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            path = self.path.split("?")[0]
            if path == "/__shots/data.js":
                return self.send(data, "text/javascript; charset=utf-8")
            if path == "/__shots/stub.js":
                return self.send(open(os.path.join(HERE, "stub.js"), "rb").read(), "text/javascript; charset=utf-8")
            if path.endswith(".html"):
                page = open(os.path.join(ROOT, "ui", path.lstrip("/")), "rb").read()
                return self.send(page.replace(b"<head>", b"<head>" + inject, 1), "text/html; charset=utf-8")
            return super().do_GET()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server.server_address[1]


def figure(scene, lang, rects, width):
    """The picture with its pins, and the parts' names under it: HTML
    without a blank line in it, so that Markdown leaves it alone."""
    en = lang == "en"
    alt = scene["alt"][0 if en else 1]
    lines = dict((e[0], (e[2], e[3])) for e in terms.DATA if e[0] is not terms.S)
    # No wider than the page it is of: a narrow one (a side bar alone)
    # would be blown up to the column's width otherwise.
    out = [f'<figure class="shot" style="max-width:{round(width)}px">', '<div class="shot-frame">',
           f'<img src="../images/{lang}/{scene["name"]}.png" alt="{alt}" loading="lazy">']
    legend = []
    n = 0
    for part in scene["parts"]:
        term, at = part[0], (part[2] if len(part) > 2 else "tl")
        if term not in rects:
            continue
        n += 1
        x, y, w, h = rects[term]
        px = x + {"l": 0, "c": w / 2, "r": w}[at[1] if len(at) > 1 else "l"]
        py = y + {"t": 0, "m": h / 2, "b": h}[at[0]]
        # A pin is drawn round its point: kept off the picture's edge.
        px, py = min(max(px, 0.012), 0.988), min(max(py, 0.02), 0.98)
        out.append(f'<span class="shot-box" data-n="{n}" style="left:{x:.4%};top:{y:.4%};width:{w:.4%};height:{h:.4%}"></span>')
        out.append(f'<span class="shot-pin" data-n="{n}" style="left:{px:.4%};top:{py:.4%}">{n}</span>')
        if len(part) > 3:
            # A part with no term of its own: named and described here.
            name, line = part[3]["en" if en else "zh"]
        else:
            name = term if en else terms.TERMS[term]
            line = lines[term][0 if en else 1]
        legend.append(f'<li data-n="{n}"><b>{name}</b>{": " if en else "："}{line}</li>')
    out += ["</div>", '<ol class="shot-legend">'] + legend + ["</ol>", "</figure>"]
    return "\n".join(out) + "\n"


def main():
    args = sys.argv[1:]
    keep = "--keep-data" in args
    wanted = [a for a in args if not a.startswith("--")]
    if not (keep and os.path.exists(os.path.join(DATA, "state.json"))):
        make_data()
    doc = load()
    if "--demo" in args:
        return write_demo(doc)
    port = serve(doc)
    shoot = os.path.join(WORK, "shoot")
    subprocess.run(["swiftc", "-O", os.path.join(HERE, "shoot.swift"), "-o", shoot], check=True)
    frame = os.path.join(WORK, "frame")
    subprocess.run(["swiftc", "-O", os.path.join(HERE, "frame.swift"), "-o", frame], check=True)
    failed = False
    for scene in scenes.SCENES:
        if wanted and scene["name"] not in wanted:
            continue
        for lang, code in LANGS.items():
            os.makedirs(os.path.join(ROOT, "docs", "images", lang), exist_ok=True)
            os.makedirs(os.path.join(ROOT, "docs", lang, "src", "figures"), exist_ok=True)
            png = os.path.join(ROOT, "docs", "images", lang, scene["name"] + ".png")
            query = scene.get("query", "")
            url = f"http://127.0.0.1:{port}/{scene['page']}?scene={scene['name']}&lang={code}&label={scene.get('label', 'main')}{'&' + query if query else ''}"
            w, h = scene["size"]
            r = subprocess.run([shoot, url, str(w), str(h), png], capture_output=True, text=True)
            if r.returncode:
                print(f"{scene['name']} ({lang}): {r.stderr.strip()}")
                failed = True
                continue
            shot = json.loads(r.stdout)
            notes = []
            if shot["missing"]:
                notes.append("not found: " + ", ".join(shot["missing"]))
                failed = True
            if shot["errors"]:
                notes.append("the page said: " + "; ".join(shot["errors"])[:600])
            if shot.get("debug"):
                notes.append("debug: " + shot["debug"])
            if shot["unknown"]:
                notes.append("unanswered: " + ", ".join(shot["unknown"]))
            if scene.get("window"):
                # The page alone so far: now the window it is on macOS.
                subprocess.run([frame, png, png, "2"], check=True)
                print(f"{scene['name']} ({lang}): a window" + "".join("\n    " + n for n in notes))
                continue
            print(f"{scene['name']} ({lang}): {len(shot['rects'])} parts" + "".join("\n    " + n for n in notes))
            with open(os.path.join(ROOT, "docs", lang, "src", "figures", scene["name"] + ".html"), "w") as f:
                f.write(figure(scene, lang, shot["rects"], shot["crop"][2]))
    sys.exit(1 if failed else 0)


main()
