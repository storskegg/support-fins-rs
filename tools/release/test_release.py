"""Unit tests of the version computation (tools/release/release.py), run by `just gate` (the `version` step):

    python3 -m unittest discover -s tools/release
"""
import datetime
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import release  # noqa: E402

D = datetime.date


class IsoWeek(unittest.TestCase):
    def test_the_last_days_of_2026_and_the_first_of_2027_are_week_53_of_2026(self):
        # 2026 starts on a Thursday, so it has 53 ISO weeks; 2027-01-01..03 (Fri-Sun) still belong to it
        self.assertEqual(release.iso_week(D(2026, 12, 31)), (2026, 53))
        self.assertEqual(release.iso_week(D(2027, 1, 1)), (2026, 53))
        self.assertEqual(release.iso_week(D(2027, 1, 3)), (2026, 53))
        self.assertEqual(release.iso_week(D(2027, 1, 4)), (2027, 1))

    def test_a_late_december_day_can_be_week_1_of_the_next_year(self):
        self.assertEqual(release.iso_week(D(2024, 12, 30)), (2025, 1))
        self.assertEqual(release.next_version(D(2024, 12, 30), []), "2025.1.0")

    def test_an_early_january_day_can_be_week_52_of_the_year_before(self):
        self.assertEqual(release.next_version(D(2022, 1, 1), []), "2021.52.0")

    def test_weeks_start_on_monday(self):
        self.assertEqual(release.iso_week(D(2026, 11, 1)), (2026, 44))   # a Sunday
        self.assertEqual(release.iso_week(D(2026, 11, 2)), (2026, 45))   # the Monday after

    def test_week_53_years(self):
        for year in (2015, 2020, 2026, 2032):
            self.assertEqual(release.next_version(D(year, 12, 31), []), f"{year}.53.0", year)
        # a 52-week year has no week 53: its 31 December is week 1 of the next
        self.assertEqual(release.next_version(D(2025, 12, 31), []), "2026.1.0")


class NextVersion(unittest.TestCase):
    def test_today_is_week_44_of_2026(self):
        self.assertEqual(release.next_version(D(2026, 11, 1), []), "2026.44.0")

    def test_patch_counts_the_weeks_releases(self):
        day = D(2026, 12, 2)   # week 49
        self.assertEqual(release.next_version(day, []), "2026.49.0")
        self.assertEqual(release.next_version(day, ["v2026.49.0"]), "2026.49.1")
        self.assertEqual(release.next_version(day, ["v2026.49.0", "v2026.49.1"]), "2026.49.2")
        # the highest, not the count: a deleted tag leaves a gap
        self.assertEqual(release.next_version(day, ["v2026.49.0", "v2026.49.7"]), "2026.49.8")
        self.assertEqual(release.next_version(day, ["v2026.49.10", "v2026.49.9"]), "2026.49.11")

    def test_other_weeks_and_years_do_not_count(self):
        day = D(2026, 12, 2)
        self.assertEqual(release.next_version(day, ["v2026.48.3", "v2025.49.2", "v2027.49.0"]), "2026.49.0")

    def test_no_leading_zero_on_the_week(self):
        self.assertEqual(release.next_version(D(2026, 1, 28), []), "2026.5.0")
        self.assertEqual(release.next_version(D(2026, 3, 2), []), "2026.10.0")

    def test_tags_that_are_not_versions_are_ignored(self):
        day = D(2026, 1, 28)   # week 5
        tags = ["v2026.05.3", "2026.5.4", "v2026.5.03", "v2026.5.1-rc1", "release-1", "v1.0.0", " v2026.5.1 "]
        self.assertEqual(release.next_version(day, tags), "2026.5.2")

    def test_the_result_is_semver(self):
        semver = r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$"
        day = D(2026, 1, 1)
        for _ in range(800):
            self.assertRegex(release.next_version(day, []), semver)
            self.assertRegex(release.next_version(day, []), release.VERSION_RE)
            day += datetime.timedelta(days=1)


class Files(unittest.TestCase):
    def test_cargo_version_is_the_workspace_packages(self):
        text = '[workspace]\nmembers = ["a"]\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2024"\n\n[workspace.dependencies]\nx = { version = "1" }\n'
        self.assertEqual(release.cargo_version(text), "0.1.0")
        out = release.set_cargo_version(text, "2026.44.0")
        self.assertEqual(release.cargo_version(out), "2026.44.0")
        self.assertIn('x = { version = "1" }', out)
        self.assertEqual(out.replace("2026.44.0", "0.1.0"), text)

    def test_the_repositorys_files_agree(self):
        self.assertEqual(release.check(), 0)


if __name__ == "__main__":
    unittest.main()
