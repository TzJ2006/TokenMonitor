#!/usr/bin/env python3
"""
Compare TokenMonitor data vs AWS Cost Explorer: Apr 13-19, 2026.
All dates aligned to UTC. Includes extra host data from screenshot.
"""

import json
import os
import glob
from datetime import datetime, timezone, timedelta, date as Date
from collections import defaultdict

CLAUDE_LOG_ROOT = os.path.expanduser("~/.claude/projects")
APP_DATA = os.path.expanduser("~/Library/Application Support/com.tokenmonitor.app")
ARCHIVE_DIR = os.path.join(APP_DATA, "usage-archive")
REMOTE_CACHE_DIR = os.path.join(APP_DATA, "remote-cache")

LOCAL_TZ_OFFSET_HOURS = -4  # EDT

TARGET_DATES_UTC = [
    "2026-04-13", "2026-04-14", "2026-04-15", "2026-04-16",
    "2026-04-17", "2026-04-18", "2026-04-19",
]
TARGET_SET = set(TARGET_DATES_UTC)

ARCHIVE_DATES_NEEDED = set()
for d in TARGET_DATES_UTC:
    dt = Date.fromisoformat(d)
    ARCHIVE_DATES_NEEDED.add((dt - timedelta(days=1)).isoformat())
    ARCHIVE_DATES_NEEDED.add(d)

EXCLUDE_DEVICES = {"tianhe"}

AWS_DATA_DAILY = {
    "2026-04-13": {"cache_read": 88.580, "cache_write": 7.358, "input": 1.745, "output": 0.663},
    "2026-04-14": {"cache_read": 179.832, "cache_write": 39.027, "input": 5.334, "output": 4.062},
    "2026-04-15": {"cache_read": 63.065, "cache_write": 4.319, "input": 0.084, "output": 0.387},
    "2026-04-16": {"cache_read": 20.257, "cache_write": 1.876, "input": 0.075, "output": 0.134},
    "2026-04-17": {"cache_read": 78.972, "cache_write": 4.074, "input": 0.565, "output": 0.622},
    "2026-04-18": {"cache_read": 80.752, "cache_write": 4.728, "input": 0.073, "output": 0.380},
    "2026-04-19": {"cache_read": 95.504, "cache_write": 12.398, "input": 0.436, "output": 1.755},
}

# Extra host data from screenshot (dates assumed as-is, timezone TBD)
EXTRA_HOST = {
    # 04/13: no activity
    "2026-04-14": {"input": 0.192, "output": 2.740, "cache_read": 71.8, "cache_write": 28.0},
    # 04/15: no activity
    "2026-04-16": {"input": 0.002, "output": 0.334, "cache_read": 44.0, "cache_write": 2.4},
    "2026-04-17": {"input": 0.042, "output": 0.851, "cache_read": 58.3, "cache_write": 5.3},
    "2026-04-18": {"input": 0.013, "output": 0.371, "cache_read": 90.5, "cache_write": 6.9},
    "2026-04-19": {"input": 0.096, "output": 0.247, "cache_read": 13.0, "cache_write": 1.5},
}

MODEL_PRICING = {
    "opus-4-6":   {"input": 5.00,  "output": 25.00, "cache_write": 6.25,  "cache_read": 0.50},
    "opus-4-7":   {"input": 5.00,  "output": 25.00, "cache_write": 6.25,  "cache_read": 0.50},
    "opus-4-5":   {"input": 5.00,  "output": 25.00, "cache_write": 6.25,  "cache_read": 0.50},
    "sonnet-4-6": {"input": 3.00,  "output": 15.00, "cache_write": 3.75,  "cache_read": 0.30},
    "sonnet-4-5": {"input": 3.00,  "output": 15.00, "cache_write": 3.75,  "cache_read": 0.30},
    "sonnet-4":   {"input": 3.00,  "output": 15.00, "cache_write": 3.75,  "cache_read": 0.30},
    "haiku-4-5":  {"input": 1.00,  "output": 5.00,  "cache_write": 1.25,  "cache_read": 0.10},
}
DEFAULT_PRICING = {"input": 3.00, "output": 15.00, "cache_write": 3.75, "cache_read": 0.30}

