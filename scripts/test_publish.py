"""Release source synchronization must be idempotent and preserve version tags."""
import importlib.util
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, call, patch


sys.path.insert(0, str(Path(__file__).parent / "gitlab"))
spec = importlib.util.spec_from_file_location("release_publish", Path(__file__).parent / "gitlab/publish.py")
publish = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publish)


class GitlabSourceTests(unittest.TestCase):
    def setUp(self):
        self.env = patch.dict(os.environ, {
            "CI_COMMIT_SHA": "a" * 40,
            "CI_COMMIT_TAG": "v4.1.13",
            "CI_API_V4_URL": "https://gitlab.example/api/v4",
            "CI_PROJECT_ID": "1",
            "CI_PROJECT_URL": "https://gitlab.example/project",
            "GITLAB_RELEASE_TOKEN": "synthetic-token",
        })
        self.env.start()
        self.addCleanup(self.env.stop)
        self.gitlab = publish.Gitlab()

    def test_missing_version_source_is_pushed_before_release_lookup(self):
        self.gitlab.api = Mock(side_effect=[None, None])
        with patch.object(publish, "git_push") as push:
            release = self.gitlab.release("v4.1.13", "notes", False)
        push.assert_called_once_with("https://gitlab.example/project.git", "refs/tags/v4.1.13",
                                     "synthetic-token", "oauth2", skip_ci=True)
        self.assertEqual(self.gitlab.api.call_args_list, [
            call("GET", "/repository/tags/v4.1.13", missing=True),
            call("GET", "/releases/v4.1.13", missing=True),
        ])
        self.assertTrue(release["pending"])

    def test_existing_tag_at_source_commit_needs_no_push(self):
        existing = {"tag_name": "v4.1.13"}
        self.gitlab.api = Mock(side_effect=[{"commit": {"id": "a" * 40}}, existing])
        with patch.object(publish, "git_push") as push:
            self.assertIs(self.gitlab.release("v4.1.13", "notes", False), existing)
        push.assert_not_called()

    def test_conflicting_version_tag_fails_before_any_release_mutation(self):
        self.gitlab.api = Mock(return_value={"commit": {"id": "b" * 40}})
        with patch.object(publish, "git_push") as push:
            with self.assertRaisesRegex(ValueError, "different commit"):
                self.gitlab.release("v4.1.13", "notes", False)
        push.assert_not_called()
        self.gitlab.api.assert_called_once_with("GET", "/repository/tags/v4.1.13", missing=True)

    def test_rolling_channels_keep_their_existing_publication_path(self):
        for tag, source_tag in [("staging", ""), ("nightly", ""), ("release", "v4.1.13")]:
            with self.subTest(tag=tag), patch.dict(os.environ, {"CI_COMMIT_TAG": source_tag}):
                self.gitlab.api = Mock(return_value=None)
                with patch.object(publish, "git_push") as push:
                    self.gitlab.release(tag, "notes", True)
                push.assert_not_called()
                self.gitlab.api.assert_called_once_with("GET", f"/releases/{tag}", missing=True)

    def test_source_push_skips_duplicate_ci_without_forcing_the_tag(self):
        with patch.object(publish.subprocess, "run") as run:
            publish.git_push("https://gitlab.example/project.git", "refs/tags/v4.1.13",
                             "synthetic-token", "oauth2", skip_ci=True)
        command = run.call_args.args[0]
        self.assertEqual(command, ["git", "push", "-o", "ci.skip", "https://gitlab.example/project.git",
                                   "a" * 40 + ":refs/tags/v4.1.13"])
        self.assertNotIn("synthetic-token", " ".join(command))


if __name__ == "__main__":
    unittest.main()
