from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

from setuptools import Distribution, setup
from setuptools.command.bdist_wheel import bdist_wheel
from setuptools.command.build_py import build_py


ROOT = Path(__file__).resolve().parent


def native_library_name() -> str:
    if sys.platform == "win32":
        return "mfsd.dll"
    if sys.platform == "darwin":
        return "libmfsd.dylib"
    if sys.platform.startswith("linux"):
        return "libmfsd.so"
    raise RuntimeError(f"MFSD does not support Python on {sys.platform!r}")


class BuildPythonWithRust(build_py):
    """Build the Rust cdylib and place it beside the Python modules."""

    def run(self) -> None:
        profile = os.environ.get("MFSD_CARGO_PROFILE", "release")
        command = ["cargo", "build"]
        if profile == "release":
            command.append("--release")
        elif profile != "debug":
            command.extend(["--profile", profile])

        subprocess.run(command, cwd=ROOT, check=True)
        super().run()

        source = ROOT / "target" / profile / native_library_name()
        destination = Path(self.build_lib) / "mfsd" / native_library_name()
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)


class PlatformDistribution(Distribution):
    def has_ext_modules(self) -> bool:
        # The wheel contains a platform-specific Rust shared library even
        # though setuptools does not build a conventional Python extension.
        return True


class PlatformWheel(bdist_wheel):
    def finalize_options(self) -> None:
        super().finalize_options()
        self.root_is_pure = False

    def get_tag(self) -> tuple[str, str, str]:
        _, _, platform = super().get_tag()
        # ctypes uses Python's stable C-level calling convention rather than
        # the version-specific CPython extension ABI.
        return "py3", "none", platform


setup(
    cmdclass={"bdist_wheel": PlatformWheel, "build_py": BuildPythonWithRust},
    distclass=PlatformDistribution,
)
