# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""The website tutorials' demo (site/tutorials/): a new vault, made with the starter notes as
Create a new vault makes it, and the beginner tutorial's story written into it: you're organising
your team's move to a new office. Scenes with "data": "tutorial" open it; "data": "welcome" is the
first run, before a vault is chosen."""

import datetime
import gzip
import hashlib
import json
import pathlib
import re
import shutil

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
STARTER = ROOT / "src-tauri" / "starter-vault"


def fill(text, now):
    """The starter's placeholders, as src-tauri/core/src/starter.rs fills them."""

    def one(m):
        if m[1] == "now":
            return now.strftime("%Y-%m-%d %H:%M")
        n = int(m[3] or 0) * (-1 if m[2] == "-" else 1)
        return (now.date() + datetime.timedelta(days=n)).isoformat()

    return re.sub(r"\{\{(now|date)(?:([+-])(\d+))?\}\}", one, text)


FLOOR_PLAN = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 960 540" font-family="Helvetica Neue, Arial, sans-serif">
  <rect width="960" height="540" fill="#ffffff"/>
  <rect x="30" y="30" width="900" height="480" rx="6" fill="#f7f8fa" stroke="#1f2937" stroke-width="6"/>
  <g stroke="#1f2937" stroke-width="3" fill="none">
    <path d="M330 30v170M330 250v100M330 400v110M630 30v200M630 280v230M30 350h170M250 350h80M630 230h300"/>
  </g>
  <g fill="#e8f0fd" stroke="#1d5fd6" stroke-width="1.5">
    <rect x="370" y="70" width="70" height="40" rx="4"/><rect x="450" y="70" width="70" height="40" rx="4"/>
      <rect x="530" y="70" width="70" height="40" rx="4"/>
    <rect x="370" y="150" width="70" height="40" rx="4"/><rect x="450" y="150" width="70" height="40" rx="4"/>
      <rect x="530" y="150" width="70" height="40" rx="4"/>
    <rect x="370" y="320" width="70" height="40" rx="4"/><rect x="450" y="320" width="70" height="40" rx="4"/>
      <rect x="530" y="320" width="70" height="40" rx="4"/>
    <rect x="370" y="400" width="70" height="40" rx="4"/><rect x="450" y="400" width="70" height="40" rx="4"/>
      <rect x="530" y="400" width="70" height="40" rx="4"/>
  </g>
  <g fill="#e6f4f1" stroke="#0d7f74" stroke-width="1.5">
    <rect x="90" y="110" width="180" height="90" rx="10"/><rect x="700" y="80" width="170" height="80" rx="10"/>
  </g>
  <g fill="#fdf2e3" stroke="#9a4b08" stroke-width="1.5"><circle cx="120" cy="440" r="26"/><circle cx="210" cy="440" r="26"/></g>
  <g font-size="22" font-weight="600" fill="#151a22" text-anchor="middle">
    <text x="180" y="80">Meeting room 1</text><text x="485" y="250">Team zone A</text>
      <text x="485" y="480" font-size="18">Team zone B</text>
    <text x="785" y="200">Meeting room 2</text><text x="785" y="330">Quiet room</text><text x="165" y="390">Kitchen</text>
  </g>
  <g font-size="15" fill="#4d5664" text-anchor="middle">
    <text x="180" y="230">Seats 8, screen</text><text x="785" y="225">Seats 6</text><text x="785" y="355">Phone calls, focus work</text>
  </g>
  <g fill="#1d5fd6"><path d="M480 510l-14-24h28z"/></g>
  <text x="512" y="503" font-size="15" fill="#4d5664">Entrance</text>
  <text x="900" y="500" font-size="14" fill="#6e7785" text-anchor="end">12 Mill Lane, second floor</text>
