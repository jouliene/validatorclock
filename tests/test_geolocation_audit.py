"""Regression checks for the offline audit; no external services needed."""

import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "geo_audit", Path(__file__).resolve().parents[1] / "scripts/audit_node_geolocation.py"
)
geo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geo)


class AuditTests(unittest.TestCase):
    def test_same_country_distant_city_is_reviewed(self):
        node = {"ip": "67.213.125.125", "country": "Australia", "lat": -31.2532, "lon": 146.921}
        alternative = {"country": {"iso_code": "AU"}, "location": {"latitude": -33.8688, "longitude": 151.2093}}
        row = geo.compare(node, {"country": "Australia", "country_code": "AU"}, alternative, 100)
        self.assertEqual(row["review_reasons"], ["coordinate_disagreement"])
        self.assertGreater(row["distance_km"], 490)
        self.assertLess(row["distance_km"], 510)

    def test_missing_coordinates_are_not_zero_distance(self):
        row = geo.compare({"ip": "1.1.1.1"}, {}, None, 100)
        self.assertIsNone(row["distance_km"])
        self.assertFalse(row["country_comparable"])

    def test_same_name_does_not_hide_coordinate_disagreement(self):
        node = {"ip": "178.63.165.86", "city": "Falkenstein", "lat": 50.4777, "lon": 12.3649}
        other = {"city": {"names": {"en": "Falkenstein"}}, "location": {"latitude": 49.1, "longitude": 12.4}}
        self.assertIn("coordinate_disagreement", geo.compare(node, {}, other, 100)["review_reasons"])

    def test_manual_country_does_not_inherit_cache_country(self):
        row = geo.compare(
            {"ip": "1.1.1.1", "country": "France", "geo_source": "manual"},
            {"country": "Germany", "country_code": "DE"},
            {"country": {"iso_code": "FR"}}, 100,
        )
        self.assertIsNone(row["published_country_code"])
        self.assertTrue(row["manual"])

    def test_invalid_and_nonfinite_coordinates(self):
        for lat, lon in [(91, 0), (0, -181), (float("nan"), 0), (0, float("inf")), (True, 0)]:
            self.assertIsNone(geo.coordinates(lat, lon))
        self.assertAlmostEqual(geo.distance_km((0, 179.9), (0, -179.9)), 22.239, places=2)

    def test_shared_ip_is_counted_once_and_latest_map_is_compared(self):
        class Reader:
            def get(self, ip):
                return None
        a = {"ip": "1.1.1.1", "city": "old", "geo_updated_at": 1}
        b = {"ip": "1.1.1.1", "city": "new", "geo_updated_at": 2}
        report = geo.audit({}, [("ton", [a]), ("other", [b])], Reader(), 100)
        self.assertEqual(report["summary"]["unique_ips"], 1)
        self.assertEqual(report["summary"]["node_rows"], 2)
        self.assertEqual(report["rows"][0]["published"]["city"], "new")
        self.assertEqual(report["rows"][0]["published_variants"], 2)


if __name__ == "__main__":
    unittest.main()
