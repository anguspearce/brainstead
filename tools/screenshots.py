#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Retake the docs screenshots, docs/images/<scene>-<theme>.png, and check UI changes without
clicking through the app.

Makes a demo in .demo/ (gitignored): the fixture vault (tests/fixtures/vault) copied into a demo
home, and app data whose settings open it. Then it launches the dev build once per scene and
theme with the scene in BRAINSTEAD_SCENE (the app saves nothing). The window is invisible and
takes no focus, so nothing flashes on screen: the app saves its webview's snapshot to
BRAINSTEAD_SNAPSHOT, and the window's buttons and rounded corners are drawn back on it here.
Nothing outside .demo/ is read or written; the real vault is never opened.

    python3 tools/screenshots.py                 # every scene, light and dark
    python3 tools/screenshots.py settings-vault  # one scene
    python3 tools/screenshots.py --theme dark    # one theme
    python3 tools/screenshots.py --fresh         # remake the demo first
    python3 tools/screenshots.py --docs          # only the scenes the docs show
    python3 tools/screenshots.py -j 1            # one at a time (default: 4 side by side)
    python3 tools/screenshots.py --release       # the built app (make app), with its own CSP

Needs the Vite dev server (started here if it isn't running) and Pillow.
"""

import argparse
import concurrent.futures
import datetime
import gzip
import hashlib
import io
import json
import os
import pathlib
import queue
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

from PIL import Image, ImageChops, ImageCms, ImageDraw, ImageStat

ROOT = pathlib.Path(__file__).resolve().parent.parent
HERE = ROOT / "tools" / "screenshots"
# The demo's writers (demo_wiki.py, demo_tutorial.py) sit beside the scenes.
sys.path.insert(0, str(HERE))
DEMO = ROOT / ".demo"
# Scenes the docs show ("docs": true) go to docs/images/ and are committed; the rest, for checking
# screens while building, go to a gitignored folder.
OUT = ROOT / "docs" / "images"
# The product page's pictures: a scene with a picture there (site/img/<name>-light.webp) is saved
# there too, from the same snapshot at its full (2x) size, so it stays sharp on a Retina screen.
SITE = ROOT / "site" / "img"
CHECK = ROOT / "tools" / "screenshots" / "out"
BIN = ROOT / "src-tauri" / "target" / "debug" / "Brainstead"
RELEASE_BIN = ROOT / "src-tauri" / "target" / "release" / "bundle" / "macos" / "Brainstead.app" / "Contents" / "MacOS" / "Brainstead"
DEV_URL = "http://localhost:1440"
FIXTURE = ROOT / "tests" / "fixtures" / "vault"
WIDTH = 1400
SETTLE = 5.0
# How long past its delay a snapshot may take to come before the launch is given up as failed
# (normally it's there within a second or two).
GRACE = 10.0
# Apps running side by side (-j): each settles more slowly, so each waits a little longer.
PARALLEL = 1


def to_srgb(im):
    icc = im.info.get("icc_profile")
    if icc:
        alpha = im.getchannel("A") if im.mode == "RGBA" else None
        src = ImageCms.ImageCmsProfile(io.BytesIO(icc))
        im = ImageCms.profileToProfile(im.convert("RGB"), src, ImageCms.createProfile("sRGB"), renderingIntent=ImageCms.Intent.PERCEPTUAL)
        if alpha:
            im.putalpha(alpha)
    im.info.pop("icc_profile", None)
    return im


def dev_server():
    try:
        urllib.request.urlopen(DEV_URL, timeout=1)
        return None
    except OSError:
        pass
    p = subprocess.Popen(["npx", "vite", "--port", "1440", "--strictPort"], cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(60):
        time.sleep(0.5)
        try:
            urllib.request.urlopen(DEV_URL, timeout=1)
            return p
        except OSError:
            pass
    sys.exit("the Vite dev server didn't start")


def build():
    subprocess.run(["cargo", "build"], cwd=ROOT / "src-tauri", check=True)


def env(data=DEMO / "data"):
    # HOME is the demo's, so paths show as ~/Notes and not as where this repo is on this Mac.
    return {**os.environ, "HOME": str(DEMO / "Home"), "BRAINSTEAD_DATA": str(data)}


def make_demo():
    shutil.rmtree(DEMO, ignore_errors=True)
    vault = DEMO / "Home" / "Notes"
    shutil.copytree(FIXTURE, vault)
    # Demo-only wiki pages around the fixture's two, so the Graph and the pies are well filled.
    import demo_wiki

    demo_wiki.write(vault)
    # The website tutorials' own vault and first run (scenes with "data").
    import demo_tutorial

    demo_tutorial.write(DEMO, DEMO / "Home")
    (DEMO / "data").mkdir(parents=True)
    settings = {"vaultPath": str(vault), "excluded": ["archived"], "readOnly": True, "theme": "system"}
    (DEMO / "data" / "settings.json").write_text(json.dumps(settings, indent=2))
    # A few agent changes in Changes (src-tauri/core/src/changes.rs), from the texts in proposals/
    # (made by the MCP server against the fixture vault): one held from the daily check, the rest
    # made from a chat.
    demo_changes(DEMO / "data" / "changes", vault)
    # An email thread from the Outlook extension, waiting in the Inbox.
    cap = "Email. Thread. Re Orbit App launch date - 2026-10-02.md"
    (vault / "sources" / cap).write_text(
        "# Re Orbit App launch date\n\n**Source:** [Open in Outlook](https://outlook.office.com/mail/)\n"
        "**Captured:** 2026-10-02T07:40:00.000Z\n\n---\n\n## Maya — Thu 2 Oct 08:12\n\n**To:** Lena\n\n"
        "Steerco agreed: soft launch to staff moves to 14 November. Theo to update the comms plan.\n\n---\n"
    )
    captured = [{"extension": "outlook", "what": "Email thread", "path": f"sources/{cap}", "error": None, "at": "2026-10-02T07:40:00.000Z"}]
    (DEMO / "data" / "captures.json").write_text(json.dumps(captured, indent=2))
    # A contradictions check's findings, for the `contradictions` scene.
    people = ["Maya Chen", "Lena Ortiz", "Theo Park", "Sam Carter", "Priya Nair", "Jonas Berg", "Ada Okafor", "Ravi Shah"]
    things = ["Orbit App", "Hub Platform", "Steerco Charter", "Pilot Programme", "Roadmap Update", "Launch Comms"]
    clashes, verdicts = [], []
    kinds = [("contradiction", "medium"), ("compatible", ""), ("evolution", ""), ("compatible", "")]
    for i, subject in enumerate(sorted(people + things)):
        person = subject in people
        attribute = "role" if person else "launch_date"
        values = ["Product lead", "Delivery lead"] if person else ["14 November", "1 December"]
        cid = f"demo{i:02d}"
        clashes.append(
            {
                "id": cid,
                "subject": subject,
                "attribute": attribute,
                "claims": [
                    {"page": f"wiki/{subject}.md", "value": values[0], "asOf": "2026-09-18", "quote": values[0]},
                    {"page": "wiki/Orbit App.md", "value": values[1], "asOf": "2026-10-01", "quote": values[1]},
                ],
            }
        )
        verdict, severity = kinds[i % len(kinds)]
        summary = f"{subject}: two pages give different values"
        verdicts.append({"id": cid, "verdict": verdict, "severity": severity, "summary": summary, "judged": "2026-10-04T21:00:00Z"})
    cdir = DEMO / "data" / "contradictions"
    cdir.mkdir(parents=True, exist_ok=True)
    (cdir / "clashes.json").write_text(json.dumps(clashes, indent=2))
    (cdir / "verdicts.jsonl").write_text("".join(json.dumps(v) + "\n" for v in verdicts))
    # Two Teams transcripts, for the `meeting` scene.
    for topic, day in (("Orbit App Steerco", "2026-10-01"), ("Orbit App launch plan", "2026-10-02")):
        (vault / "sources" / f"Teams. Transcript. {topic} - {day}.md").write_text(
            f"# {topic}\n\n**Source:** [Open meeting recap](https://teams.microsoft.com/)\n**Meeting:** {day}\n"
            f"**Captured:** {day}T09:30:00.000Z\n\n---\n\n## Maya — 0 minutes 4 seconds\n\n0:04\n\nShall we start with the launch date?\n"
        )
    # A template with a tag in a tasks query, for the `template-assist` scenes.
    (vault / "Templates" / "1-1.md").write_text(
        '<%* const name = await tp.system.prompt("Name?") -%>\n# 1-1 with <% name %>\n\n## Actions\n- \n\n'
        "### Followups\n\n```tasks\nnot done\n(tags include #followup/<% name %>)\n```\n\n## Notes\n<% tp.file.cursor(1) -%>\n"
        + "".join(f"\n## Topic {i}\n- \n" for i in range(1, 13))
    )
    # A note with mistakes in it, for the `spelling` scene.
    (vault / "Idea. Spelling.md").write_text(
        "# Spelling\n\nThis sentense has a mistake in it. He went to the the shop.\n\nA [[Orbit App]] link and `cde` aren't checked.\n"
    )
    # Tasks inside a callout and a quote, and callouts of a few types, for the `callout-tasks` scenes.
    (vault / "Idea. Before the frost.md").write_text(
        "# Before the frost\n\n> [!todo] This weekend\n> - [ ] Cover the beds #context/home 📅 2026-10-04\n"
        "> - [x] Order fleece ✅ 2026-10-01\n\n> Lena's tip:\n> > - [ ] Move the pots against the wall\n\n"
        "> [!warning] Frost due on Tuesday\n> Bring the chillies in first.\n\n> [!tip]+ Fleece\n> Two layers on the coldest nights.\n\n"
        "> [!example]- Last year's list\n> Beds, pots, the water butt.\n\n> [!quote] Theo\n> Better a day early than a week late.\n"
    )
    # Today's daily summary, for the `today-summary` scene's card.
    today = datetime.date.today()
    (vault / f"Me. Daily Summaries - {today:%Y-%m}.md").write_text(
        f"> **This is a system note** - Append-only log of the daily summary for {today:%Y-%m}.\n\n"
        f"## Daily summary {today:%Y-%m-%d}\n\n### Done\n\n- Agreed the Orbit App soft launch date with Maya.\n"
        "- Wrote up the [[Meeting. Orbit App Steerco - 2026-09-30|steerco]] actions.\n\n"
        "### Still open\n\n- Lena's comms plan is waiting on the launch date.\n"
    )
    # Log lines quoting tasks, whose fields Activity draws as icons, for the `activity` scene.
    log = vault / "log.md"
    head, rest = log.read_text().split("\n## ", 1)
    log.write_text(
        head
        + "\n## [2026-10-01 09:12] update | Me. To Do List\nDelete “Lena to send the comms plan #waiting-for ✅ 2026-09-30”\n\n"
        + "## [2026-10-01 09:10] update | Project. Orbit App launch\nAdd “Book the launch venue 📅 2026-10-20 ⏫”\n\n## "
        + rest
    )
    # Find tasks and projects' suggestions (src-tauri/src/find.rs), for the `tasks` and `projects` scenes.
    steerco = "Meeting. Orbit App Steerco - 2026-09-30.md"
    src = [{"path": steerco, "quote": "Discussed [[Orbit App]] and the [[OA|app]] launch date"}]
    find = {
        "run": {
            "id": "find-demo",
            "startedAt": "2026-10-01T09:00:00",
            "finishedAt": "2026-10-01T09:02:00",
            "status": "done",
            "model": "claude:sonnet",
            "notes": 18,
            "batches": 1,
            "done": 1,
            "found": 3,
            "dropped": 1,
        },
        "suggestions": [
            {
                "id": "demo-t1",
                "kind": "task",
                "text": "Confirm the launch date with Sam",
                "project": "Orbit App launch",
                "waiting": False,
                "tasks": [],
                "sources": src,
            },
            {"id": "demo-t2", "kind": "task", "text": "Zara: launch comms draft", "waiting": True, "tasks": [], "sources": src},
            {
                "id": "demo-p1",
                "kind": "project",
                "text": "Launch comms",
                "outcome": "Staff know what changes on launch day.",
                "waiting": False,
                "tasks": ["Ask Zara for the comms plan"],
                "sources": src,
            },
        ],
        "decided": [],
    }
    (DEMO / "data" / "find").mkdir(parents=True)
    (DEMO / "data" / "find" / "state.json").write_text(json.dumps(find, indent=2))
    # Two notes in the Trash, as Brainstead's own delete leaves them (core/src/trash.rs).
    trashed = [
        ("Idea. Team offsite - 2026-09-12.md", "note", "# Team offsite\n\nA day at the lake.\n", "2026-10-01T15:20:00.000Z"),
        ("wiki/concepts/Old Pricing.md", "wiki", "---\ntype: concept\n---\n\n# Old Pricing\n\nReplaced.\n", "2026-09-28T09:05:00.000Z"),
    ]
    for i, (rel, layer, body, when) in enumerate(trashed):
        tid = f"demo{i}"
        base = rel.rsplit("/", 1)[-1]
        d = vault / ".trash" / tid
        d.mkdir(parents=True)
        (d / base).write_text(body)
        meta = {"id": tid, "originalRel": rel, "layer": layer, "basename": base, "deletedAt": when}
        (d / "meta.json").write_text(json.dumps(meta, indent=2))


def demo_changes(out, vault):
    """Writes Changes' records and their compressed texts, as the app keeps them."""
    (out / "text").mkdir(parents=True)

    def put(text):
        h = hashlib.sha256(text.encode()).hexdigest()
        (out / "text" / f"{h}.gz").write_bytes(gzip.compress(text.encode()))
        return h

    held = {"demo-1"}
    for f in sorted((HERE / "proposals").glob("*.json")):
        p = json.loads(f.read_text())
        c = {k: p[k] for k in ("id", "created", "model", "page", "kind", "title", "reason", "quotes", "warnings") if k in p}
        if p["id"] in held:
            c["origin"] = {"kind": "ingest", "label": "Ingest of Steerco minutes", "run": "daily-check-demo", "trigger": "scheduled"}
            # Its changed lines as find and replace, as from_texts makes them, so it runs on the page as it is.
            pairs = [(a, b) for a, b in zip(p["before"].split("\n"), p["after"].split("\n"), strict=False) if a != b and a.strip()]
            c["instruction"] = {"op": "replace", "edits": [{"find": a, "replace": b} for a, b in pairs[:1]]}
            c["status"] = "held"
            c["flags"] = ["This quote isn't in sources/Steerco minutes.md: “launch moved to 28 November”"]
        else:
            c["origin"] = {**p["origin"], "run": p["origin"].get("chat")}
            c["instruction"] = {"op": "page", "content": p["after"]}
            c["status"] = "applied"
            c["decided"] = p["created"]
            c["flags"] = []
            if p["before"]:
                c["before"] = put(p["before"])
            c["after"] = put(p["after"])
            # The demo vault as the change left it.
            (vault / p["page"]).parent.mkdir(parents=True, exist_ok=True)
            (vault / p["page"]).write_text(p["after"])
        (out / f"{p['id']}.json").write_text(json.dumps(c, indent=2))


def capture(scene, theme, data, settle):
    """Launches the app on the scene, unseen, and returns its webview's snapshot, taken `settle`
    seconds in, as an sRGB image, or None when none came."""
    sc = {k: v for k, v in scene.items() if k not in ("crop", "width", "docs", "data")}
    sc["theme"] = theme
    if scene.get("data") and "path" in sc:
        import demo_tutorial

        sc["path"] = demo_tutorial.fill(sc["path"], datetime.datetime.now())
    with tempfile.TemporaryDirectory() as tmp:
        shot = pathlib.Path(tmp) / "shot.tiff"
        app = subprocess.Popen(
            [str(BIN)],
            cwd=ROOT / "src-tauri",
            env={
                **env(data),
                "BRAINSTEAD_SCENE": json.dumps(sc),
                "BRAINSTEAD_SNAPSHOT": str(shot),
                "BRAINSTEAD_SNAPSHOT_AFTER": str(settle),
            },
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            # The app writes the file whole (atomically) once WebKit has drawn it; when it hasn't
            # shortly after the delay (the app quit, or the snapshot failed), this launch failed.
            end = time.time() + settle + GRACE + 2 * (PARALLEL - 1)
            while not shot.exists() and app.poll() is None and time.time() < end:
                time.sleep(0.2)
            if not shot.exists():
                return None
            im = Image.open(shot)
            im.load()
            return to_srgb(im.convert("RGBA") if im.mode not in ("RGB", "RGBA") else im)
        finally:
            app.send_signal(signal.SIGKILL)
            app.wait()


def chrome(im, theme, s=1.0):
    """Draws back what a webview snapshot leaves out: the window's (unfocused) buttons and its
    rounded corners, as a capture of the window showed them. `s`: the shot's size over the docs'."""
    k = 4  # drawn large and scaled down, for smooth edges
    w, h = im.size
    over = Image.new("RGBA", (w * k, h * k))
    d = ImageDraw.Draw(over)
    fill, rim = ((230, 231, 233), (220, 221, 223)) if theme == "light" else ((126, 127, 129), (140, 141, 143))
    for cx in (23.5, 46, 68.5):
        cx, cy, r = cx * s, 19 * s, 7 * s
        d.ellipse([(cx - r) * k, (cy - r) * k, (cx + r) * k, (cy + r) * k], fill=fill + (255,), outline=rim + (255,), width=round(k * s))
    im = Image.alpha_composite(im.convert("RGBA"), over.resize((w, h), Image.LANCZOS))
    mask = Image.new("L", (w * k, h * k))
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, w * k - 1, h * k - 1], radius=round(16 * k * s), fill=255)
    im.putalpha(mask.resize((w, h), Image.LANCZOS))
    return im