</svg>
"""

TIMELINE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 960 300" font-family="Helvetica Neue, Arial, sans-serif">
  <rect width="960" height="300" fill="#ffffff"/>
  <g font-size="14" fill="#6e7785" text-anchor="middle">
    <text x="320" y="30">27 Oct</text><text x="440" y="30">3 Nov</text><text x="560" y="30">10 Nov</text>
      <text x="680" y="30">17 Nov</text><text x="800" y="30">24 Nov</text>
  </g>
  <g stroke="#e3e7ed" stroke-width="1"><path d="M260 42v240M380 42v240M500 42v240M620 42v240M740 42v240M860 42v240"/></g>
  <g font-size="16" fill="#151a22">
    <text x="20" y="77">Approve the move</text><text x="20" y="127">Sign lease, book movers</text>
      <text x="20" y="177">Network and furniture</text>
    <text x="20" y="227">Pack and move</text><text x="20" y="277">First day at Mill Lane</text>
  </g>
  <g>
    <rect x="260" y="60" width="90" height="26" rx="13" fill="#1d5fd6"/>
    <rect x="340" y="110" width="160" height="26" rx="13" fill="#1d5fd6" opacity="0.85"/>
    <rect x="470" y="160" width="240" height="26" rx="13" fill="#0d7f74"/>
    <rect x="700" y="210" width="90" height="26" rx="13" fill="#9a4b08"/>
    <circle cx="815" cy="273" r="11" fill="#151a22"/>
  </g>
</svg>
"""


def write(demo, home):
    """Writes the beginner tutorial's vault at `home`/Documents/My notes and its app data
    (`demo`/data-tutorial), the intermediate one's (`demo`/data-intermediate) and the first run's
    app data (`demo`/data-welcome)."""
    now = datetime.datetime.now().replace(second=0, microsecond=0)
    beginner(home / "Documents" / "My notes", demo / "data-tutorial", now)
    intermediate(home / "Work" / "My notes", demo / "data-intermediate", now)
    advanced(home / "Wiki" / "My notes", demo / "data-advanced", now)
    power(home / "Power" / "My notes", demo / "data-power", now)
    welcome = demo / "data-welcome"
    welcome.mkdir(parents=True, exist_ok=True)
    (welcome / "settings.json").write_text(json.dumps({"theme": "system", "skippedFullDiskAccess": True}, indent=2))


