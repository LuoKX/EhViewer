#!/usr/bin/env python3
"""Offline reference-fixture generator; C oracle is never an application dependency.

Build pinned libwebp v1.6.0 as host shared libraries; pass its lib directory.
Files exercise premultiplied blend/dispose, nontransparent ANIM background, offsets,
zero durations, finite loops, VP8L, VP8 and external compressed alpha.
"""
import argparse
import ctypes as C
import hashlib
import json
from pathlib import Path


class Data(C.Structure):
    _fields_ = [("bytes", C.POINTER(C.c_uint8)), ("size", C.c_size_t)]


class Options(C.Structure):
    _fields_ = [("color_mode", C.c_int), ("use_threads", C.c_int), ("padding", C.c_uint32 * 7)]


class Info(C.Structure):
    _fields_ = [(n, C.c_uint32) for n in ("width", "height", "loops", "background", "frames")] + [("padding", C.c_uint32 * 4)]


def chunk(tag, body):
    return tag + len(body).to_bytes(4, "little") + body + bytes(len(body) % 2)


def chunks(blob):
    pos = 12
    while pos < len(blob):
        size = int.from_bytes(blob[pos + 4:pos + 8], "little")
        yield blob[pos:pos + 4], blob[pos + 8:pos + 8 + size]
        pos += 8 + size + size % 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("libdir", type=Path)
    args = parser.parse_args()
    webp = C.CDLL(str(args.libdir / "libwebp.so"), mode=C.RTLD_GLOBAL)
    demux = C.CDLL(str(args.libdir / "libwebpdemux.so"))
    webp.WebPGetDecoderVersion.restype = C.c_int
    assert webp.WebPGetDecoderVersion() == 0x010600
    for name in ("WebPEncodeLosslessRGBA", "WebPEncodeRGBA"):
        fn = getattr(webp, name)
        fn.restype = C.c_size_t
        fn.argtypes = [C.POINTER(C.c_uint8), C.c_int, C.c_int, C.c_int] + ([C.c_float] if name == "WebPEncodeRGBA" else []) + [C.POINTER(C.POINTER(C.c_uint8))]
    webp.WebPFree.argtypes = [C.c_void_p]
    demux.WebPAnimDecoderNewInternal.argtypes = [C.POINTER(Data), C.POINTER(Options), C.c_int]
    demux.WebPAnimDecoderNewInternal.restype = C.c_void_p
    demux.WebPAnimDecoderGetInfo.argtypes = [C.c_void_p, C.POINTER(Info)]
    demux.WebPAnimDecoderGetNext.argtypes = [C.c_void_p, C.POINTER(C.POINTER(C.c_uint8)), C.POINTER(C.c_int)]
    demux.WebPAnimDecoderDelete.argtypes = [C.c_void_p]

    def encode(w, h, pixels, lossy):
        source = (C.c_uint8 * len(pixels)).from_buffer_copy(pixels)
        output = C.POINTER(C.c_uint8)()
        fn = webp.WebPEncodeRGBA if lossy else webp.WebPEncodeLosslessRGBA
        params = [source, w, h, w * 4] + ([C.c_float(85)] if lossy else []) + [C.byref(output)]
        size = fn(*params)
        assert size
        result = C.string_at(output, size)
        webp.WebPFree(output)
        return b"".join(chunk(tag, data) for tag, data in chunks(result) if tag in (b"ALPH", b"VP8 ", b"VP8L"))

    def oracle(encoded):
        source = (C.c_uint8 * len(encoded)).from_buffer_copy(encoded)
        data = Data(source, len(encoded))
        options = Options(7, 1, (C.c_uint32 * 7)())  # MODE_rgbA: premultiplied RGBA.
        decoder = demux.WebPAnimDecoderNewInternal(C.byref(data), C.byref(options), 0x0107)
        assert decoder
        info = Info()
        assert demux.WebPAnimDecoderGetInfo(decoder, C.byref(info))
        frames = []
        for _ in range(info.frames):
            rgba = C.POINTER(C.c_uint8)()
            timestamp = C.c_int()
            assert demux.WebPAnimDecoderGetNext(decoder, C.byref(rgba), C.byref(timestamp))
            pixels = C.string_at(rgba, info.width * info.height * 4)
            frames.append({"timestamp": timestamp.value, "sha256": hashlib.sha256(pixels).hexdigest(), "rgba": pixels})
        demux.WebPAnimDecoderDelete(decoder)
        return info, frames

    root = Path("app/src/main/rust/crates/image/src/webp/test_data")
    root.mkdir(parents=True, exist_ok=True)
    specs = [(0, 0, 6, 4, 13, 2), (2, 0, 2, 2, 37, 1), (0, 2, 4, 2, 0, 2), (0, 0, 6, 4, 29, 0), (4, 2, 2, 2, 7, 3), (2, 0, 2, 4, 19, 0)]
    for lossy in [False, True]:
        name = "lossy-alpha" if lossy else "lossless-blend"
        body = chunk(b"VP8X", b"\x12\0\0\0" + (5).to_bytes(3, "little") + (3).to_bytes(3, "little"))
        body += chunk(b"ANIM", bytes([0x12, 0x34, 0x56, 0xff]) + (3).to_bytes(2, "little"))
        for i, (x, y, w, h, duration, flags) in enumerate(specs):
            pixels = bytes(v for p in range(w * h) for v in ((31 * (p + 1) + i * 17) % 256, (55 * p + i * 29) % 256, (79 * p + i * 13) % 256, [0, 1, 63, 127, 192, 254, 255][(p + i) % 7]))
            header = b"".join(v.to_bytes(3, "little") for v in (x // 2, y // 2, w - 1, h - 1, duration)) + bytes([flags])
            body += chunk(b"ANMF", header + encode(w, h, pixels, lossy))
        encoded = b"RIFF" + (4 + len(body)).to_bytes(4, "little") + b"WEBP" + body
        info, frames = oracle(encoded)
        (root / (name + ".webp")).write_bytes(encoded)
        (root / (name + ".rgba")).write_bytes(b"".join(f["rgba"] for f in frames))
        (root / (name + ".json")).write_text(json.dumps({"width": info.width, "height": info.height, "loops": info.loops, "frames": [{k: v for k, v in f.items() if k != "rgba"} for f in frames]}, indent=2) + "\n")
    original = Path("app/src/main/rust/test_data/animated.webp.hex")
    _, frames = oracle(bytes.fromhex(original.read_text()))
    (root / "animated.sha256").write_text("\n".join(f["sha256"] for f in frames) + "\n")
    print("Pinned C 1.6.0 oracles generated; runtime uses only checked-in results.")


if __name__ == "__main__":
    main()
