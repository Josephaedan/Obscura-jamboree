#!/usr/bin/env python3
"""Run a list of Carousell searches with `obscura carousell`.

Searches come from a JSON file (a list of objects) or, for a manual run, from
the SEARCH_JSON environment variable holding one search object. Each search
writes results/<n>-<slug>.json. A Markdown report is appended to
$GITHUB_STEP_SUMMARY when set, otherwise printed. Exits non-zero if any
search failed.
"""

import argparse
import json
import os
import re
import subprocess
import sys

REGIONS = {"sg", "my", "hk", "ph", "tw"}
SORTS = {"best", "recent", "price-low", "price-high"}


def load_searches(path):
    raw = os.environ.get("SEARCH_JSON", "").strip()
    if raw:
        data = [json.loads(raw)]
    else:
        with open(path) as f:
            data = json.load(f)
    if not isinstance(data, list):
        raise ValueError("search file must hold a JSON list")
    return [validate(s, i) for i, s in enumerate(data)]


def validate(s, i):
    if not isinstance(s, dict):
        raise ValueError(f"search {i}: must be an object")
    query = str(s.get("query") or "").strip()
    if not query:
        raise ValueError(f"search {i}: query is required")
    region = str(s.get("region") or "sg")
    if region not in REGIONS:
        raise ValueError(f"search {i}: region must be one of {sorted(REGIONS)}")
    sort = str(s.get("sort") or "best")
    if sort not in SORTS:
        raise ValueError(f"search {i}: sort must be one of {sorted(SORTS)}")
    out = {"query": query, "region": region, "sort": sort}
    for key in ("min_price", "max_price", "limit"):
        value = s.get(key)
        if value in (None, ""):
            continue
        if isinstance(value, bool) or not re.fullmatch(r"\d+", str(value)):
            raise ValueError(f"search {i}: {key} must be a whole number")
        out[key] = int(value)
    return out


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:40] or "search"


def command(binary, s, out_path, stealth, proxy):
    cmd = [binary]
    if stealth:
        cmd.append("--stealth")
    if proxy:
        cmd += ["--proxy", proxy]
    cmd += ["carousell", "--region", s["region"], "--sort", s["sort"],
            "--limit", str(s.get("limit", 50)), "--format", "json",
            "--quiet", "-o", out_path]
    if "min_price" in s:
        cmd += ["--min-price", str(s["min_price"])]
    if "max_price" in s:
        cmd += ["--max-price", str(s["max_price"])]
    cmd += ["--", s["query"]]
    return cmd


def cell(value):
    return str(value or "-").replace("|", "\\|").replace("\n", " ")


def report_section(s, listings, error):
    filters = [s["sort"]]
    if "min_price" in s or "max_price" in s:
        filters.append(f"price {s.get('min_price', 0)}-{s.get('max_price', 'any')}")
    lines = [f"## {cell(s['query'])} ({s['region']}, {', '.join(filters)})", ""]
    if error:
        lines.append(f"Search failed: {cell(error)}")
    elif not listings:
        lines.append("No listings found.")
    else:
        lines += [f"{len(listings)} listing(s)", "",
                  "| Price | Title | Condition | Seller | Posted |",
                  "|---|---|---|---|---|"]
        for l in listings:
            title = cell(l.get("title"))
            if l.get("url"):
                title = f"[{title}]({l['url']})"
            lines.append(f"| {cell(l.get('price'))} | {title} | {cell(l.get('condition'))} | "
                         f"{cell(l.get('seller'))} | {cell(l.get('posted'))} |")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--searches", default="carousell-searches.json")
    ap.add_argument("--binary", default="./target/release/obscura")
    ap.add_argument("--out-dir", default="results")
    ap.add_argument("--timeout", type=int, default=120,
                    help="seconds allowed per search")
    args = ap.parse_args()

    stealth = os.environ.get("STEALTH", "true").lower() != "false"
    proxy = os.environ.get("CAROUSELL_PROXY", "").strip()

    try:
        searches = load_searches(args.searches)
    except (OSError, ValueError) as e:
        print(f"Invalid searches: {e}", file=sys.stderr)
        return 2
    os.makedirs(args.out_dir, exist_ok=True)

    sections, failures = [], 0
    for n, s in enumerate(searches, 1):
        out_path = os.path.join(args.out_dir, f"{n:02d}-{slug(s['query'])}.json")
        print(f"[{n}/{len(searches)}] {s['query']} ({s['region']})", file=sys.stderr)
        error, listings = None, None
        try:
            proc = subprocess.run(command(args.binary, s, out_path, stealth, proxy),
                                  capture_output=True, text=True, timeout=args.timeout)
            if proc.returncode != 0:
                msg = [l for l in proc.stderr.splitlines() if l.startswith("Error:")]
                error = (msg[0][len("Error:"):].strip() if msg
                         else f"exit code {proc.returncode}")
            else:
                with open(out_path) as f:
                    listings = json.load(f)
        except subprocess.TimeoutExpired:
            error = f"timed out after {args.timeout}s"
        except (OSError, ValueError) as e:
            error = str(e)
        if error:
            failures += 1
            print(f"  failed: {error}", file=sys.stderr)
        else:
            print(f"  {len(listings)} listing(s)", file=sys.stderr)
        sections.append(report_section(s, listings, error))

    report = "# Carousell searches\n\n" + "\n".join(sections)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as f:
            f.write(report)
    else:
        print(report)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