def beginner(vault, data, now):
    """A new vault, as the beginner tutorial leaves it: the office move's project, kick-off and
    proposal, and a task and a thought just captured."""
    day = lambda n=0: (now.date() + datetime.timedelta(days=n)).isoformat()  # noqa: E731
    for f in sorted(STARTER.rglob("*")):
        if f.is_file():
            out = vault / fill(str(f.relative_to(STARTER)), now)
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(fill(f.read_text(), now))

    kickoff = f"Meeting. Office move kick-off - {day(-2)}"
    proposal = f"Report. Office move proposal - {day()}"
    (vault / "Project. Office move.md").write_text(
        "---\nstatus: active\narea: Work\noutcome: The team is working from 12 Mill Lane by 1 December\n---\n\n"
        "Our lease at Harbour Street ends on 30 November, so the team of 24 is moving to a new office. "
        f"We agreed the plan at the [[{kickoff}|kick-off]], and Sam wants a [[{proposal}|proposal]] by Friday.\n\n"
        "## Next actions\n\n"
        f"- [ ] Draft the office move proposal for Sam #context/computer 📅 {day(2)}\n"
        f"- [ ] Plan who sits where ⏳ {day(7)}\n"
        "- [ ] Tell the team the moving date #context/computer\n\n"
        "## Waiting for\n\n"
        f"- [ ] Theo to check the internet connection at Mill Lane #waiting-for ➕ {day(-2)}\n\n"
        "## Done\n\n"
        f"- [x] Walk round 12 Mill Lane with Lena ✅ {day(-3)}\n"
    )
    (vault / f"{kickoff}.md").write_text(
        "---\ntags: [meeting]\nattendees: [Sam, Lena, Theo]\n---\n\n"
        "## Attendees\n\n- Sam (Head of Operations)\n- Lena (Facilities)\n- Theo (IT)\n\n"
        "## Notes\n\n"
        "- The lease on Harbour Street ends on **30 November**.\n"
        "- Two options: 12 Mill Lane (30 desks, two meeting rooms) or the Riverside co-working floor.\n"
        f"- Everyone preferred Mill Lane. Sam wants a one-page [[{proposal}|proposal]] by Friday for the leadership team.\n"
        "- This is all part of the [[Project. Office move|Office move]].\n\n"
        "## Actions\n\n"
        f"- [ ] Get three quotes from removal firms 📅 {day(3)}\n"
        "- [ ] Theo to check the internet connection at Mill Lane #waiting-for\n"
        f"- [x] Walk round 12 Mill Lane with Lena ✅ {day(-3)}\n"
    )
    (vault / "images").mkdir(exist_ok=True)
    (vault / "images" / "office-floor-plan.svg").write_text(FLOOR_PLAN)
    (vault / "images" / "move-timeline.svg").write_text(TIMELINE)
    (vault / f"{proposal}.md").write_text(
        "---\ntags: [office-move]\nfor: Sam Carter, Head of Operations\n---\n\n"
        "## Summary\n\n"
        "Our lease at Harbour Street ends on 30 November. We recommend moving the team to **12 Mill Lane** over the weekend "
        "of 22 November. It costs less each month than we pay now, has room for the team to grow to 30, and is a "
        "ten-minute walk from the station.\n\n"
        "## The options\n\n"
        "| | 12 Mill Lane | Riverside co-working |\n|---|---|---|\n"
        "| Desks | 30, our own | 24, shared |\n| Monthly cost | $18,400 | $21,000 |\n"
        "| Meeting rooms | 2, and a quiet room | Booked by the hour |\n| Ready from | 15 November | Now |\n\n"
        "## The new floor\n\n"
        "![[office-floor-plan.svg|640]]\n\n"
        "*The second floor at 12 Mill Lane: two team zones, two meeting rooms and a quiet room.*\n\n"
        "## Timeline\n\n"
        "![[move-timeline.svg|720]]\n\n"
        "## One-off costs\n\n"
        "| Item | Cost |\n|---|---:|\n| Removal firm | $3,200 |\n| Network and Wi-Fi | $1,800 |\n"
        "| Furniture | $4,500 |\n| Contingency | $1,000 |\n| **Total** | **$10,500** |\n\n"
        "> [!warning] The internet connection\n"
        "> Theo is checking whether fibre is in place at Mill Lane. If it isn't, it takes three weeks to install, "
        "so we need an answer by 1 November.\n\n"
        "## Next steps\n\n"
        "1. Sam approves the move by 31 October.\n"
        "2. We sign the lease and book the removal firm.\n"
        "3. Lena shares the seating plan by 14 November.\n"
    )
    # What you captured with ⌃⌥Space: a task and a thought, waiting in the Inbox.
    todo = vault / "Me. To Do List.md"
    todo.write_text(todo.read_text().replace("#### Other\n\n", "#### Other\n\n- [ ] Ask Lena for the Mill Lane floor plan\n", 1))
    pad = vault / "Me. Scratchpad.md"
    pad.write_text(pad.read_text() + f"\n## {now:%Y-%m-%d %H:%M}\n\nCould we ask the landlord for the first month rent-free?\n")

    data.mkdir(parents=True, exist_ok=True)
    settings = {
        "vaultPath": str(vault),
        "readOnly": False,
        "theme": "system",
        "docLooks": {f"{proposal}.md": {"style": "business", "accent": "blue"}},
    }
    (data / "settings.json").write_text(json.dumps(settings, indent=2))
    return proposal


