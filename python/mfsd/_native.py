from __future__ import annotations

import ctypes
import ctypes.util
import os
import sys
from pathlib import Path


class NativeVersion(ctypes.Structure):
    _fields_ = [
        ("major", ctypes.c_uint32),
        ("minor", ctypes.c_uint32),
        ("patch", ctypes.c_uint32),
    ]


def _library_name() -> str:
    if sys.platform == "win32":
        return "mfsd.dll"
    if sys.platform == "darwin":
        return "libmfsd.dylib"
    if sys.platform.startswith("linux"):
        return "libmfsd.so"
    raise ImportError(f"MFSD does not support Python on {sys.platform!r}")


def _library_candidates() -> list[str]:
    configured = os.environ.get("MFSD_LIBRARY_PATH")
    if configured:
        return [configured]

    name = _library_name()
    package_directory = Path(__file__).resolve().parent
    repository = package_directory.parents[1]
    candidates = [
        str(package_directory / name),
        str(repository / "target" / "debug" / name),
        str(repository / "target" / "release" / name),
    ]
    installed = ctypes.util.find_library("mfsd")
    if installed:
        candidates.append(installed)
    return candidates


def _load_library() -> ctypes.CDLL:
    errors: list[str] = []
    for candidate in _library_candidates():
        try:
            loaded = ctypes.CDLL(candidate)
            _configure(loaded)
            return loaded
        except (AttributeError, OSError) as error:
            errors.append(f"{candidate}: {error}")

    detail = "\n".join(errors)
    raise ImportError(
        "Could not load the MFSD native library. Install the package with "
        "`python -m pip install .`, run `cargo build`, or set "
        f"MFSD_LIBRARY_PATH to the shared library.\n{detail}"
    )


def _configure(library: ctypes.CDLL) -> None:
    library.version.argtypes = []
    library.version.restype = NativeVersion

    library.mfsd_last_error_message.argtypes = []
    library.mfsd_last_error_message.restype = ctypes.c_char_p

    library.mfsd_document_deserialize.argtypes = [
        ctypes.c_uint32,
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.POINTER(ctypes.c_void_p),
    ]
    library.mfsd_document_deserialize.restype = ctypes.c_int

    library.mfsd_document_element_count.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_size_t),
    ]
    library.mfsd_document_element_count.restype = ctypes.c_int

    library.mfsd_document_serialize.argtypes = [
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.POINTER(ctypes.c_void_p),
    ]
    library.mfsd_document_serialize.restype = ctypes.c_int

    library.mfsd_buffers_count.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_size_t),
    ]
    library.mfsd_buffers_count.restype = ctypes.c_int

    library.mfsd_buffers_get.argtypes = [
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(ctypes.c_size_t),
    ]
    library.mfsd_buffers_get.restype = ctypes.c_int

    library.mfsd_document_free.argtypes = [ctypes.c_void_p]
    library.mfsd_document_free.restype = None

    library.mfsd_buffers_free.argtypes = [ctypes.c_void_p]
    library.mfsd_buffers_free.restype = None


library = _load_library()