def blank(im):
    # An undrawn window is one flat colour but for its traffic lights.
    return ImageStat.Stat(im.convert("L")).stddev[0] < 4


def same(a, b):
    """Whether two finished shots show the same screen: nearly no pixel differs at a small size
    (a blinking caret or a spinner may)."""
    if a is None or b is None or a.size != b.size:
        return False
    small = (max(1, a.width // 4), max(1, a.height // 4))
    d = ImageChops.difference(a.convert("L").resize(small), b.convert("L").resize(small))
    changed = sum(d.histogram()[24:])
    return changed / (small[0] * small[1]) < 0.002


def shoot(scene, theme, data):
    # A shot is kept once it's settled: the same as the scene's last kept shot, or as a second one
    # taken later. So one caught mid-load ("Loading…", "Reading your vault") is taken again, later,
    # rather than kept. One that came out blank (or never came) is taken again too.
    out = (OUT if scene.get("docs") else CHECK) / f"{scene['name']}-{theme}.png"
    last = None
    if out.exists():
        with Image.open(out) as f:
            last = f.convert("RGBA")
    settle = SETTLE + (PARALLEL - 1)
    prev = None
    why = "no window"
    for _ in range(4):
        raw = capture(scene, theme, data, settle)
        settle += 3
        if raw is None or blank(raw):
            why = "no window" if raw is None else "blank"
            continue
        im = finish(raw, scene, theme)
        if same(im, last) or same(im, prev):
            break
        prev, why = im, "still changing"
    else:
        print(f"  {scene['name']} {theme}: {why}")
        return False
    im.save(out, optimize=True)
    print(f"  {out.relative_to(ROOT)}  {im.width}x{im.height}")
    site = SITE / f"{scene['name']}-{theme}.webp"
    if site.exists():
        big = finish(raw, scene, theme, full=True)
        big.save(site, "WEBP", quality=90, method=6)
        print(f"  {site.relative_to(ROOT)}  {big.width}x{big.height}")
    # A picture shown at its own size (the tutorials' cards) has a 1x copy beside it, for srcset:
    # a screen at 1x then gets the app's own pixels, not a 2x picture scaled down to a blur.
    small = SITE / f"{scene['name']}-{theme}@1x.webp"
    if small.exists():
        im.save(small, "WEBP", quality=92, method=6)
        print(f"  {small.relative_to(ROOT)}  {im.width}x{im.height}")
    return True


def finish(im, scene, theme, full=False):
    """The snapshot as it's saved: at the scene's width (or, `full`, its own size, never larger),
    with the window's chrome, cropped."""
    # A scene may keep its own width (the menu-bar window, at its natural 2x size).
    width = scene.get("width", WIDTH)
    s = max(1.0, im.width / width) if full else 1.0
    size = round(width * s)
    im = im.resize((size, round(im.height * size / im.width)), Image.LANCZOS)
    if not scene.get("tray"):
        im = chrome(im, theme, s)
    if "crop" in scene:
        x, y, w, h = (round(v * s) for v in scene["crop"])
        im = im.crop((x, y, x + w, y + h))
    return im


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*")
    ap.add_argument("--theme", choices=["light", "dark"], action="append")
    ap.add_argument("--fresh", action="store_true", help="remake the demo data")
    ap.add_argument("--docs", action="store_true", help="only the scenes the docs show")
    ap.add_argument("-j", type=int, default=4, metavar="N", help="apps to run side by side (default 4)")
    ap.add_argument("--release", action="store_true", help="run the built app (make app) instead of the dev build")
    a = ap.parse_args()
    global BIN
    if a.release:
        if not RELEASE_BIN.exists():
            sys.exit("no built app: run make app first")
        BIN = RELEASE_BIN
    scenes = json.loads((HERE / "scenes.json").read_text())
    if a.names:
        scenes = [s for s in scenes if s["name"] in a.names]
    if a.docs:
        scenes = [s for s in scenes if s.get("docs")]
    themes = a.theme or ["light", "dark"]
    OUT.mkdir(parents=True, exist_ok=True)
    CHECK.mkdir(parents=True, exist_ok=True)
    server = None if a.release else dev_server()
    try:
        with tempfile.TemporaryDirectory() as tmp:
            if not a.release:
                build()
            if a.fresh or not (DEMO / "data" / "settings.json").exists():
                make_demo()
            # Each app running at once gets its own copy of the app data: the index is one app's.
            jobs = [(s, t) for s in scenes for t in themes]
            n = max(1, min(a.j, len(jobs)))
            global PARALLEL
            PARALLEL = n
            free = queue.Queue()
            for i in range(n):
                data = pathlib.Path(tmp) / f"data-{i}"
                shutil.copytree(DEMO / "data", data)
                free.put(data)

            def one(job):
                data = free.get()
                try:
                    # A scene on the tutorials' app data runs on a fresh copy of it.
                    if job[0].get("data"):
                        own = pathlib.Path(tempfile.mkdtemp(dir=tmp))
                        shutil.rmtree(own)
                        shutil.copytree(DEMO / f"data-{job[0]['data']}", own)
                        return shoot(*job, own)
                    return shoot(*job, data)
                finally:
                    free.put(data)

            with concurrent.futures.ThreadPoolExecutor(n) as pool:
                ok = all(list(pool.map(one, jobs)))
    finally:
        if server:
            server.terminate()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