def intermediate(vault, data, now):
    """The same vault a week on, as the intermediate tutorial finds it: the move approved and
    under way, two more projects (one stuck), a 1-1 with Lena each week, a dashboard note, a
    full Inbox and a weekly review paused at Projects. The starter's examples are gone."""
    day = lambda n=0: (now.date() + datetime.timedelta(days=n)).isoformat()  # noqa: E731
    proposal = beginner(vault, data, now)
    for rel in (
        "Start here.md",
        "Guide. What a note can do.md",
        "Project. Orbit App launch.md",
        f"Meeting. Orbit App kick-off - {day(-1)}.md",
    ):
        (vault / rel).unlink()
    for d in ("sources", "wiki"):
        shutil.rmtree(vault / d)
    (vault / "Me. Bookmarks.md").write_text("* [[Me. To Do List]]\n* [[Project. Office move]]\n* [[Dashboard. Office move]]\n")
    todo = vault / "Me. To Do List.md"
    text = re.sub(
        r"#### Other\n\n.*?\n\n",
        "#### Other\n\n"
        "- [ ] Order monitors for the quiet room\n"
        "- [ ] Call the landlord about parking permits\n"
        "- [ ] Find a caterer for the first-day lunch\n\n",
        todo.read_text(),
        count=1,
        flags=re.S,
    )
    todo.write_text(text)
    (vault / "Me. Scratchpad.md").write_text(
        re.sub(r"\n## \d.*", "", (vault / "Me. Scratchpad.md").read_text(), flags=re.S)
        + f"\n## {now:%Y-%m-%d %H:%M}\n\nA house-warming lunch at Mill Lane on the first Friday?\n"
    )
    kickoff = f"Meeting. Office move kick-off - {day(-2)}"
    meeting = vault / f"{kickoff}.md"
    meeting.write_text(
        meeting.read_text()
        .replace("- [ ] Get three quotes from removal firms 📅 " + day(3), f"- [x] Get three quotes from removal firms ✅ {day(-4)}")
        .replace("- [ ] Theo to check", "- [x] Theo to check")
        .replace("Mill Lane #waiting-for\n", f"Mill Lane #waiting-for ✅ {day(-1)}\n")
    )
    (vault / "Project. Office move.md").write_text(
        "---\nstatus: active\narea: Work\noutcome: The team is working from 12 Mill Lane by 1 December\n---\n\n"
        f"Sam approved the [[{proposal}|proposal]]: we move to 12 Mill Lane over the weekend of 22 November. "
        f"The plan started at the [[{kickoff}|kick-off]].\n\n"
        "## Next actions\n\n"
        f"- [ ] Book the removal firm for 22 November #context/calls [effort:: 15m] 📅 {day(1)}\n"
        f"- [ ] Send the team the moving-day checklist #context/computer [effort:: 30m] 📅 {day(3)}\n"
        "- [ ] Order name plates for the meeting rooms #context/computer [effort:: 15m]\n"
        f"- [ ] Label the desks at Mill Lane #context/office [effort:: 1h] ⏳ {day(9)}\n"
        "- [ ] Collect spare keys from the landlord #context/errands [effort:: 30m]\n\n"
        "## Waiting for\n\n"
        f"- [ ] Theo to confirm the date the fibre goes in #waiting-for ➕ {day(-6)}\n"
        f"- [ ] Lena to send the seating plan #waiting-for ➕ {day(-2)}\n\n"
        "## Done\n\n"
        f"- [x] Draft the office move proposal for Sam ✅ {day(-5)}\n"
        f"- [x] Get three quotes from removal firms ✅ {day(-4)}\n"
        f"- [x] Walk round 12 Mill Lane with Lena ✅ {day(-8)}\n"
    )
    (vault / "Project. Team away day.md").write_text(
        "---\nstatus: active\narea: Work\noutcome: A day out for the whole team in January, booked and paid for\n---\n\n"
        "## Next actions\n\n## Waiting for\n\n## Notes\n\n- Ideas so far: a cooking class, a walk and lunch in the hills, an escape room.\n"
    )
    (vault / "Project. Summer holiday.md").write_text(
        "---\nstatus: active\narea: Personal\noutcome: Flights and somewhere to stay booked by the end of November\n---\n\n"
        "## Next actions\n\n"
        "- [ ] Compare flights to Lisbon #context/computer [effort:: 30m]\n"
        "- [ ] Ask Jo which week suits #context/calls [effort:: 15m]\n\n"
        "## Someday / maybe\n\n- [ ] A day trip to Sintra #someday-maybe\n"
    )
    (vault / f"1-1. Lena - {day(-7)}.md").write_text(
        "## Their update\n\n- The landlord will hand over the keys on 15 November.\n"
        "- Lena is drawing up the seating plan.\n\n## My update\n\n- Sam approved the move.\n\n"
        "## Follow-ups\n\n"
        "- [ ] Ask Lena how the quiet room should be booked #followup/Lena\n"
        f"- [x] Send Lena the removal firm's quote #followup/Lena ✅ {day(-6)}\n"
    )
    (vault / f"1-1. Lena - {day()}.md").write_text(
        "## Their update\n\n- The seating plan is nearly done: team zone A by the windows.\n"
        "- Movers want the lift booked from 7am.\n\n## My update\n\n- Removal firm chosen; booking it tomorrow.\n\n"
        "## Follow-ups\n\n"
        "- [ ] Book the lift at Mill Lane for moving day #followup/Lena\n"
    )
    (vault / "Dashboard. Office move.md").write_text(
        "Everything about the [[Project. Office move|office move]] on one page. The lists update themselves.\n\n"
        "## Due in the next two weeks\n\n"
        f"```tasks\nnot done\ndue before {day(14)}\npath includes Office move\nsort by due\n```\n\n"
        "## Waiting on others\n\n"
        "```tasks\nnot done\ntags include #waiting-for\npath includes Office move\n```\n\n"
        "## Follow-ups with Lena\n\n"
        "```tasks\nnot done\ntags include #followup/Lena\n```\n"
    )
    # A weekly review paused at Projects (src/Weekly.tsx, STEPS), for its week.
    d = now.date() - datetime.timedelta(days=7 if now.weekday() in (0, 1) else 0)
    y, w, _ = d.isocalendar()
    state = {
        "week": f"{y}-W{w:02d}",
        "step": 6,
        "done": [0, 1, 2, 3, 4, 5],
        "log": [],
        "notes": "",
        "startedAt": int(now.timestamp() * 1000),
    }
    (data / "weekly.json").write_text(json.dumps(state, indent=2))


