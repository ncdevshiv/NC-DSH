from __future__ import annotations

import struct


SOI = b"\xff\xd8"
EOI = b"\xff\xd9"
SOF_MARKERS = {
    0xC0,
    0xC1,
    0xC2,
    0xC3,
    0xC5,
    0xC6,
    0xC7,
    0xC9,
    0xCA,
    0xCB,
    0xCD,
    0xCE,
    0xCF,
}


def jpeg_dimensions(data: bytes) -> tuple[int, int]:
    if not data.startswith(SOI):
        raise ValueError("missing JPEG SOI marker")

    offset = len(SOI)
    while offset < len(data):
        if data[offset] != 0xFF:
            raise ValueError("expected JPEG marker prefix")
        while offset < len(data) and data[offset] == 0xFF:
            offset += 1
        if offset >= len(data):
            break

        marker = data[offset]
        offset += 1
        if marker == 0xD9:
            break
        if marker == 0x01 or 0xD0 <= marker <= 0xD7:
            continue
        if offset + 2 > len(data):
            raise ValueError("truncated JPEG segment length")
        segment_length = struct.unpack_from(">H", data, offset)[0]
        if segment_length < 2 or offset + segment_length > len(data):
            raise ValueError("invalid JPEG segment length")
        if marker in SOF_MARKERS:
            if segment_length < 7:
                raise ValueError("truncated JPEG SOF segment")
            height, width = struct.unpack_from(">HH", data, offset + 3)
            if width <= 0 or height <= 0:
                raise ValueError(f"invalid JPEG dimensions {width}x{height}")
            return width, height
        if marker == 0xDA:
            break
        offset += segment_length

    raise ValueError("JPEG is missing a supported SOF marker")
