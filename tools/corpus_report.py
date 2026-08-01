#!/usr/bin/env python3
"""Summarise a corpus scan.

    python tools/corpus_report.py corpus.jsonl

Reports at two confidence levels, because they answer different questions.
`certain` is stubs only: tests that are empty. `likely` adds tests that run
real code without asserting on it, some of which are deliberate crash-or-hang
regression tests. Quoting one number without the other would be misleading.
"""

from __future__ import annotations

import argparse
import collections
import json
from pathlib import Path


def pct(part: int, whole: int) -> str:
    return f"{100.0 * part / whole:.2f}%" if whole else "n/a"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=Path, nargs="?", default=Path("corpus.jsonl"))
    parser.add_argument("--top", type=int, default=20, help="worst offenders to list")
    args = parser.parse_args()

    rows = [json.loads(line) for line in args.corpus.open(encoding="utf-8")]
    scanned = [r for r in rows if "findings" in r]
    with_tests = [r for r in scanned if r.get("tests", 0) > 0]

    total_tests = sum(r["tests"] for r in scanned)
    findings = [(r, f) for r in scanned for f in r["findings"]]
    # A disabled test strands every assertion after the `return`, so findings
    # overcount tests. The binary reports the distinct figure.
    total_flagged = sum(r.get("flagged", 0) for r in scanned)

    by_confidence = collections.Counter(f["confidence"] for _, f in findings)
    by_rule = collections.Counter(f["rule"] for _, f in findings)
    certain = [(r, f) for r, f in findings if f["confidence"] == "certain"]
    likely_plus = [(r, f) for r, f in findings if f["confidence"] in ("certain", "likely")]

    skips = collections.Counter(r["skipped"].split(":")[0] for r in rows if "skipped" in r)

    print("=" * 68)
    print("CORPUS")
    print("=" * 68)
    print(f"  packages in list       {len(rows):,}")
    print(f"  scanned                {len(scanned):,}")
    print(f"  shipping tests         {len(with_tests):,}")
    print(f"  tests examined         {total_tests:,}")
    for reason, count in skips.most_common():
        print(f"  skipped ({reason})".ljust(25) + f"{count:,}")

    print()
    print("=" * 68)
    print("FINDINGS")
    print("=" * 68)
    print(f"  findings               {len(findings):,}")
    print(f"  tests affected         {total_flagged:,}  = {pct(total_flagged, total_tests)} of all tests")
    print(f"  certain (stubs)        {len(certain):,}  = {pct(len(certain), total_tests)} of all tests")
    print(f"  certain + likely       {len(likely_plus):,}")
    print()
    for level in ("certain", "likely", "possible"):
        if by_confidence[level]:
            print(f"    {level:9} {by_confidence[level]:>7,}")
    print()
    for rule, count in by_rule.most_common():
        print(f"    {rule:28} {count:>7,}")

    affected = {r["package"] for r, _ in likely_plus}
    print()
    print(f"  packages with at least one   {len(affected):,} of {len(with_tests):,} "
          f"({pct(len(affected), len(with_tests))})")

    print()
    print("=" * 68)
    print(f"HIGHEST COUNTS (certain + likely)")
    print("=" * 68)
    counts = collections.Counter(r["package"] for r, _ in likely_plus)
    tests_by_package = {r["package"]: r["tests"] for r in scanned}
    print(f"  {'package':28} {'found':>6} {'tests':>8} {'rate':>7}")
    for package, count in counts.most_common(args.top):
        total = tests_by_package.get(package, 0)
        print(f"  {package:28} {count:>6,} {total:>8,} {pct(count, total):>7}")

    print()
    print("=" * 68)
    print("STUBS ONLY (certain) — the indefensible ones")
    print("=" * 68)
    stub_counts = collections.Counter(r["package"] for r, _ in certain)
    print(f"  {'package':28} {'stubs':>6} {'tests':>8}")
    for package, count in stub_counts.most_common(args.top):
        print(f"  {package:28} {count:>6,} {tests_by_package.get(package, 0):>8,}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
