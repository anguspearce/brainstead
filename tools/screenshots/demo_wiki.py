# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Demo-only wiki pages, so the Graph, Wiki health and the pies have something to show.

The fixture vault (tests/fixtures/vault) stays small because tests count what's in it; these pages
are written into the screenshot demo's copy only. Everything here is invented.
"""

import json
import pathlib

SOURCE = "[[Roadmap Update 2026-09-18]]"

# (folder, title, tags, summary, links)
PAGES = [
    ("entities", "Maya", ["people", "orbit"], "Product lead for the staff app.", ["Orbit App", "Lena", "Theo", "Steerco", "Soft Launch"]),
    (
        "entities",
        "Lena",
        ["people", "orbit"],
        "Runs the pilot and the support model.",
        ["Maya", "Pilot Feedback", "Support Model", "Orbit App"],
    ),
    ("entities", "Theo", ["people", "comms"], "Owns the comms plan for the launch.", ["Comms Plan", "Soft Launch", "Maya", "Staff Portal"]),
    (
        "entities",
        "Sam",
        ["people", "platform"],
        "Engineering manager for the Hub Platform.",
        ["Hub Platform", "Release Train", "Identity Service", "Priya"],
    ),
    (
        "entities",
        "Priya",
        ["people", "platform"],
        "Leads design and accessibility.",
        ["Design System", "Accessibility", "Onboarding Flow", "Sam"],
    ),
    (
        "entities",
        "Jonas",
        ["people", "risk"],
        "Security and data protection lead.",
        ["Data Retention", "Single Sign-On", "Identity Service", "Risk Register"],
    ),
    ("entities", "Victor", ["people"], "Finance partner for the programme.", ["Budget 2027", "Steerco", "Acme"]),
    ("entities", "Acme", ["vendor"], "Supplies the push-notification service.", ["Orbit App", "Payments Gateway", "Victor", "Northwind"]),
    ("entities", "Northwind", ["vendor"], "Hosting partner for the Hub Platform.", ["Hub Platform", "Acme", "Data Retention"]),
    (
        "entities",
        "Staff Portal",
        ["platform"],
        "The intranet the app replaces in stages.",
        ["Orbit App", "Single Sign-On", "Theo", "Hub Platform"],
    ),
    (
        "entities",
        "Payments Gateway",
        ["platform", "risk"],
        "Handles expense claims from the app.",
        ["Hub Platform", "Acme", "Identity Service"],
    ),
    ("entities", "Identity Service", ["platform"], "Sign-in for every staff tool.", ["Single Sign-On", "Hub Platform", "Jonas", "Sam"]),
    (
        "entities",
        "Steerco",
        ["governance"],
        "Monthly steering committee for the programme.",
        ["Maya", "Victor", "Risk Register", "Q4 Roadmap"],
    ),
    (
        "concepts",
        "Soft Launch",
        ["process", "orbit"],
        "Staff first, then everyone, from 14 November.",
        ["Orbit App", "Feature Flags", "Pilot Feedback", "Comms Plan"],
    ),
    (
        "concepts",
        "Feature Flags",
        ["process", "platform"],
        "How features reach groups of staff one at a time.",
        ["Release Train", "Soft Launch", "Hub Platform"],
    ),
    ("concepts", "Release Train", ["process"], "A release every second Thursday.", ["Feature Flags", "Sam", "Q4 Roadmap"]),
    (
        "concepts",
        "Design System",
        ["design"],
        "Shared components across the app and the portal.",
        ["Priya", "Accessibility", "Staff Portal", "Onboarding Flow"],
    ),
    (
        "concepts",
        "Accessibility",
        ["design", "risk"],
        "AA everywhere; checked before each release.",
        ["Design System", "Priya", "Release Train"],
    ),
    ("concepts", "Single Sign-On", ["platform", "risk"], "One sign-in across staff tools.", ["Identity Service", "Staff Portal", "Jonas"]),
    ("concepts", "Data Retention", ["risk"], "What the app keeps, and for how long.", ["Jonas", "Northwind", "Risk Register"]),
    (
        "concepts",
        "Onboarding Flow",
        ["design", "orbit"],
        "The first five minutes in the app.",
        ["Design System", "Priya", "Pilot Feedback", "Orbit App"],
    ),
    ("concepts", "Support Model", ["process"], "Who answers when staff get stuck.", ["Lena", "Comms Plan", "Pilot Feedback"]),
    ("concepts", "Comms Plan", ["comms"], "Emails, posters and a launch video.", ["Theo", "Soft Launch", "Support Model"]),
    (
        "summaries",
        "Pilot Feedback",
        ["orbit"],
        "What two hundred pilot users said in August.",
        ["Lena", "Onboarding Flow", "Support Model", "Orbit App"],
    ),
    (
        "summaries",
        "Q4 Roadmap",
        ["governance"],
        "What ships before the year ends.",
        ["Release Train", "Soft Launch", "Steerco", "Budget 2027"],
    ),
    (
        "summaries",
        "Risk Register",
        ["risk", "governance"],
        "The programme's open risks, reviewed at each steerco.",
        ["Jonas", "Data Retention", "Accessibility", "Payments Gateway"],
    ),
]


def page(folder: str, title: str, tags: list[str], summary: str, links: list[str]) -> str:
    kind = {"entities": "entity", "concepts": "concept", "summaries": "summary"}[folder]
    related = "\n".join(f"- [[{name}]]" for name in links)
    return (
        f'---\ntitle: {title}\ntype: {kind}\ntags: [{", ".join(tags)}]\nsources:\n  - "{SOURCE}"\nupdated: 2026-09-30\n---\n'
        f"# {title}\n\n{summary} It comes up alongside [[{links[0]}]] and [[{links[1]}]].\n\n## Related\n\n{related}\n"
    )


def write(vault: pathlib.Path) -> None:
    """Writes the pages into the demo vault, and links the fixture's own pages to them."""
    for folder, title, tags, summary, links in PAGES:
        d = vault / "wiki" / folder
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{title}.md").write_text(page(folder, title, tags, summary, links))
    more = {
        "wiki/entities/Orbit App.md": ["Maya", "Lena", "Soft Launch", "Acme", "Staff Portal", "Pilot Feedback", "Onboarding Flow"],
        "wiki/concepts/Hub Platform.md": ["Sam", "Northwind", "Identity Service", "Payments Gateway", "Feature Flags"],
    }
    for rel, links in more.items():
        p = vault / rel
        p.write_text(p.read_text().rstrip("\n") + "\n\n## See also\n\n" + "\n".join(f"- [[{name}]]" for name in links) + "\n")
    # Facts ingest kept for Orbit App, for its Facts card (wiki/.claims/, D-20261006-08).
    roadmap = ("sources/Roadmap Update 2026-09-18.md", "2026-09-18", "2026-09-18 — Roadmap update")
    steerco = ("sources/emails/Email. Steerco minutes - 2026-09-28.md", "2026-09-28", "2026-09-28 — Steerco: launch moves")

    def fact(attribute, value, quote, kept):
        source, as_of, entry = kept
        return {
            "subject": "Orbit App",
            "attribute": attribute,
            "value": value,
            "asOf": as_of,
            "quote": quote,
            "source": source,
            "entry": entry,
            "recorded": as_of,
        }

    claims = {
        "page": "wiki/entities/Orbit App.md",
        "claims": [
            fact("go_live_date", "14 November", "Staff soft launch remains on track for 14 November", roadmap),
            fact("go_live_date", "28 November", "Staff launch now targeted for 28 November", steerco),
            fact("status", "pending pen-test closure", "pending pen-test closure", steerco),
        ],
    }
    f = vault / "wiki" / ".claims" / "entities" / "Orbit App.json"
    f.parent.mkdir(parents=True, exist_ok=True)
    f.write_text(json.dumps(claims, indent=2, ensure_ascii=False) + "\n")
