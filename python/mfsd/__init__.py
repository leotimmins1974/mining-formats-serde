"""Python bindings for Mining Formats Serialise Deserialise."""

from __future__ import annotations

import ctypes
from enum import IntEnum

from ._native import library


class Format(IntEnum):
    """A file format supported by MFSD."""

    OMF = 0
    OBJ = 1
    LAS = 2
    LAZ = 3
    TIFF = 4


class MfsdError(Exception):
    """A native MFSD validation or conversion failure."""

    def __init__(self, status_code: int, message: str) -> None:
        super().__init__(message)
        self.status_code = status_code


def _check(status: int) -> None:
    if status == 0:
        return
    raw_message = library.mfsd_last_error_message()
    message = raw_message.decode("utf-8", errors="replace") if raw_message else ""
    if not message.strip():
        message = f"MFSD native call failed with status {status}."
    raise MfsdError(status, message)


def _format_value(file_format: Format) -> int:
    try:
        return int(Format(file_format))
    except (TypeError, ValueError) as error:
        raise ValueError(f"Unsupported MFSD format: {file_format!r}") from error


def version() -> tuple[int, int, int]:
    """Return the native MFSD library version."""

    native = library.version()
    return native.major, native.minor, native.patch


class Document:
    """An owned, in-memory OMF document used for format conversion."""

    _handle: ctypes.c_void_p | None

    def __init__(self) -> None:
        raise TypeError("Documents must be created with Document.deserialize()")

    @classmethod
    def _from_handle(cls, handle: ctypes.c_void_p) -> Document:
        document = object.__new__(cls)
        document._handle = handle
        return document

    @classmethod
    def deserialize(
        cls,
        file_format: Format,
        data: bytes | bytearray | memoryview,
    ) -> Document:
        """Deserialize a complete file held in memory."""

        try:
            input_bytes = memoryview(data).cast("B").tobytes()
        except (TypeError, ValueError) as error:
            raise TypeError("data must be a bytes-like object") from error

        input_buffer = ctypes.create_string_buffer(input_bytes) if input_bytes else None
        input_pointer = ctypes.cast(input_buffer, ctypes.c_void_p) if input_buffer else None
        handle = ctypes.c_void_p()
        status = library.mfsd_document_deserialize(
            _format_value(file_format),
            input_pointer,
            len(input_bytes),
            ctypes.byref(handle),
        )
        _check(status)
        if not handle.value:
            raise MfsdError(3, "MFSD returned a null document after a successful native call.")
        return cls._from_handle(handle)

    @property
    def elements(self) -> ElementCollection:
        """The top-level elements in the document."""

        self._require_open()
        return ElementCollection(self)

    def serialize(self, file_format: Format) -> list[bytes]:
        """Serialize the document into independently usable byte strings."""

        handle = self._require_open()
        buffers = ctypes.c_void_p()
        status = library.mfsd_document_serialize(
            handle,
            _format_value(file_format),
            ctypes.byref(buffers),
        )
        _check(status)
        if not buffers.value:
            raise MfsdError(3, "MFSD returned null buffers after a successful native call.")

        try:
            count = ctypes.c_size_t()
            _check(library.mfsd_buffers_count(buffers, ctypes.byref(count)))
            output: list[bytes] = []
            for index in range(count.value):
                pointer = ctypes.c_void_p()
                length = ctypes.c_size_t()
                _check(
                    library.mfsd_buffers_get(
                        buffers,
                        index,
                        ctypes.byref(pointer),
                        ctypes.byref(length),
                    )
                )
                output.append(ctypes.string_at(pointer, length.value) if length.value else b"")
            return output
        finally:
            library.mfsd_buffers_free(buffers)

    def close(self) -> None:
        """Release the native document. Calling this more than once is safe."""

        if self._handle is not None:
            library.mfsd_document_free(self._handle)
            self._handle = None

    def _require_open(self) -> ctypes.c_void_p:
        if self._handle is None:
            raise ValueError("Document is closed")
        return self._handle

    def __enter__(self) -> Document:
        self._require_open()
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            self.close()
        except Exception:
            # Interpreter shutdown can tear down ctypes globals first.
            pass


class ElementCollection:
    """The top-level elements owned by an MFSD document.

    Element inspection is not exposed by the current stable ABI; the
    collection currently provides its length only.
    """

    def __init__(self, document: Document) -> None:
        self._document = document

    def __len__(self) -> int:
        count = ctypes.c_size_t()
        _check(
            library.mfsd_document_element_count(
                self._document._require_open(),
                ctypes.byref(count),
            )
        )
        return count.value


__version__ = ".".join(map(str, version()))

__all__ = [
    "Document",
    "ElementCollection",
    "Format",
    "MfsdError",
    "version",
]
