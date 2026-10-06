# Pinned Rust codecs

Library source, normalized manifests, upstream README and LICENSE files are kept
locally. Dev targets/dependencies and external test assets are omitted. No registry
cache is patched at build time; Cargo.lock selects these paths deterministically.

| Library | Upstream release | Original .crate SHA-256 |
| --- | --- | --- |
| zip (zip-rs/zip2) | 8.6.0 | 2d04a6b5381502aa6087c94c669499eb1602eb9c5e8198e534de571f7154809b |
| lzma-rust2 (hasenbanck/lzma-rust2) | 0.18.0 | 12931b17820a3a0c48e46359d2be49ef57b5d50509fd5baba43e6c654977aa66 |
| image-webp (image-rs/image-webp) | 0.2.4 | 525e9ff3e1a4be2fbea1fdf0e98686a6d98b4d8f937e1bf7402245af1909e8c3 |

## Local hooks

zip adds opt-in Config.indexed_entries. Numeric internal keys preserve every
physical member and source order, even when names or Unicode-path overrides
collide. Production reading uses by_index; name lookup is not supported in this
mode. Default behavior for application CBZ writing/validation is unchanged.

zip uses lzma-rust2 0.18.0 rather than 0.16.x, and checks PPMd model memory before
creating its decoder. The ppmd feature activates the dependency needed by this
hook. Dictionary checks occur before LZMA/LZMA2 allocation; XZ index checks occur
before reserving the record table. LZ dictionaries and PPMd models retain an
RAII DictionaryReservation until decoder destruction. Optional first-party shared
credit caps simultaneously live outer-filter and inner-ZIP dictionaries/models.

Resource policy lives in first-party eh_codec_limits, not this vendor tree.
The lzma-rust2 limits module is a thin std re-export plus upstream-compatible
no_std no-ops. Bounds are opt-in, nested, per-thread and unwind-safe. Existing
RAR/7z operations keep upstream behavior outside ZIP/filter decoding scopes.
Reservations without an allocation scope carry no shared quota; no_std behavior
remains unchanged.

Neither ZIP zstd nor bzip2-sys features are enabled by eh_archive: codec paths
introduced here use Rust implementations, not replacement C codec wrappers.

image-webp adds an opt-in allocation helper backed by first-party cumulative
charge_allocation. Major static-decoder VP8 partitions/YUV planes, VP8L transform/
entropy/group/Huffman buffers and ALPH buffers are checked before allocation,
then use fallible reserve. VP8 streaming partition input grows under the same
budget. Tiny fixed-size/dimension-bound tables remain upstream; neither encoder
algorithms nor decoded pixels are changed. No budget scope means upstream limits.
Application scopes use a fresh 64-MiB conservative cumulative budget per frame.
This supplements upstream set_memory_limit, which does not cover every allocation.
The application only sends validated static frames to this codec; its own Rust
compositor implements premultiplied blend/dispose instead of upstream animation.

## Updating

Verify the published artifact hash, compare source changes, retain both LICENSE
files, and reapply only these documented hooks. Run production ZIP/CRC/AES/encoding/
duplicate/ZIP64/descriptor tests, independent owner coverage, RAR/7z regression,
WebP C-reference RGBA goldens, alpha/timing/reset and allocation-limit tests,
Android Clippy and ARM64 linking. Do not replace indexed mode with deduplicating
ZipArchive::new or remove pre-allocation guards.
