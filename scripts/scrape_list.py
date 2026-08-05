#!/usr/bin/env python3
"""Read URLs from a text file (one per line) and scrape each with wbps."""

import shutil
import subprocess
import sys
from pathlib import Path


def find_wbps():
    which = shutil.which("wbps")
    if which:
        return which, "PATH"
    here = Path(__file__).resolve().parent.parent
    for profile in ("release", "debug"):
        binary = here / "target" / profile / "wbps"
        if binary.exists():
            return str(binary), profile
    return None, None


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <urls_file> [wbps_args...]")
        print("Example: python scrape_list.py urls.txt --download-videos")
        sys.exit(1)

    urls_file = Path(sys.argv[1])
    extra_args = sys.argv[2:]

    if not urls_file.exists():
        print(f"File not found: {urls_file}")
        sys.exit(1)

    wbps, source = find_wbps()
    if not wbps:
        print("wbps not found in PATH or target/. Build it first: cargo build")
        sys.exit(1)

    label = f"{source} ({wbps})" if source == "PATH" else f"{source} build ({wbps})"
    print(f"using wbps from {label}")

    urls = [line.strip() for line in urls_file.read_text().splitlines()
            if line.strip() and not line.strip().startswith("#")]

    total = len(urls)
    for i, url in enumerate(urls, 1):
        print(f"\n[{i}/{total}] {url}")
        result = subprocess.run(
            [wbps, url, *extra_args],
            cwd=urls_file.parent,
        )
        if result.returncode != 0:
            print(f"  FAILED (exit code {result.returncode})")

    print(f"\nDone. {total} URLs processed.")


if __name__ == "__main__":
    main()