def copy(demo, name, to):
    """A fresh copy of the named app data, for one app to run on."""
    shutil.copytree(demo / f"data-{name}", to)


def advanced(vault, data, now):
    """The same vault again, with an AI assistant at work, as the advanced tutorial finds it: the
    lease and the movers' quote ingested into three wiki pages (listed in Changes), and Theo's
    email and a Teams check-in captured but not yet dealt with."""
    day = lambda n=0: (now.date() + datetime.timedelta(days=n)).isoformat()  # noqa: E731
    intermediate(vault, data, now)
    proposal = f"Report. Office move proposal - {day()}"
    lease, quote = "Lease. 12 Mill Lane heads of terms", "Quote. Swift Removals"
    src = vault / "sources"
    src.mkdir()
    lease_text = (
        f"---\nsource_type: note\ningested: {day(-1)}\n---\n\n# 12 Mill Lane: heads of terms\n\n"
        "From the landlord's agent, Harbour & Vale Property.\n\n"
        "**Premises:** the second floor of 12 Mill Lane, about 420 square metres.\n\n"
        "**Term:** five years from 15 November, with a break at year three.\n\n"
        "**Rent:** $18,400 a month, the first month rent-free.\n\n"
        "**Parking:** four permits included; more at $90 a month each.\n\n"
        "**Access:** the building is open 7am to 8pm on weekdays. Moves at weekends need the goods lift booked a week ahead.\n"
    )
    (src / f"{lease}.md").write_text(lease_text)
    quote_text = (
        f"---\nsource_type: note\ningested: {day(-1)}\n---\n\n# Swift Removals: quote\n\n"
        "Swift Removals will move 24 desks, chairs and the IT equipment over the weekend of 22 November for $3,200. "
        "Two crews arrive at 7am with two vans. Crates are delivered a week before, and collected the week after.\n"
    )
    (src / f"{quote}.md").write_text(quote_text)
    (src / f"Email. Thread. Re Fibre at Mill Lane - {day(-1)}.md").write_text(
        "# Re Fibre at Mill Lane\n\n**Source:** [Open in Outlook](https://outlook.office.com/mail/)\n"
        f"**Captured:** {day(-1)}T15:20:00.000Z\n\n---\n\n## Theo — {day(-1)} 16:05\n\n**To:** Me\n\n"
        "Good news: the fibre at Mill Lane goes in on 10 November.\n\n"
        "Can someone let the engineer in at 8am, and will the comms room key be ready by then? "
        "And do we still need the second line for the guest Wi-Fi?\n\nTheo\n\n---\n"
    )
    (src / f"Teams. Transcript. Office move check-in - {day()}.md").write_text(
        f"# Office move check-in\n\n**Source:** [Open meeting recap](https://teams.microsoft.com/)\n**Meeting:** {day()}\n"
        f"**Captured:** {day()}T10:05:00.000Z\n\n---\n\n"
        "## Sam — 0 minutes 3 seconds\n\n0:03\n\nLet's go round. Where are we on the move?\n\n"
        "## Lena — 0 minutes 12 seconds\n\n0:12\n\nThe seating plan is done. Team zone A gets the windows.\n\n"
        "## Theo — 0 minutes 31 seconds\n\n0:31\n\nFibre goes in on the 10th. I need someone there at eight.\n\n"
        "## Sam — 0 minutes 45 seconds\n\n0:45\n\nGood. Let's send the moving-day checklist to the team by Friday.\n"
    )
    pages = {
        "wiki/entities/12 Mill Lane.md": (
            "---\ntype: entity\nname: 12 Mill Lane\ndescription: The team's new office from 22 November\n"
            f'sources: ["[[{lease}]]", "[[{quote}]]"]\n---\n\n# 12 Mill Lane\n\n'
            f"The second floor of 12 Mill Lane is the team's new office, chosen in the [[{proposal}|office move proposal]].\n\n"
            "## Current state\n\n"
            f'- The lease runs "five years from 15 November, with a break at year three." [[{lease}]]\n'
            f'- Rent is "$18,400 a month, the first month rent-free." [[{lease}]]\n'
            f'- Parking: "four permits included; more at $90 a month each." [[{lease}]]\n'
            f'- Access: "Moves at weekends need the goods lift booked a week ahead." [[{lease}]]\n\n'
            "## Moving in\n\n"
            f'[[Swift Removals]] do the move on [[Moving day]]: "Two crews arrive at 7am with two vans." [[{quote}]]\n'
        ),
        "wiki/entities/Swift Removals.md": (
            "---\ntype: entity\nname: Swift Removals\ndescription: The removal firm moving the team to 12 Mill Lane\n"
            f'sources: ["[[{quote}]]"]\n---\n\n# Swift Removals\n\n'
            "The removal firm chosen for the move to [[12 Mill Lane]].\n\n## Current state\n\n"
            f'- They move "24 desks, chairs and the IT equipment over the weekend of 22 November for $3,200." [[{quote}]]\n'
            f'- "Crates are delivered a week before, and collected the week after." [[{quote}]]\n'
        ),
        "wiki/concepts/Moving day.md": (
            "---\ntype: concept\nname: Moving day\ndescription: The weekend of 22 November, when the team moves to 12 Mill Lane\n"
            f'sources: ["[[{lease}]]", "[[{quote}]]"]\n---\n\n# Moving day\n\n'
            "The weekend the team moves from Harbour Street to [[12 Mill Lane]].\n\n## Current state\n\n"
            f'- [[Swift Removals]]: "Two crews arrive at 7am with two vans." [[{quote}]]\n'
            f'- The goods lift: "Moves at weekends need the goods lift booked a week ahead." [[{lease}]]\n\n'
            "## Open questions\n\n- Where the vans park, with four permits.\n"
        ),
    }
    for rel, text in pages.items():
        (vault / rel).parent.mkdir(parents=True, exist_ok=True)
        (vault / rel).write_text(text)
    # The ingest's changes in Changes (src-tauri/core/src/changes.rs), as tools/screenshots.py's
    # demo_changes writes them.
    origin = {"kind": "ingest", "label": f"Ingest of {lease}", "run": f"ingest-{day(-1)}-lease"}
    for i, (rel, text) in enumerate(pages.items()):
        h = keep_text(data, text)
        quotes = [
            {"source": s, "text": q, "checked": True, "path": f"sources/{s}.md"}
            for q, s in re.findall(r'"([^"]+)" \[\[([^\]]+)\]\]', text)[:3]
        ]
        name = rel.rsplit("/", 1)[-1][:-3]
        c = {
            "id": f"tut-{i}",
            "created": f"{day(-1)}T14:3{i}:00",
            "model": "claude:sonnet",
            "page": rel,
            "kind": "new",
            "title": f"New page: {name}",
            "reason": f"The {'lease' if i != 1 else 'quote'} names {name}, and the wiki had no page for it.",
            "quotes": quotes,
            "warnings": [],
            "origin": origin,
            "instruction": {"op": "page", "content": text},
            "status": "applied",
            "decided": f"{day(-1)}T14:3{i}:00",
            "flags": [],
            "after": h,
        }
        (data / "changes" / f"tut-{i}.json").write_text(json.dumps(c, indent=2))


