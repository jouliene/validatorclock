#!/usr/bin/env python3
"""Summarize recorded Rust all-network trial; no network, no accuracy claims."""
import argparse
import collections
import csv
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot_dir", type=Path)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    report = json.loads((args.snapshot_dir / "report.json").read_text())
    replay = json.loads((args.snapshot_dir / "legacy-replay.json").read_text())
    unique = {row["ip"]: row for row in report["rows"]}
    entries = [r["entry"] for r in unique.values() if r["entry"]]
    jobs = [e["globalping"] for e in entries if e["globalping"]]
    moved = [r for r in unique.values() if not r["manual"] and (r["distance_km"] or 0) > 100]
    same_primary = {r["ip"]: r for r in replay["rows"]}
    summary = {
        "snapshot_at": report["snapshot_at"], "finished_at": report["finished_at"],
        "chains": report["chains"], "unique_ips": len(unique),
        "confidence": dict(collections.Counter(e["confidence"] for e in entries)),
        "completed": sum(e["completed_at"] > 0 for e in entries),
        "manual": sum(r["manual"] for r in unique.values()),
        "moved_over_100km": len(moved),
        "measured_moves": [r["ip"] for r in moved if r["entry"]["confidence"] == "measured_metro"],
        "unmeasured_moves": [r["ip"] for r in moved if r["entry"]["confidence"] != "measured_metro"],
        "unmeasured_moves_without_disputed_flag": [r["ip"] for r in moved if r["entry"]["confidence"] == "approximate"],
        "same_primary_algorithm_differences_over_100km": sum(not r["manual"] and (r["distance_km"] or 0) > 100 for r in same_primary.values()),
        "requests": report["total_budget"]["requests_by_source"],
        "total_http_requests": report["total_budget"]["total_requests"],
        "globalping_jobs": len(jobs), "finished_jobs": sum(j["finished"] for j in jobs),
        "globalping_probes_reported": sum((j["response"] or {}).get("probesCount", 0) for j in jobs),
        "measurement_target_asns": sorted({r["new_cache"]["asn"] for r in unique.values() if (r["entry"] or {}).get("globalping")}),
        "warm_http_requests": report["immediate_warm_http_requests"],
        "restart_http_requests": report["immediate_restart_http_requests"],
        "post_completion_reopen_http_requests": report.get("post_completion_reopen_http_requests"),
        "failed_primary": report["failed_primary"], "failed_secondary": report["failed_secondary"],
        "method": report["method"], "replay_method": replay["method"],
        "input_sha256": {name: hashlib.sha256((args.snapshot_dir / name).read_bytes()).hexdigest()
                         for name in ["snapshot.json", "geo_cache.json", "inventory.json"]},
    }
    (args.output_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    (args.output_dir / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    (args.output_dir / "legacy-replay.json").write_text(json.dumps(replay, indent=2) + "\n")
    fields = ["chain", "ip", "manual", "old_city", "old_country", "new_city", "new_country", "confidence", "distance_km", "completed", "globalping_id", "reasons"]
    with (args.output_dir / "all-networks.csv").open("w", newline="") as out:
        writer = csv.DictWriter(out, fieldnames=fields)
        writer.writeheader()
        for r in report["rows"]:
            old, new, e = r["old_map"] or {}, r["new_cache"] or {}, r["entry"] or {}
            if r["manual"]:
                new = old
            writer.writerow({"chain": r["chain"], "ip": r["ip"], "manual": r["manual"],
                "old_city": old.get("city"), "old_country": old.get("country"),
                "new_city": new.get("city"), "new_country": new.get("country"),
                "confidence": "manual" if r["manual"] else e.get("confidence"),
                "distance_km": round(r["distance_km"], 3) if r["distance_km"] is not None else "",
                "completed": e.get("completed_at", 0) > 0,
                "globalping_id": (e.get("globalping") or {}).get("id"),
                "reasons": "; ".join(e.get("reasons", []))})
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