# Weighted avg for AWS (no model split) and for extra host (no model split)
# ~60% Opus, ~20% Haiku, ~20% Sonnet from CloudTrail
AVG_PRICING = {
    "input":       0.60 * 5.00 + 0.20 * 1.00 + 0.20 * 3.00,   # 3.80
    "output":      0.60 * 25.00 + 0.20 * 5.00 + 0.20 * 15.00,  # 19.00
    "cache_write": 0.60 * 6.25 + 0.20 * 1.25 + 0.20 * 3.75,    # 4.75
    "cache_read":  0.60 * 0.50 + 0.20 * 0.10 + 0.20 * 0.30,    # 0.38
}


def edt_to_utc_date(edt_date_str, edt_hour):
    utc_hour = edt_hour - LOCAL_TZ_OFFSET_HOURS
    d = Date.fromisoformat(edt_date_str)
    if utc_hour >= 24:
        d = d + timedelta(days=1)
    return d.isoformat()


def model_key_from_archive(mk):
    mk = mk.lower().strip()
    for key in MODEL_PRICING:
        if key in mk:
            return key
    return None


def parse_archive():
    totals = defaultdict(lambda: defaultdict(lambda: {
        "input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "entries": 0,
        "by_model": defaultdict(lambda: {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0})
    }))

    sources = {}
    for month in ["2026-03.jsonl", "2026-04.jsonl"]:
        p = os.path.join(ARCHIVE_DIR, "local", "claude", month)
        if os.path.isfile(p):
            sources[f"local_{month}"] = ("local", p)

    devices_dir = os.path.join(ARCHIVE_DIR, "devices")
    if os.path.isdir(devices_dir):
        for device in os.listdir(devices_dir):
            if device in EXCLUDE_DEVICES:
                continue
            for month in ["2026-03.jsonl", "2026-04.jsonl"]:
                p = os.path.join(devices_dir, device, month)
                if os.path.isfile(p):
                    sources[f"{device}_{month}"] = (device, p)

    for source_key, (device, path) in sources.items():
        with open(path) as f:
            for line in f:
                try:
                    d = json.loads(line.strip())
                except json.JSONDecodeError:
                    continue

                edt_date = d.get("d", "")
                edt_hour = d.get("h", 0)
                mk = d.get("mk", "unknown")

                if edt_date not in ARCHIVE_DATES_NEEDED:
                    continue

                utc_date = edt_to_utc_date(edt_date, edt_hour)
                if utc_date not in TARGET_SET:
                    continue

                inp = d.get("in", 0)
                out = d.get("out", 0)
                cw = d.get("c5", 0) + d.get("c1", 0)
                cr = d.get("cr", 0)

                t = totals[device][utc_date]
                t["input"] += inp
                t["output"] += out
                t["cache_write"] += cw
                t["cache_read"] += cr
                t["entries"] += 1
                t["by_model"][mk]["input"] += inp
                t["by_model"][mk]["output"] += out
                t["by_model"][mk]["cache_write"] += cw
                t["by_model"][mk]["cache_read"] += cr

    return totals


def parse_remote_cache():
    totals = defaultdict(lambda: defaultdict(lambda: {
        "input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "entries": 0,
        "by_model": defaultdict(lambda: {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0})
    }))

    start_utc = datetime(2026, 4, 13, tzinfo=timezone.utc)
    end_utc = datetime(2026, 4, 20, tzinfo=timezone.utc)

    for device in os.listdir(REMOTE_CACHE_DIR):
        if device in EXCLUDE_DEVICES:
            continue
        path = os.path.join(REMOTE_CACHE_DIR, device, "usage.jsonl")
        if not os.path.isfile(path):
            continue

        seen = set()
        with open(path) as f:
            for line in f:
                try:
                    d = json.loads(line.strip())
                except json.JSONDecodeError:
                    continue

                ts_str = d.get("ts", "")
                if not ts_str:
                    continue
                try:
                    ts = datetime.fromisoformat(ts_str.replace("Z", "+00:00"))
                except (ValueError, TypeError):
                    continue

                if ts < start_utc or ts >= end_utc:
                    continue

                utc_date = ts.strftime("%Y-%m-%d")
                dk = (ts_str, d.get("m", ""), d.get("out", 0))
                if dk in seen:
                    continue
                seen.add(dk)

                mk = d.get("m", "unknown")
                inp = d.get("in", 0)
                out = d.get("out", 0)
                cw = d.get("c5", 0) + d.get("c1", 0)
                cr = d.get("cr", 0)

                t = totals[device][utc_date]
                t["input"] += inp
                t["output"] += out
                t["cache_write"] += cw
                t["cache_read"] += cr
                t["entries"] += 1
                t["by_model"][mk]["input"] += inp
                t["by_model"][mk]["output"] += out
                t["by_model"][mk]["cache_write"] += cw
                t["by_model"][mk]["cache_read"] += cr

    return totals


