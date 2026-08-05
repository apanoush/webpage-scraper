#!/usr/bin/env python3
"""Read URLs from a text file (one per line) and scrape each with wbps."""

import subprocess
import sys
from pathlib import Path


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

    urls = [line.strip() for line in urls_file.read_text().splitlines()
            if line.strip() and not line.strip().startswith("#")]

    wbps = Path(__file__).parent / "webpage_scraper" / "target" / "debug" / "wbps"
    if not wbps.exists():
        wbps = Path(__file__).parent / "webpage_scraper" / "target" / "release" / "wbps"
    if not wbps.exists():
        print("wbps binary not found. Build it first: cargo build")
        sys.exit(1)

    total = len(urls)
    for i, url in enumerate(urls, 1):
        print(f"\n[{i}/{total}] {url}")
        result = subprocess.run(
            [str(wbps), url, *extra_args],
            cwd=urls_file.parent,
        )
        if result.returncode != 0:
            print(f"  FAILED (exit code {result.returncode})")

    print(f"\nDone. {total} URLs processed.")


if __name__ == "__main__":
    main()
