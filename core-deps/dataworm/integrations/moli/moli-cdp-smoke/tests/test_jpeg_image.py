from __future__ import annotations

import unittest

from moli_cdp_smoke.jpeg_image import jpeg_dimensions


class JpegDimensionsTests(unittest.TestCase):
    def test_reads_baseline_sof(self) -> None:
        data = (
            b"\xff\xd8"
            b"\xff\xe0\x00\x04ab"
            b"\xff\xc0\x00\x11\x08\x01\x2c\x01\x90\x03"
            b"\x01\x11\x00\x02\x11\x00\x03\x11\x00"
            b"\xff\xd9"
        )

        self.assertEqual(jpeg_dimensions(data), (400, 300))

    def test_rejects_invalid_or_dimensionless_data(self) -> None:
        for data in [
            b"not-a-jpeg",
            b"\xff\xd8\xff\xe0\x00",
            b"\xff\xd8\xff\xda\x00\x02\xff\xd9",
        ]:
            with self.subTest(data=data), self.assertRaises(ValueError):
                jpeg_dimensions(data)


if __name__ == "__main__":
    unittest.main()