def keep_text(data, text):
    """Keeps a page's text as Changes keeps it, compressed by its hash, and returns the hash."""
    out = data / "changes" / "text"
    out.mkdir(parents=True, exist_ok=True)
    h = hashlib.sha256(text.encode()).hexdigest()
    (out / f"{h}.gz").write_bytes(gzip.compress(text.encode()))
    return h


BURNDOWN = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 960 360" font-family="Helvetica Neue, Arial, sans-serif">
  <rect width="960" height="360" fill="#ffffff"/>
  <g stroke="#e3e7ed"><path d="M80 60h840M80 130h840M80 200h840M80 270h840"/></g>
  <g font-size="14" fill="#6e7785" text-anchor="end"><text x="68" y="65">24</text><text x="68" y="135">16</text>
    <text x="68" y="205">8</text><text x="68" y="275">0</text></g>
  <g>
    <rect x="120" y="60" width="70" height="210" rx="6" fill="#143a5e"/><rect x="290" y="95" width="70" height="175" rx="6" fill="#143a5e"/>
    <rect x="460" y="139" width="70" height="131" rx="6" fill="#143a5e"/>
      <rect x="630" y="174" width="70" height="96" rx="6" fill="#143a5e"/>
    <rect x="800" y="218" width="70" height="52" rx="6" fill="#0d7f74" opacity="0.55"/>
  </g>
  <path d="M155 60 L835 270" stroke="#9a4b08" stroke-width="2.5" stroke-dasharray="8 7" fill="none"/>
  <g font-size="15" fill="#151a22" text-anchor="middle"><text x="155" y="300">Week 1</text><text x="325" y="300">Week 2</text>
    <text x="495" y="300">Week 3</text><text x="665" y="300">This week</text><text x="835" y="300">Moving day</text></g>
  <g font-size="14" fill="#4d5664"><rect x="80" y="326" width="14" height="14" rx="3" fill="#143a5e"/>
    <text x="102" y="338">Open tickets</text>
    <path d="M230 333h28" stroke="#9a4b08" stroke-width="2.5" stroke-dasharray="6 5"/><text x="266" y="338">On track to finish</text>
    <rect x="420" y="326" width="14" height="14" rx="3" fill="#0d7f74" opacity="0.55"/><text x="442" y="338">Forecast</text></g>
