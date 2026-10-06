# Pinned WebP animation oracles

Generated offline by scripts/fixtures/generate_webp_goldens.py against pinned
libwebp 1.6.0, verified by WebPGetDecoderVersion()==0x010600. Mode MODE_rgbA
(7), threads enabled; no runtime or CI dependency on C reference decoding.

lossless-blend and lossy-alpha contain six frames on a 6x4 canvas, finite loop
count 3, nontransparent ANIM background, offset subrectangles, source replacement,
premultiplied blend, background disposal and one zero-duration later frame.
Timestamps: 13,50,50,79,86,105. RGBA files store concatenated complete canvases.
JSON records SHA-256 and timestamps. Rust tests require byte-exact results.

animated.sha256 stores one SHA-256 for each of the existing 400x400 ten-frame
fixture's complete premultiplied canvases. The input remains at
app/src/main/rust/test_data/animated.webp.hex.

Encoded files are generated test inputs; oracle outputs are not C library source.
Actual ARM64/Android Bitmap copying and performance remain separate device tests.
