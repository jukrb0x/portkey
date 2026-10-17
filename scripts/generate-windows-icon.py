"""Deterministic rasterization of the repository-owned paired-arrow vector mark."""
from pathlib import Path
import math
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1]
SEGMENTS = [((16, 23), (47, 23)), ((38, 14), (47, 23)), ((47, 23), (38, 32)),
            ((48, 41), (17, 41)), ((26, 32), (17, 41)), ((17, 41), (26, 50))]


def distance(x, y, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    t = max(0, min(1, ((x-a[0])*dx+(y-a[1])*dy)/(dx*dx+dy*dy)))
    return math.hypot(x-a[0]-t*dx, y-a[1]-t*dy)


def png(size):
    rows = bytearray()
    for row in range(size):
        rows.append(0)
        for col in range(size):
            rgba = [0, 0, 0, 0]
            for sy in range(4):
                for sx in range(4):
                    x, y = (col+(sx+.5)/4)*64/size, (row+(sy+.5)/4)*64/size
                    if math.hypot(max(15-x, 0, x-49), max(15-y, 0, y-49)) > 14:
                        continue
                    color = (255, 255, 255) if min(distance(x, y, a, b) for a, b in SEGMENTS) <= 2.25 else (34, 104, 172)
                    for c in range(3):
                        rgba[c] += color[c]
                    rgba[3] += 255
            alpha = rgba[3] // 16
            count = rgba[3] / 255
            rows.extend([round(rgba[c]/count) if count else 0 for c in range(3)] + [alpha])
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind+payload))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')


sizes = [16, 24, 32, 48, 64, 128, 256]
images = [png(size) for size in sizes]
offset = 6 + 16*len(sizes)
entries = []
for size, data in zip(sizes, images):
    entries.append(struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
    offset += len(data)
output = ROOT / 'native/windows/Assets/PortKey.ico'
output.write_bytes(struct.pack('<HHH', 0, 1, len(images)) + b''.join(entries) + b''.join(images))
assert output.read_bytes()[:6] == b'\0\0\1\0\7\0'
print(output)