</svg>
"""


def power(vault, data, now):
    """The advanced vault after a few terminal sessions with Brainstead's tools and Atlassian's:
    Jira tickets as tasks on the project, a progress report for Sam with a chart, the session's
    changes in Changes, and one change from the overnight run held for you."""
    day = lambda n=0: (now.date() + datetime.timedelta(days=n)).isoformat()  # noqa: E731
    advanced(vault, data, now)
    project = vault / "Project. Office move.md"
    project.write_text(
        project.read_text().replace(
            "## Waiting for\n\n",
            f"- [ ] FAC-142 Order the network switches #context/computer 📅 {day(2)}\n"
            f"- [ ] FAC-147 Approve the furniture delivery slot #context/calls 📅 {day(4)}\n\n"
            "## Waiting for\n\n"
            f"- [ ] FAC-150 Lena: deep clean at Mill Lane #waiting-for ➕ {day()}\n",
            1,
        )
    )
    (vault / "images" / "move-burndown.svg").write_text(BURNDOWN)
    report = f"Report. Office move progress - {day()}"
    text = (
        "---\ntags: [office-move]\nfor: Sam Carter, Head of Operations\n---\n\n"
        "## Summary\n\n"
        "The move to 12 Mill Lane is **on track** for the weekend of 22 November. 15 of 24 Jira tickets are done, "
        "the fibre goes in on 10 November, and the one risk is parking for the movers' vans.\n\n"
        "| Area | Status | Owner | Next step |\n|---|---|---|---|\n"
        "| Lease and keys | Done | Lena | Keys on 15 November |\n"
        "| Network and Wi-Fi | On track | Theo | Fibre installed 10 November |\n"
        "| Furniture | On track | Lena | Delivery slot to approve (FAC-147) |\n"
        "| Removals | On track | Me | Swift Removals booked for 22 November |\n"
        "| Parking | At risk | Me | Two more permits from the landlord |\n\n"
        "## Tickets\n\n![[move-burndown.svg|640]]\n\n"
        "*Open tickets in the FAC project labelled office-move, week by week.*\n\n"
        "## Fit-out\n\nFrom the Confluence page *Mill Lane fit-out plan*: the quiet room and both meeting rooms are finished; "
        "team zone B's desks arrive in the week of 10 November.\n\n"
        "> [!warning] Parking for moving day\n"
        "> The lease includes four permits. Swift Removals bring two vans, so we need two more for the weekend.\n\n"
        "## Decisions needed\n\n1. Approve the furniture delivery slot (FAC-147) by Friday.\n"
        "2. Agree $180 for two extra parking permits for the weekend of 22 November.\n"
    )
    (vault / f"{report}.md").write_text(text)
    settings = json.loads((data / "settings.json").read_text())
    settings["docLooks"][f"{report}.md"] = {"style": "report", "accent": "blue"}
    (data / "settings.json").write_text(json.dumps(settings, indent=2))
    # The terminal session's changes, and the overnight run's held one.
    session = {"kind": "chat", "label": "Claude Code in Terminal: progress report for Sam", "run": f"term-{day()}-report"}
    made = [
        ("pw-0", "image", "images/move-burndown.svg", "New image: move-burndown.svg", "The chart of open tickets for the report.", None),
        ("pw-1", "new", f"{report}.md", f"New note: {report[8:]}", "Sam asked for a progress report he can share.", text),
        (
            "pw-2",
            "task",
            "Project. Office move.md",
            "Tasks: FAC-142, FAC-147 and FAC-150",
            "Your Jira tickets as next actions; Lena's as waiting for.",
            None,
        ),
    ]
    for i, (cid, kind, page, title, reason, after) in enumerate(made):
        c = {
            "id": cid,
            "created": f"{day()}T08:4{i}:00",
            "model": "claude:opus",
            "page": page,
            "kind": kind,
            "title": title,
            "reason": reason,
            "quotes": [],
            "warnings": [],
            "origin": session,
            "status": "applied",
            "decided": f"{day()}T08:4{i}:00",
            "flags": [],
            "instruction": {"op": "page", "content": after or ""},
        }
        if after:
            c["after"] = keep_text(data, after)
        (data / "changes" / f"{cid}.json").write_text(json.dumps(c, indent=2))
    held = {
        "id": "pw-held",
        "created": f"{day()}T06:02:00",
        "model": "claude:sonnet",
        "page": "wiki/entities/12 Mill Lane.md",
        "kind": "edit",
        "title": "Update 12 Mill Lane: parking",
        "reason": "The landlord's email says weekend permits are free.",
        "quotes": [{"source": "Email. Thread. Re Parking at Mill Lane", "text": "weekend permits are free of charge", "checked": False}],
        "warnings": [],
        "origin": {"kind": "chat", "label": "Overnight: refresh the wiki", "run": f"overnight-{day()}", "trigger": "scheduled"},
        "status": "held",
        "decided": None,
        "flags": ["This quote isn't in its source: “weekend permits are free of charge”"],
        "instruction": {
            "op": "replace",
            "edits": [
                {
                    "find": '- Parking: "four permits included; more at $90 a month each." [[Lease. 12 Mill Lane heads of terms]]',
                    "replace": '- Parking: "four permits included"; weekend permits are free of charge. '
                    "[[Email. Thread. Re Parking at Mill Lane]]",
                }
            ],
        },
    }
    (data / "changes" / "pw-held.json").write_text(json.dumps(held, indent=2))
