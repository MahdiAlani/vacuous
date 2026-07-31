#!/usr/bin/env python3
"""Print the source of flagged tests so they can be checked by hand.

    python tools/inspect_findings.py numpy --limit 15
    python tools/inspect_findings.py requests --rule swallowed-failure

Numbers from a corpus scan are worthless unless the findings are real, and the
only way to know is to read them.
"""

from __future__ import annotations

import argparse
import io
import json
import random
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from corpus_scan import USER_AGENT, extract, fetch_json, scan  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package")
    parser.add_argument("--limit", type=int, default=10)
    parser.add_argument("--rule")
    parser.add_argument("--seed", type=int, default=0, help="0 means don't shuffle")
    parser.add_argument("--context", type=int, default=14, help="lines to show")
    parser.add_argument(
        "--binary",
        type=Path,
        default=Path(
            "target/release/vacuous.exe" if sys.platform == "win32" else "target/release/vacuous"
        ),
    )
    args = parser.parse_args()

    data = fetch_json(f"https://pypi.org/pypi/{args.package}/json")
    entry = next((u for u in data["urls"] if u["packagetype"] == "sdist"), None)
    if entry is None:
        print(f"{args.package} has no sdist", file=sys.stderr)
        return 1

    request = urllib.request.Request(entry["url"], headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=120) as response:
        archive = response.read()

    workdir = Path(tempfile.mkdtemp(prefix="vacuous-inspect-"))
    try:
        if not extract(archive, entry["url"], workdir):
            print("could not extract", file=sys.stderr)
            return 1

        report = scan(args.binary, workdir)
        if report is None:
            print("scan failed", file=sys.stderr)
            return 1

        findings = report["findings"]
        if args.rule:
            findings = [f for f in findings if f["rule"] == args.rule]
        if args.seed:
            random.Random(args.seed).shuffle(findings)

        print(f"# {args.package} {data['info']['version']}")
        print(f"# {report['summary']['tests_scanned']} tests, {len(report['findings'])} findings")
        print(f"# showing {min(args.limit, len(findings))}")

        for finding in findings[: args.limit]:
            path = workdir / finding["file"]
            print()
            print("=" * 78)
            print(f"{finding['file']}:{finding['line']}  [{finding['rule']}] {finding['confidence']}")
            print("=" * 78)
            try:
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError as error:
                print(f"  (unreadable: {error})")
                continue
            start = max(0, finding["line"] - 2)
            for number, text in enumerate(lines[start : start + args.context], start + 1):
                print(f"{number:5} | {text}")
    finally:
        shutil.rmtree(workdir, ignore_errors=True)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
