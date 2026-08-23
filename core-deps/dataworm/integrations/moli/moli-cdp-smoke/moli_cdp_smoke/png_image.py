from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


@dataclass(frozen=True)
class DecodedPng:
    width: int
    height: int
    rgba: bytes

    def pixel(self, x: int, y: int) -> tuple[int, int, int, int]:
        if not 0 <= x < self.width or not 0 <= y < self.height:
            raise ValueError(f"pixel ({x}, {y}) is outside {self.width}x{self.height}")
        offset = (y * self.width + x) * 4
        red, green, blue, alpha = self.rgba[offset : offset + 4]
        return red, green, blue, alpha

    def distinct_color_count(self) -> int:
        return len(
            {
                self.rgba[offset : offset + 4]
                for offset in range(0, len(self.rgba), 4)
            }
        )


def decode_png(data: bytes) -> DecodedPng:
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("missing PNG signature")

    width = height = bit_depth = color_type = interlace = None
    compressed = bytearray()
    offset = len(PNG_SIGNATURE)
    saw_iend = False
    while offset < len(data):
        if offset + 12 > len(data):
            raise ValueError("truncated PNG chunk header")
        length = struct.unpack_from(">I", data, offset)[0]
        chunk_type = data[offset + 4 : offset + 8]
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            raise ValueError("truncated PNG chunk payload")
        payload = data[offset + 8 : offset + 8 + length]
        expected_crc = struct.unpack_from(">I", data, offset + 8 + length)[0]
        actual_crc = zlib.crc32(chunk_type)
        actual_crc = zlib.crc32(payload, actual_crc) & 0xFFFFFFFF
        if actual_crc != expected_crc:
            raise ValueError(f"invalid {chunk_type!r} PNG chunk CRC")

        if chunk_type == b"IHDR":
            if length != 13 or width is not None:
                raise ValueError("invalid PNG IHDR")
            width, height, bit_depth, color_type, compression, filtering, interlace = (
                struct.unpack(">IIBBBBB", payload)
            )
            if compression != 0 or filtering != 0:
                raise ValueError("unsupported PNG compression or filter method")
        elif chunk_type == b"IDAT":
            compressed.extend(payload)
        elif chunk_type == b"IEND":
            saw_iend = True
            break
        offset = chunk_end

    if width is None or height is None or not saw_iend:
        raise ValueError("PNG is missing IHDR or IEND")
    if width <= 0 or height <= 0:
        raise ValueError(f"invalid PNG dimensions {width}x{height}")
    if bit_depth != 8 or color_type not in (2, 6) or interlace != 0:
        raise ValueError(
            "screenshot smoke supports non-interlaced 8-bit RGB/RGBA PNGs only; "
            f"got bitDepth={bit_depth}, colorType={color_type}, interlace={interlace}"
        )

    bytes_per_pixel = 3 if color_type == 2 else 4
    stride = width * bytes_per_pixel
    filtered = zlib.decompress(bytes(compressed))
    expected_size = (stride + 1) * height
    if len(filtered) != expected_size:
        raise ValueError(
            f"unexpected PNG scanline size: expected {expected_size}, got {len(filtered)}"
        )

    unfiltered = bytearray()
    previous = bytearray(stride)
    cursor = 0
    for _ in range(height):
        filter_type = filtered[cursor]
        cursor += 1
        source = filtered[cursor : cursor + stride]
        cursor += stride
        row = _unfilter_row(filter_type, source, previous, bytes_per_pixel)
        unfiltered.extend(row)
        previous = row

    if color_type == 6:
        rgba = bytes(unfiltered)
    else:
        rgba_buffer = bytearray(width * height * 4)
        output = 0
        for source in range(0, len(unfiltered), 3):
            rgba_buffer[output : output + 3] = unfiltered[source : source + 3]
            rgba_buffer[output + 3] = 255
            output += 4
        rgba = bytes(rgba_buffer)
    return DecodedPng(width=width, height=height, rgba=rgba)


def _unfilter_row(
    filter_type: int,
    source: bytes,
    previous: bytearray,
    bytes_per_pixel: int,
) -> bytearray:
    row = bytearray(len(source))
    for index, value in enumerate(source):
        left = row[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
        up = previous[index]
        upper_left = previous[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
        if filter_type == 0:
            predictor = 0
        elif filter_type == 1:
            predictor = left
        elif filter_type == 2:
            predictor = up
        elif filter_type == 3:
            predictor = (left + up) // 2
        elif filter_type == 4:
            predictor = _paeth(left, up, upper_left)
        else:
            raise ValueError(f"unsupported PNG row filter {filter_type}")
        row[index] = (value + predictor) & 0xFF
    return row


def _paeth(left: int, up: int, upper_left: int) -> int:
    estimate = left + up - upper_left
    left_distance = abs(estimate - left)
    up_distance = abs(estimate - up)
    upper_left_distance = abs(estimate - upper_left)
    if left_distance <= up_distance and left_distance <= upper_left_distance:
        return left
    if up_distance <= upper_left_distance:
        return up
    return upper_left
