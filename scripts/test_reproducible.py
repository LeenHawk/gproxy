"""Regression checks for release reproducibility, without publishing artifacts."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile


def module(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


archive = module("reproducible-archive")
verify = module("verify-reproducible")
build = module("reproducible-run")
replay = module("reproduce-build")
zip_times = module("reproducible-zip-timestamps")


class ReproducibleTests(unittest.TestCase):
    def test_archive_ignores_order_and_mtime_but_preserves_contents_and_modes(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1700000000"}):
            root = Path(directory)
            for attempt in (1, 2):
                tree = root / str(attempt)
                tree.mkdir()
                for name in ("binary", "data") if attempt == 1 else ("data", "binary"):
                    path = tree / name
                    path.write_bytes(name.encode())
                    path.chmod(0o755 if name == "binary" else 0o644)
                    os.utime(path, (attempt * 1000000000, attempt * 1000000000))
                archive.archive(tree, root / f"{attempt}.zip", ["."])
            self.assertEqual((root / "1.zip").read_bytes(), (root / "2.zip").read_bytes())
            with zipfile.ZipFile(root / "1.zip") as package:
                self.assertEqual(package.read("binary"), b"binary")
                self.assertEqual(package.getinfo("binary").external_attr >> 16 & 0o777, 0o755)

    @unittest.skipIf(os.name == "nt", "Creating Windows symlinks requires extra privileges")
    def test_bundle_symlink_is_not_dereferenced(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1700000000"}):
            root = Path(directory)
            (root / "app").mkdir()
            (root / "app/Applications").symlink_to("/Applications")
            archive.archive(root / "app", root / "bundle.zip", ["."])
            with zipfile.ZipFile(root / "bundle.zip") as package:
                self.assertEqual(package.read("Applications"), b"/Applications")
                self.assertEqual(package.getinfo("Applications").external_attr >> 16 & 0o170000, 0o120000)

    def test_missing_targets_and_changed_bytes_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first, second = root / "a", root / "b"
            first.mkdir()
            second.mkdir()
            (first / "app.zip").write_bytes(b"a")
            (second / "app.zip").write_bytes(b"b")
            result = verify.compare(first, second, ["app.zip", "missing.apk"])
            self.assertEqual([row["status"] for row in result], ["different", "missing"])

    def test_zip_timestamp_fix_preserves_alignment_and_compressed_data(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for attempt, date in [(1, (2020, 1, 1, 0, 0, 0)), (2, (2026, 1, 1, 0, 0, 0))]:
                path = root / f"{attempt}.hap"
                with zipfile.ZipFile(path, "w") as package:
                    item = zipfile.ZipInfo("module", date)
                    item.extra = b"\xff\xff\x04\x00pad!"
                    package.writestr(item, b"unchanged payload" * 100, compress_type=zipfile.ZIP_DEFLATED)
                with zipfile.ZipFile(path) as package:
                    before = package.getinfo("module")
                zip_times.normalize(path, 1700000000)
                with zipfile.ZipFile(path) as package:
                    after = package.getinfo("module")
                    self.assertEqual((before.header_offset, before.compress_size, before.extra),
                                     (after.header_offset, after.compress_size, after.extra))
                    self.assertEqual(package.read("module"), b"unchanged payload" * 100)
            self.assertEqual((root / "1.hap").read_bytes(), (root / "2.hap").read_bytes())

    def test_clean_rebuild_removes_readonly_go_module_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            tree = Path(directory) / "owned-build"
            module_dir = tree / "go-mod/cache/example"
            module_dir.mkdir(parents=True)
            source = module_dir / "source.go"
            source.write_text("package example\n")
            source.chmod(0o444)
            module_dir.chmod(0o555)
            replay.remove_tree(tree)
            self.assertFalse(tree.exists())

    def test_build_preserves_existing_rust_flags_and_source_identity(self):
        with patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1700000000", "GPROXY_BUILD_HASH": "a" * 40,
                                    "RUSTFLAGS": "-C target-feature=+crt-static"}, clear=True):
            env = build.build_environment("aarch64-pc-windows-msvc")
        flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
        self.assertIn("target-feature=+crt-static", flags)
        self.assertIn("link-arg=/Brepro", flags)
        self.assertEqual(env["SOURCE_DATE_EPOCH"], "1700000000")
        self.assertEqual(env["GPROXY_BUILD_HASH"], "a" * 40)


if __name__ == "__main__":
    unittest.main()
