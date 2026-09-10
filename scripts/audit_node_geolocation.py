#!/usr/bin/env python3
"""Offline comparison of published node locations with a City MMDB.

Requires maxminddb. No network calls, cache edits, or automatic corrections.
Disagreements are review candidates, not measured errors or accuracy scores.
"""

import argparse
import hashlib
import ipaddress
import json
import math
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path


def coordinates(lat, lon):
    if isinstance(lat, bool) or isinstance(lon, bool):
        return None
    if not isinstance(lat, (int, float)) or not isinstance(lon, (int, float)):
        return None
    if not (math.isfinite(lat) and math.isfinite(lon)):
        return None
    return (lat, lon) if -90 <= lat <= 90 and -180 <= lon <= 180 else None


def distance_km(left, right):
    if left is None or right is None:
        return None
    a, b = map(math.radians, left)
    c, d = map(math.radians, right)
    h = math.sin((c - a) / 2) ** 2 + math.cos(a) * math.cos(c) * math.sin((d - b) / 2) ** 2
    return 12742 * math.asin(math.sqrt(max(0, min(1, h))))


def compare(node, cached, alternative, threshold):
    alternative = alternative or {}
    country = alternative.get("country", {})
    location = alternative.get("location", {})
    baseline_code = cached.get("country_code")
    # A manual location may disagree with the cache; never borrow its country code.
    if node.get("country") != cached.get("country"):
        baseline_code = None
    other_code = country.get("iso_code")
    km = distance_km(
        coordinates(node.get("lat"), node.get("lon")),
        coordinates(location.get("latitude"), location.get("longitude")),
    )
    country_diff = bool(baseline_code and other_code and baseline_code.upper() != other_code.upper())
    reasons = []
    if country_diff:
        reasons.append("country_disagreement")
    if km is not None and km > threshold:
        reasons.append("coordinate_disagreement")
    return {
        "ip": node["ip"],
        "asn": cached.get("asn"),
        "published": {k: node.get(k) for k in (
            "city", "country", "lat", "lon", "geo_source", "geo_confidence", "geo_updated_at"
        )},
        "published_country_code": baseline_code,
        "comparison": {
            "city": alternative.get("city", {}).get("names", {}).get("en"),
            "country_code": other_code,
            "lat": location.get("latitude"),
            "lon": location.get("longitude"),
            "accuracy_radius_km": location.get("accuracy_radius"),
        },
        "distance_km": round(km, 2) if km is not None else None,
        "country_comparable": bool(baseline_code and other_code),
        "review_reasons": reasons,
        "manual": node.get("geo_source") == "manual",
    }


def fingerprint(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return {"file": path.name, "sha256": digest.hexdigest()}


def audit(cache, maps, reader, threshold):
    grouped = {}
    node_count = 0
    for name, nodes in maps:
        for node in nodes:
            ipaddress.ip_address(node["ip"])
            node_count += 1
            grouped.setdefault(node["ip"], []).append((name, node))
    rows = []
    for ip, entries in sorted(grouped.items(), key=lambda item: (
        ipaddress.ip_address(item[0]).version, int(ipaddress.ip_address(item[0]))
    )):
        # Snapshots of separate chains may have been written at different times.
        node = max((n for _, n in entries), key=lambda n: n.get("geo_updated_at", 0))
        row = compare(node, cache.get(ip, {}), reader.get(ip), threshold)
        row["maps"] = sorted({name for name, _ in entries})
        row["node_rows"] = len(entries)
        row["published_variants"] = len({
            (n.get("city"), n.get("country"), n.get("lat"), n.get("lon"), n.get("geo_source"))
            for _, n in entries
        })
        rows.append(row)
    return {
        "notice": "Disagreement audit only. Neither source is ground truth. No locations were changed.",
        "coordinate_threshold_km": threshold,
        "summary": {
            "node_rows": node_count,
            "unique_ips": len(rows),
            "coordinates_compared": sum(r["distance_km"] is not None for r in rows),
            "countries_compared": sum(r["country_comparable"] for r in rows),
            "review_candidates": sum(bool(r["review_reasons"]) for r in rows),
            "reasons": dict(Counter(reason for r in rows for reason in r["review_reasons"])),
            "multiple_published_variants": sum(r["published_variants"] > 1 for r in rows),
        },
        "rows": rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", required=True, type=Path)
    parser.add_argument("--nodes", required=True, action="append", type=Path)
    parser.add_argument("--mmdb", required=True, type=Path)
    parser.add_argument("--threshold-km", type=float, default=100)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not math.isfinite(args.threshold_km) or args.threshold_km <= 0:
        parser.error("--threshold-km must be finite and positive")
    inputs = [args.cache, *args.nodes, args.mmdb]
    if args.output.resolve() in {p.resolve() for p in inputs}:
        parser.error("output must not overwrite an input")
    try:
        import maxminddb
    except ImportError:
        parser.error("install maxminddb in a virtual environment first")
    cache = json.loads(args.cache.read_text())
    maps = [(p.name, json.loads(p.read_text())) for p in args.nodes]
    with maxminddb.open_database(str(args.mmdb)) as reader:
        report = audit(cache, maps, reader, args.threshold_km)
        metadata = reader.metadata()
        report["database"] = {
            "type": metadata.database_type,
            "build_epoch": metadata.build_epoch,
            "description": metadata.description,
        }
    report["generated_at"] = datetime.now(timezone.utc).isoformat()
    report["inputs"] = [fingerprint(p) for p in inputs]
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False, allow_nan=False) + "\n")
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
