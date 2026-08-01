#!/usr/bin/env python3
"""Run vacuous over the most-downloaded packages on PyPI.

    python tools/corpus_scan.py --top 1000 --out corpus.jsonl

Downloads each package's source distribution, because wheels usually ship
without tests, scans it, and appends one JSON object per package. Re-running
skips packages already in the output file, so it can be interrupted.

Results are recorded at `possible` confidence and tagged with their level, so
the analysis can slice by confidence without rescanning.
"""

from __future__ import annotations

import argparse
import io
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

TOP_PACKAGES = "https://hugovk.github.io/top-pypi-packages/top-pypi-packages.min.json"
USER_AGENT = "vacuous-corpus-scan (+https://github.com/MahdiAlani/vacuous)"

# Some sdists are enormous and mostly vendored C. Not worth the disk or time.
MAX_SDIST_BYTES = 60 * 1024 * 1024


def fetch_json(url: str) -> dict:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def top_packages(count: int) -> list[str]:
    data = fetch_json(TOP_PACKAGES)
    rows = data["rows"] if isinstance(data, dict) else data
    return [row["project"] for row in rows[:count]]


def sdist_url(package: str) -> tuple[str, str] | None:
    """Latest release's source distribution, as (url, version)."""
    try:
        data = fetch_json(f"https://pypi.org/pypi/{package}/json")
    except urllib.error.HTTPError:
        return None

    version = data["info"]["version"]
    for entry in data.get("urls", []):
        if entry.get("packagetype") == "sdist":
            if entry.get("size", 0) > MAX_SDIST_BYTES:
                return None
            return entry["url"], version
    return None


def is_within(directory: Path, target: Path) -> bool:
    try:
        target.resolve().relative_to(directory.resolve())
        return True
    except ValueError:
        return False


def extract(archive: bytes, url: str, into: Path) -> bool:
    """Unpack an sdist, refusing members that escape the target directory.

    Python 3.11.0 predates tarfile's `filter=` argument, so the check is done
    by hand rather than assuming it exists.
    """
    try:
        if url.endswith(".zip"):
            with zipfile.ZipFile(io.BytesIO(archive)) as zf:
                for member in zf.namelist():
                    if not is_within(into, into / member):
                        return False
                zf.extractall(into)
        else:
            with tarfile.open(fileobj=io.BytesIO(archive)) as tf:
                for member in tf.getmembers():
                    if member.issym() or member.islnk():
                        continue
                    if not is_within(into, into / member.name):
                        return False
                tf.extractall(into)
        return True
    except Exception:
        return False


def scan(binary: Path, target: Path) -> dict | None:
    result = subprocess.run(
        [
            str(binary),
            "check",
            str(target),
            "--format",
            "json",
            "--min-confidence",
            "possible",
            "--no-baseline",
        ],
        capture_output=True,
        text=True,
        timeout=300,
    )
    # 0 clean, 1 findings. 2 means vacuous itself failed.
    if result.returncode not in (0, 1):
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def already_done(path: Path) -> set[str]:
    if not path.exists():
        return set()
    done = set()
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            try:
                done.add(json.loads(line)["package"])
            except (json.JSONDecodeError, KeyError):
                continue
    return done


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--top", type=int, default=1000)
    parser.add_argument("--out", type=Path, default=Path("corpus.jsonl"))
    parser.add_argument(
        "--binary",
        type=Path,
        default=Path("target/release/vacuous.exe" if sys.platform == "win32" else "target/release/vacuous"),
    )
    args = parser.parse_args()

    if not args.binary.exists():
        print(f"no vacuous binary at {args.binary}; run `cargo build --release`", file=sys.stderr)
        return 2

    packages = top_packages(args.top)
    done = already_done(args.out)
    print(f"{len(packages)} packages, {len(done)} already scanned", file=sys.stderr)

    scanned = skipped = 0
    with args.out.open("a", encoding="utf-8") as out:
        for index, package in enumerate(packages, 1):
            if package in done:
                continue

            record: dict = {"package": package}
            try:
                found = sdist_url(package)
                if found is None:
                    record["skipped"] = "no sdist"
                else:
                    url, version = found
                    record["version"] = version
                    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
                    with urllib.request.urlopen(request, timeout=120) as response:
                        archive = response.read()

                    workdir = Path(tempfile.mkdtemp(prefix="vacuous-corpus-"))
                    try:
                        if not extract(archive, url, workdir):
                            record["skipped"] = "could not extract"
                        else:
                            report = scan(args.binary, workdir)
                            if report is None:
                                record["skipped"] = "scan failed"
                            else:
                                record["tests"] = report["summary"]["tests_scanned"]
                                record["files"] = report["summary"]["files_scanned"]
                                # Below findings when one test yields several.
                                record["flagged"] = report["summary"]["tests_flagged"]
                                record["findings"] = report["findings"]
                    finally:
                        shutil.rmtree(workdir, ignore_errors=True)
            except Exception as error:
                record["skipped"] = f"{type(error).__name__}: {error}"

            out.write(json.dumps(record) + "\n")
            out.flush()

            if "skipped" in record:
                skipped += 1
            else:
                scanned += 1

            if index % 25 == 0:
                print(
                    f"  {index}/{len(packages)}  scanned={scanned} skipped={skipped}",
                    file=sys.stderr,
                    flush=True,
                )

            # PyPI is a free service being hit by a script. Be polite.
            time.sleep(0.2)

    print(f"done: {scanned} scanned, {skipped} skipped", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
