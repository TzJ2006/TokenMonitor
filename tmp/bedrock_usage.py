#!/usr/bin/env python3
"""
Query CloudTrail for all Bedrock API calls and break down usage per IAM user/role.
Uses concurrent fetching and progress tracking.
Outputs summary to stdout, detailed JSON to tmp/bedrock_usage_report.json.
"""

import json
import subprocess
import collections
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

EVENT_NAMES = [
    "InvokeModel",
    "InvokeModelWithResponseStream",
    "Converse",
    "ConverseStream",
]

START_TIME = "2026-04-01"
END_TIME = "2026-04-26"
MAX_PAGES = 40  # safety limit per event type (~2000 events)


def lookup_events(event_name):
    all_events = []
    next_token = None
    page = 0

    while page < MAX_PAGES:
        page += 1
        cmd = [
            "aws", "cloudtrail", "lookup-events",
            "--lookup-attributes", f"AttributeKey=EventName,AttributeValue={event_name}",
            "--start-time", START_TIME,
            "--end-time", END_TIME,
            "--max-results", "50",
            "--output", "json",
        ]
        if next_token:
            cmd.extend(["--next-token", next_token])

        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode != 0:
            print(f"  [WARN] {event_name} page {page}: {result.stderr.strip()}", file=sys.stderr)
            break

        data = json.loads(result.stdout)
        events = data.get("Events", [])
        all_events.extend(events)
        print(f"  {event_name}: page {page}, got {len(events)}, total {len(all_events)}", flush=True)

        next_token = data.get("NextToken")
        if not next_token or not events:
            break

    return event_name, all_events


def parse_event(event):
    username = event.get("Username", "unknown")

    model_id = "unknown"
    region = "unknown"
    try:
        detail = json.loads(event.get("CloudTrailEvent", "{}"))
        region = detail.get("awsRegion", "unknown")

        resources = event.get("Resources", [])
        for r in resources:
            arn = r.get("ResourceName", "")
            if "model/" in arn or "foundation-model/" in arn:
                model_id = arn.split("/")[-1].split(":")[0] if "/" in arn else arn
                break

        if model_id == "unknown":
            req = detail.get("requestParameters", {})
            mid = req.get("modelId", "")
            if mid:
                model_id = mid.split("/")[-1].split(":")[0] if "/" in mid else mid
    except Exception:
        pass

    return {
        "username": username,
        "model": model_id,
        "region": region,
    }


def main():
    print(f"Querying CloudTrail: {START_TIME} to {END_TIME}")
    print(f"APIs: {', '.join(EVENT_NAMES)}")
    print(f"Max pages per API: {MAX_PAGES}\n")

    all_parsed = []
    event_type_counts = {}

    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = {pool.submit(lookup_events, name): name for name in EVENT_NAMES}
        for future in as_completed(futures):
            event_name, events = future.result()
            event_type_counts[event_name] = len(events)
            for e in events:
                parsed = parse_event(e)
                parsed["api"] = event_name
                all_parsed.append(parsed)

    print(f"\nTotal events: {len(all_parsed)}")

    user_calls = collections.Counter()
    user_models = collections.defaultdict(collections.Counter)
    user_apis = collections.defaultdict(collections.Counter)

    for p in all_parsed:
        user_calls[p["username"]] += 1
        user_models[p["username"]][p["model"]] += 1
        user_apis[p["username"]][p["api"]] += 1

    print("\n" + "=" * 60)
    print("BEDROCK USAGE PER IAM USER (April 2026)")
    print("=" * 60)

    for user, count in user_calls.most_common():
        pct = count / len(all_parsed) * 100 if all_parsed else 0
        print(f"\n  {user}: {count} calls ({pct:.1f}%)")
        print(f"    By API:")
        for api, ac in user_apis[user].most_common():
            print(f"      {api}: {ac}")
        print(f"    By Model:")
        for model, mc in user_models[user].most_common():
            print(f"      {model}: {mc}")

    report = {
        "query_range": {"start": START_TIME, "end": END_TIME},
        "total_events": len(all_parsed),
        "by_event_type": event_type_counts,
        "by_user": {},
    }
    for user in user_calls:
        report["by_user"][user] = {
            "total_calls": user_calls[user],
            "percentage": round(user_calls[user] / len(all_parsed) * 100, 1) if all_parsed else 0,
            "by_api": dict(user_apis[user]),
            "by_model": dict(user_models[user]),
        }

    report_path = "tmp/bedrock_usage_report.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2, default=str)

    print(f"\nReport saved to: {report_path}")


if __name__ == "__main__":
    main()
