"""Submission lifecycle tests without GitLab writes."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import yaml

spec = importlib.util.spec_from_file_location(
    "fdroid_submit", Path(__file__).with_name("fdroid-submit.py"))
submitter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(submitter)


def document(code):
    version = f"4.0.{code}"
    return {"License": "AGPL-3.0-or-later", "AutoName": "GPROXY",
            "RepoType": "git", "Repo": "https://github.com/LeenHawk/gproxy.git",
            "Builds": [{"versionName": version, "versionCode": code,
                        "commit": str(code) * 40, "build": ["reviewer-build-command"]}],
            "CurrentVersion": version, "CurrentVersionCode": code}


class SubmissionTests(unittest.TestCase):
    def setUp(self):
        directory = submitter.ROOT / "dist/mobile/fdroid"
        directory.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=directory)
        self.addCleanup(self.directory.cleanup)
        output = Path(self.directory.name) / f"{submitter.APP}.yml"
        patcher = patch.object(submitter, "OUTPUT", output)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.dict(submitter.os.environ, {"FDROID_GITLAB_TOKEN": "test-token"})
        patcher.start()
        self.addCleanup(patcher.stop)
        self.accepted = None
        self.current = None
        self.mr = None
        self.writes = []
        patcher = patch.object(submitter, "api", side_effect=self.api)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(submitter, "recipe", side_effect=self.recipe)
        patcher.start()
        self.addCleanup(patcher.stop)

    def api(self, path, method="GET", data=None, optional=False):
        if method != "GET":
            self.writes.append((path, method, deepcopy(data)))
            return {"web_url": "https://gitlab.com/fdroid/fdroiddata/-/merge_requests/1"}
        if path == "projects/fdroid%2Ffdroiddata":
            return {"id": 1, "default_branch": "master"}
        if path == "projects/LeenHawk%2Ffdroiddata":
            return {"id": 2, "visibility": "public"}
        if "/merge_requests?" in path:
            return [self.mr] if self.mr else []
        if "/repository/branches/" in path:
            return {"name": "exists"} if self.current else None
        self.fail(f"Unexpected API request: {path}")

    def recipe(self, project, ref):
        doc = self.accepted if project == 1 else self.current
        return deepcopy(doc), "last-file-commit" if doc else None

    def pending(self, code):
        self.current = document(code)
        self.mr = {"iid": 1, "source_project_id": 2, "source_branch": "new-app-gproxy",
                   "description": "## Checklist\n\n* [ ] Review pending",
                   "web_url": "https://gitlab.com/fdroid/fdroiddata/-/merge_requests/1"}

    def test_existing_release_and_older_release_do_not_write(self):
        self.pending(4)
        submitter.submit(document(4))
        submitter.submit(document(3))
        self.assertEqual(self.writes, [])

    def test_accepted_release_does_not_create_redundant_mr(self):
        self.accepted = document(4)
        submitter.submit(document(3))
        self.assertEqual(self.writes, [])

    def test_initial_mr_updates_latest_build_and_keeps_reviewer_recipe(self):
        self.pending(3)
        generated = document(4)
        generated["Builds"][0]["build"] = ["new-template-command"]
        submitter.submit(generated)
        commit = self.writes[0][2]
        self.assertEqual(commit["branch"], "new-app-gproxy")
        self.assertEqual(commit["actions"][0]["last_commit_id"], "last-file-commit")
        content = yaml.safe_load(commit["actions"][0]["content"])
        self.assertEqual([b["versionCode"] for b in content["Builds"]], [4])
        self.assertEqual(content["Builds"][0]["build"], "reviewer-build-command")
        self.assertEqual(self.writes[1][0:2], ("projects/1/merge_requests/1", "PUT"))

    def test_version_update_preserves_upstream_signing(self):
        self.pending(3)
        self.current["Binaries"] = "https://example.com/v%v/app.apk"
        self.current["AllowedAPKSigningKeys"] = ["a" * 64]
        submitter.submit(document(4))
        content = yaml.safe_load(self.writes[0][2]["actions"][0]["content"])
        self.assertEqual(content["Binaries"], self.current["Binaries"])
        self.assertEqual(content["AllowedAPKSigningKeys"], "a" * 64)
        description = self.writes[1][2]["description"]
        self.assertIn("upstream-signed APK", description)
        self.assertNotIn("It uses F-Droid signing", description)

    def test_version_update_preserves_reviewed_checklist(self):
        self.pending(3)
        checklist = "## Checklist\n\n* [x] Reviewed by maintainer"
        self.mr["description"] = checklist + "\n\n## Submission details\n\nOld release"
        submitter.submit(document(4))
        description = self.writes[1][2]["description"]
        self.assertTrue(description.startswith(checklist))
        self.assertNotIn("Old release", description)
        self.assertEqual(description.count("## Submission details"), 1)

    def test_after_inclusion_preserves_history_and_starts_from_upstream(self):
        self.accepted = document(3)
        submitter.submit(document(4))
        commit = self.writes[0][2]
        self.assertEqual((commit["start_project"], commit["start_branch"]), (1, "master"))
        self.assertEqual(commit["actions"][0]["action"], "update")
        content = yaml.safe_load(commit["actions"][0]["content"])
        self.assertEqual([b["versionCode"] for b in content["Builds"]], [3, 4])
        self.assertEqual(self.writes[1][2]["target_project_id"], 1)
        self.assertEqual(self.writes[1][2]["title"], "GPROXY: update to 4.0.4")
        self.assertIn("## Required", self.writes[1][2]["description"])

    def test_retry_after_commit_creates_missing_mr_without_another_commit(self):
        self.current = document(4)
        submitter.submit(document(4))
        self.assertEqual(len(self.writes), 1)
        self.assertEqual(self.writes[0][0:2], ("projects/2/merge_requests", "POST"))

    def test_same_version_repairs_missing_template_without_recipe_commit(self):
        self.pending(4)
        self.mr["description"] = "Old automated description without checklist"
        submitter.submit(document(4))
        self.assertEqual(len(self.writes), 1)
        self.assertEqual(self.writes[0][0:2], ("projects/1/merge_requests/1", "PUT"))
        self.assertTrue(self.writes[0][2]["description"].startswith("## Checklist"))

    def test_same_version_with_different_source_is_rejected(self):
        self.pending(4)
        self.current["Builds"][0]["commit"] = "a" * 40
        with self.assertRaisesRegex(ValueError, "different source commit"):
            submitter.submit(document(4))
        self.assertEqual(self.writes, [])

    def test_dry_run_does_not_write(self):
        self.pending(3)
        submitter.submit(document(4), dry_run=True)
        self.assertEqual(self.writes, [])
        self.assertEqual(yaml.safe_load(submitter.OUTPUT.read_text())["CurrentVersionCode"], 4)

    def test_new_inclusion_creates_file(self):
        submitter.submit(document(4))
        self.assertEqual(self.writes[0][2]["actions"][0]["action"], "create")
        self.assertEqual(self.writes[1][2]["title"], "New app: GPROXY")
        description = self.writes[1][2]["description"]
        self.assertIn("## Checklist", description)
        self.assertIn("### Policy", description)
        self.assertIn("### Pipeline", description)
        self.assertIn("reproducible builds have not been established", description)
        self.assertNotIn("[x]", description)


if __name__ == "__main__":
    unittest.main()
