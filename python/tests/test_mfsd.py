from __future__ import annotations

import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import mfsd


TRIANGLE_OBJ = b"o triangle\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"


class DocumentTests(unittest.TestCase):
    def test_document_must_be_deserialized(self) -> None:
        with self.assertRaisesRegex(TypeError, "Document.deserialize"):
            mfsd.Document()

    def test_obj_to_omf_round_trip(self) -> None:
        with mfsd.Document.deserialize(mfsd.Format.OBJ, TRIANGLE_OBJ) as document:
            self.assertEqual(len(document.elements), 1)
            streams = document.serialize(mfsd.Format.OMF)

        self.assertEqual(len(streams), 1)
        self.assertGreater(len(streams[0]), 0)

        with mfsd.Document.deserialize(mfsd.Format.OMF, streams[0]) as round_trip:
            self.assertEqual(len(round_trip.elements), 1)

    def test_accepts_bytearray_and_memoryview(self) -> None:
        for data in (bytearray(TRIANGLE_OBJ), memoryview(TRIANGLE_OBJ)):
            with self.subTest(type=type(data).__name__):
                with mfsd.Document.deserialize(mfsd.Format.OBJ, data) as document:
                    self.assertEqual(len(document.elements), 1)

    def test_close_is_idempotent_and_prevents_use(self) -> None:
        document = mfsd.Document.deserialize(mfsd.Format.OBJ, TRIANGLE_OBJ)
        elements = document.elements
        document.close()
        document.close()

        with self.assertRaisesRegex(ValueError, "closed"):
            len(elements)
        with self.assertRaisesRegex(ValueError, "closed"):
            document.serialize(mfsd.Format.OMF)

    def test_conversion_error_includes_status_and_native_message(self) -> None:
        with self.assertRaises(mfsd.MfsdError) as raised:
            mfsd.Document.deserialize(mfsd.Format.OBJ, b"not an obj file")

        self.assertEqual(raised.exception.status_code, 2)
        self.assertTrue(str(raised.exception))

    def test_rejects_unknown_format(self) -> None:
        with self.assertRaisesRegex(ValueError, "Unsupported MFSD format"):
            mfsd.Document.deserialize(99, TRIANGLE_OBJ)  # type: ignore[arg-type]

    def test_native_version_matches_package(self) -> None:
        self.assertEqual(mfsd.version(), (0, 1, 0))
        self.assertEqual(mfsd.__version__, "0.1.0")


if __name__ == "__main__":
    unittest.main()