def calc_cost_by_model(by_model):
    total = 0.0
    for mk, tokens in by_model.items():
        pk = model_key_from_archive(mk)
        p = MODEL_PRICING.get(pk, DEFAULT_PRICING) if pk else DEFAULT_PRICING
        total += tokens["input"] / 1e6 * p["input"]
        total += tokens["output"] / 1e6 * p["output"]
        total += tokens["cache_write"] / 1e6 * p["cache_write"]
        total += tokens["cache_read"] / 1e6 * p["cache_read"]
    return total


def calc_cost_avg(tokens_m):
    return sum(tokens_m[tt] * AVG_PRICING[tt] for tt in ["input", "output", "cache_write", "cache_read"])


def main():
    print("=" * 100)
    print("TokenMonitor vs AWS Cost Explorer: Apr 13-19 2026 (ALL DATES UTC)")
    print("Sources: local Mac + DukeServer + athena + extra host. Excludes tianhe.")
    print("=" * 100)

    archive = parse_archive()
    remote = parse_remote_cache()

    # Merge TM sources (archive + remote-cache)
    tm_combined = {}
    for utc_date in TARGET_DATES_UTC:
        c = {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0,
             "by_model": defaultdict(lambda: {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0}),
             "sources": []}

        if utc_date in archive.get("local", {}):
            a = archive["local"][utc_date]
            for tt in ["input", "output", "cache_read", "cache_write"]:
                c[tt] += a[tt]
            for mk, v in a["by_model"].items():
                for tt in ["input", "output", "cache_read", "cache_write"]:
                    c["by_model"][mk][tt] += v[tt]
            c["sources"].append(f"local(a:{a['entries']})")

        for dev in ["DukeServer", "athena"]:
            if utc_date in archive.get(dev, {}):
                a = archive[dev][utc_date]
                for tt in ["input", "output", "cache_read", "cache_write"]:
                    c[tt] += a[tt]
                for mk, v in a["by_model"].items():
                    for tt in ["input", "output", "cache_read", "cache_write"]:
                        c["by_model"][mk][tt] += v[tt]
                c["sources"].append(f"{dev}(a:{a['entries']})")
            elif utc_date in remote.get(dev, {}):
                r = remote[dev][utc_date]
                for tt in ["input", "output", "cache_read", "cache_write"]:
                    c[tt] += r[tt]
                for mk, v in r["by_model"].items():
                    for tt in ["input", "output", "cache_read", "cache_write"]:
                        c["by_model"][mk][tt] += v[tt]
                c["sources"].append(f"{dev}(r:{r['entries']})")

        # Add extra host (values already in M tokens)
        if utc_date in EXTRA_HOST:
            eh = EXTRA_HOST[utc_date]
            for tt in ["input", "output", "cache_read", "cache_write"]:
                c[tt] += int(eh[tt] * 1e6)  # convert M back to raw tokens
            c["sources"].append("extra_host")

        tm_combined[utc_date] = c

    # Print comparison
    hdr = f"{'Date (UTC)':<12} {'Type':<14} {'TM (M tok)':>11} {'AWS (M tok)':>11} {'TM/AWS':>8} {'Diff (M)':>10} {'Diff%':>8}"
    print(f"\n{hdr}")
    print("-" * 78)

    tm_week = {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0}
    aws_week = {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0}
    tm_cost_week = 0.0
    aws_cost_week = 0.0

    for utc_date in TARGET_DATES_UTC:
        aws = AWS_DATA_DAILY[utc_date]
        tm = tm_combined[utc_date]
        src = ", ".join(tm["sources"]) if tm["sources"] else "NO DATA"

        # Cost: TM uses per-model for known models + avg for extra host
        tm_cost_model = calc_cost_by_model(tm["by_model"])
        # Extra host cost (no model info, use avg pricing)
        eh = EXTRA_HOST.get(utc_date, {})
        eh_cost = calc_cost_avg(eh) if eh else 0.0
        tm_cost = tm_cost_model + eh_cost

        aws_cost = calc_cost_avg(aws)
        tm_cost_week += tm_cost
        aws_cost_week += aws_cost

        print(f"\n{utc_date} [{src}]")
        for tt in ["input", "output", "cache_read", "cache_write"]:
            tm_val = tm[tt] / 1e6
            aws_val = aws[tt]
            diff = tm_val - aws_val
            ratio = f"{tm_val / aws_val * 100:.1f}%" if aws_val > 0.001 else "N/A"
            diff_pct = f"{diff / aws_val * 100:+.1f}%" if aws_val > 0.001 else "N/A"
            tm_week[tt] += tm_val
            aws_week[tt] += aws_val
            print(f"{'':12} {tt:<14} {tm_val:>11.3f} {aws_val:>11.3f} {ratio:>8} {diff:>+10.3f} {diff_pct:>8}")

        cost_diff = tm_cost - aws_cost
        cost_ratio = f"{tm_cost / aws_cost * 100:.1f}%" if aws_cost > 0.01 else "N/A"
        cost_diff_pct = f"{cost_diff / aws_cost * 100:+.1f}%" if aws_cost > 0.01 else "N/A"
        print(f"{'':12} {'COST ($)':<14} {tm_cost:>11.2f} {aws_cost:>11.2f} {cost_ratio:>8} {cost_diff:>+10.2f} {cost_diff_pct:>8}")

    # Week totals
    print(f"\n{'=' * 78}")
    print("WEEK TOTAL (Apr 13-19 UTC)")
    print("-" * 78)
    for tt in ["input", "output", "cache_read", "cache_write"]:
        tm_val = tm_week[tt]
        aws_val = aws_week[tt]
        diff = tm_val - aws_val
        ratio = f"{tm_val / aws_val * 100:.1f}%" if aws_val > 0.001 else "N/A"
        diff_pct = f"{diff / aws_val * 100:+.1f}%" if aws_val > 0.001 else "N/A"
        print(f"{'':12} {tt:<14} {tm_val:>11.3f} {aws_val:>11.3f} {ratio:>8} {diff:>+10.3f} {diff_pct:>8}")

    total_tm = sum(tm_week.values())
    total_aws = sum(aws_week.values())
    tok_diff = total_tm - total_aws
    tok_ratio = f"{total_tm / total_aws * 100:.1f}%" if total_aws > 0 else "N/A"
    tok_diff_pct = f"{tok_diff / total_aws * 100:+.1f}%" if total_aws > 0 else "N/A"
    print(f"\n{'':12} {'ALL TOKENS':<14} {total_tm:>11.3f} {total_aws:>11.3f} {tok_ratio:>8} {tok_diff:>+10.3f} {tok_diff_pct:>8}")

    cost_diff = tm_cost_week - aws_cost_week
    cost_ratio = f"{tm_cost_week / aws_cost_week * 100:.1f}%" if aws_cost_week > 0 else "N/A"
    cost_diff_pct = f"{cost_diff / aws_cost_week * 100:+.1f}%" if aws_cost_week > 0 else "N/A"
    print(f"{'':12} {'TOTAL COST $':<14} {tm_cost_week:>11.2f} {aws_cost_week:>11.2f} {cost_ratio:>8} {cost_diff:>+10.2f} {cost_diff_pct:>8}")

    print(f"\nNotes:")
    print(f"  - All dates UTC-aligned (archive EDT hours shifted +4)")
    print(f"  - Extra host dates used as-is (timezone unconfirmed, may need ±1 day shift)")
    print(f"  - TM cost: per-model pricing for known devices, weighted avg for extra host")
    print(f"  - AWS cost: weighted avg pricing (60% Opus, 20% Haiku, 20% Sonnet)")
    print(f"  - AWS includes root user (~29% calls) not tracked by TM")
    print(f"  - Subscription models (opus-4.7 etc) may cause <10% variance")


if __name__ == "__main__":
    main()
