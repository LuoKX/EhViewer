//! Pure-Rust WebP frame decoding and libwebp-compatible premultiplied animation.
//! The third-party codec only sees bounded static frames, never an animation canvas.

use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::ops::Range;

use anyhow::{Context as _, Result, bail, ensure};
use eh_codec_limits::AllocationBudget;
use image_webp::WebPDecoder;

/// Maximum owned encoded input, including ignored metadata chunks.
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
/// Maximum RGBA canvas or frame size (8 megapixels).
pub const MAX_CANVAS_BYTES: usize = 32 * 1024 * 1024;
/// Maximum animation frame/chunk metadata records.
pub const MAX_FRAMES: usize = 100_000;
/// Conservative cumulative third-party decode allocation budget, excluding caller buffers.
pub const MAX_CODEC_BYTES: usize = 64 * 1024 * 1024;

/// Metadata packed by the unchanged JNI boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageInfo {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Original WebP loop count; zero means infinite.
    pub loop_count: u32,
    /// Frames in one loop.
    pub frame_count: u32,
}

#[derive(Clone, Debug)]
struct Frame {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    duration: u32,
    blend: bool,
    dispose: bool,
    payload: Range<usize>,
}

/// Owning animation, with one premultiplied canvas and one reusable decoded raster.
pub struct AnimationDecoder {
    data: Vec<u8>,
    info: ImageInfo,
    frames: Vec<Frame>,
    canvas: Vec<u8>,
    raster: Vec<u8>,
    next: usize,
    timestamp: i32,
}

impl AnimationDecoder {
    /// Validate RIFF/frame headers and resource bounds before retaining input/canvases.
    ///
    /// # Errors
    /// Rejects malformed containers, mismatched codec dimensions and resource limits.
    pub fn new(data: &[u8]) -> Result<Self> {
        ensure!(!data.is_empty(), "WebP input is empty");
        ensure!(
            data.len() <= MAX_INPUT_BYTES,
            "WebP input exceeds memory limit"
        );
        let (info, frames) = index(data)?;
        let size = frame_length(info.width, info.height)?;
        let mut canvas = Vec::new();
        canvas
            .try_reserve_exact(size)
            .context("WebP canvas allocation failed")?;
        canvas.resize(size, 0);
        Ok(Self {
            data: data.to_vec(),
            info,
            frames,
            canvas,
            raster: Vec::new(),
            next: 0,
            timestamp: 0,
        })
    }

    /// Return checked metadata without decoding a frame.
    #[must_use]
    pub fn info(&self) -> ImageInfo {
        self.info
    }

    /// Decode the next frame; restart on explicit reset or exhaustion.
    /// Callback failure keeps the consumed frame's state, matching the previous JNI contract.
    ///
    /// # Errors
    /// Returns codec, memory, timestamp or callback errors without fallback.
    pub fn next_frame<R>(
        &mut self,
        reset: bool,
        consume: impl FnOnce(&[u8], ImageInfo, i32) -> Result<R>,
    ) -> Result<R> {
        if reset || self.next == self.frames.len() {
            self.canvas.fill(0);
            self.next = 0;
            self.timestamp = 0;
        }
        let frame = &self.frames[self.next];
        let size = frame_length(frame.width, frame.height)?;
        if self.raster.capacity() < size {
            self.raster
                .try_reserve_exact(size.saturating_sub(self.raster.len()))
                .context("WebP raster allocation failed")?;
        }
        self.raster.resize(size, 0);
        let payload = &self.data[frame.payload.clone()];
        AllocationBudget::new(MAX_CODEC_BYTES).run(|| {
            let source = FrameSource::new(payload, frame.width, frame.height)?;
            let mut decoder = WebPDecoder::new(BufReader::new(source))
                .context("Rust WebP codec rejected frame")?;
            ensure!(
                decoder.dimensions() == (frame.width, frame.height),
                "WebP codec dimensions changed"
            );
            let channels = if decoder.has_alpha() { 4 } else { 3 };
            let decoded_size = size / 4 * channels;
            ensure!(
                decoder.output_buffer_size() == Some(decoded_size),
                "WebP codec output size mismatch"
            );
            decoder.set_memory_limit(MAX_CODEC_BYTES);
            decoder
                .read_image(&mut self.raster[..decoded_size])
                .context("Rust WebP frame decode failed")?;
            if channels == 3 {
                for pixel in (0..size / 4).rev() {
                    let rgb = [
                        self.raster[pixel * 3],
                        self.raster[pixel * 3 + 1],
                        self.raster[pixel * 3 + 2],
                    ];
                    self.raster[pixel * 4..pixel * 4 + 3].copy_from_slice(&rgb);
                    self.raster[pixel * 4 + 3] = 255;
                }
            }
            Ok(())
        })?;
        if self.next > 0 && self.frames[self.next - 1].dispose {
            clear_rect(
                &mut self.canvas,
                self.info.width,
                &self.frames[self.next - 1],
            );
        }
        composite(
            &mut self.canvas,
            self.info.width,
            &self.raster,
            frame,
            self.next > 0,
        );
        self.timestamp = self
            .timestamp
            .checked_add(i32::try_from(frame.duration)?)
            .context("WebP timestamp exceeds JNI range")?;
        self.next += 1;
        ensure!(self.timestamp > 0, "WebP frame timestamp must be positive");
        consume(&self.canvas, self.info, self.timestamp)
    }
}

/// Checked RGBA buffer length with an application-level pixel bound.
///
/// # Errors
/// Rejects zero, arithmetic overflow and oversized dimensions before allocating.
pub fn frame_length(width: u32, height: u32) -> Result<usize> {
    ensure!(width > 0 && height > 0, "WebP dimensions must be positive");
    let bytes = usize::try_from(width)?
        .checked_mul(usize::try_from(height)?)
        .and_then(|size| size.checked_mul(4))
        .context("WebP frame size overflow")?;
    ensure!(
        bytes <= MAX_CANVAS_BYTES,
        "WebP canvas exceeds memory limit"
    );
    Ok(bytes)
}

fn index(data: &[u8]) -> Result<(ImageInfo, Vec<Frame>)> {
    ensure!(
        data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP",
        "WebP RIFF header is invalid"
    );
    let end = 8_usize
        .checked_add(u32::from_le_bytes(data[4..8].try_into()?) as usize)
        .context("WebP RIFF length overflow")?;
    ensure!(
        end >= 12 && end <= data.len(),
        "WebP RIFF data is truncated"
    );
    let chunks = chunks(data, 12..end)?;
    let mut canvas = None;
    let mut animated = false;
    let mut animation = None;
    let mut frames = Vec::new();
    let mut static_payload = None;
    for chunk in chunks {
        match &chunk.tag {
            b"VP8X" => {
                ensure!(
                    canvas.is_none() && chunk.body.len() >= 10 && frames.is_empty(),
                    "WebP extended header is invalid"
                );
                let body = &data[chunk.body];
                let width = read24(&body[4..7]) + 1;
                let height = read24(&body[7..10]) + 1;
                frame_length(width, height)?;
                canvas = Some((width, height));
                animated = body[0] & 2 != 0;
            }
            b"ANIM" => {
                ensure!(
                    animated && animation.is_none() && chunk.body.len() >= 6 && frames.is_empty(),
                    "WebP animation header is invalid"
                );
                animation = Some(u32::from(u16::from_le_bytes(
                    data[chunk.body.start + 4..chunk.body.start + 6].try_into()?,
                )));
            }
            b"ANMF" => {
                ensure!(
                    animated
                        && animation.is_some()
                        && frames.len() < MAX_FRAMES
                        && chunk.body.len() >= 16,
                    "WebP frame header is invalid"
                );
                let frame =
                    parse_frame(data, chunk.body, canvas.context("WebP canvas is missing")?)?;
                frames.push(frame);
            }
            b"VP8 " | b"VP8L" | b"ALPH" => {
                ensure!(!animated, "WebP animated image has a top-level raster");
                let start = static_payload.unwrap_or(chunk.body.start - 8);
                static_payload = Some(start);
            }
            _ => {}
        }
    }
    let (width, height, loops) = if animated {
        ensure!(!frames.is_empty(), "WebP animation has no frames");
        let (width, height) = canvas.context("WebP canvas is missing")?;
        (
            width,
            height,
            animation.context("WebP animation is missing")?,
        )
    } else {
        let (width, height) = index_static(
            data,
            static_payload.context("WebP raster is missing")?..end,
            canvas,
            &mut frames,
        )?;
        (width, height, 1)
    };
    frame_length(width, height)?;
    let total = frames
        .iter()
        .try_fold(0_u32, |total, frame| total.checked_add(frame.duration))
        .context("WebP animation duration overflow")?;
    ensure!(
        i32::try_from(total).is_ok(),
        "WebP animation duration exceeds JNI range"
    );
    Ok((
        ImageInfo {
            width,
            height,
            loop_count: loops,
            frame_count: u32::try_from(frames.len())?,
        },
        frames,
    ))
}

fn index_static(
    data: &[u8],
    payload: Range<usize>,
    canvas: Option<(u32, u32)>,
    frames: &mut Vec<Frame>,
) -> Result<(u32, u32)> {
    let start = payload.start;
    let end = payload.end;
    let (width, height) = payload_dimensions(data, start..end)?;
    ensure!(
        canvas.is_none_or(|size| size == (width, height)),
        "WebP canvas and raster dimensions differ"
    );
    validate_payload(data, start..end, width, height)?;
    frames.push(Frame {
        x: 0,
        y: 0,
        width,
        height,
        duration: 0,
        blend: false,
        dispose: false,
        payload: start..end,
    });
    Ok((width, height))
}

fn parse_frame(data: &[u8], body: Range<usize>, canvas: (u32, u32)) -> Result<Frame> {
    let header = &data[body.start..body.start + 16];
    let frame = Frame {
        x: read24(&header[..3]) * 2,
        y: read24(&header[3..6]) * 2,
        width: read24(&header[6..9]) + 1,
        height: read24(&header[9..12]) + 1,
        duration: read24(&header[12..15]),
        dispose: header[15] & 1 != 0,
        blend: header[15] & 2 == 0,
        payload: body.start + 16..body.end,
    };
    let (width, height) = canvas;
    ensure!(
        frame
            .x
            .checked_add(frame.width)
            .is_some_and(|right| right <= width)
            && frame
                .y
                .checked_add(frame.height)
                .is_some_and(|bottom| bottom <= height),
        "WebP frame exceeds canvas"
    );
    validate_payload(data, frame.payload.clone(), frame.width, frame.height)?;
    Ok(frame)
}

struct Chunk {
    tag: [u8; 4],
    body: Range<usize>,
}
fn chunks(data: &[u8], range: Range<usize>) -> Result<Vec<Chunk>> {
    let mut result = Vec::new();
    let mut offset = range.start;
    while offset < range.end {
        ensure!(
            result.len() < MAX_FRAMES && range.end - offset >= 8,
            "WebP chunk header is truncated or excessive"
        );
        let size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into()?) as usize;
        let end = offset
            .checked_add(8)
            .and_then(|start| start.checked_add(size))
            .context("WebP chunk size overflow")?;
        let next = end
            .checked_add(size % 2)
            .context("WebP chunk padding overflow")?;
        ensure!(next <= range.end, "WebP chunk is truncated");
        result.push(Chunk {
            tag: data[offset..offset + 4].try_into()?,
            body: offset + 8..end,
        });
        offset = next;
    }
    Ok(result)
}

fn validate_payload(data: &[u8], range: Range<usize>, width: u32, height: u32) -> Result<()> {
    frame_length(width, height)?;
    let mut raster = false;
    let mut alpha = false;
    for chunk in chunks(data, range)? {
        match &chunk.tag {
            b"VP8 " | b"VP8L" => {
                ensure!(!raster, "WebP frame contains multiple rasters");
                ensure!(
                    chunk.tag != *b"VP8L" || !alpha,
                    "WebP lossless frame has an external alpha chunk"
                );
                let dimensions = codec_dimensions(chunk.tag, &data[chunk.body])?;
                ensure!(
                    dimensions == (width, height),
                    "WebP frame and codec dimensions differ"
                );
                raster = true;
            }
            b"ALPH" => {
                ensure!(
                    !alpha && !raster && !chunk.body.is_empty(),
                    "WebP alpha chunk order is invalid"
                );
                alpha = true;
            }
            b"ANIM" | b"ANMF" | b"VP8X" => bail!("WebP frame has nested animation metadata"),
            _ => {}
        }
    }
    ensure!(raster, "WebP frame raster is missing");
    Ok(())
}
fn payload_dimensions(data: &[u8], range: Range<usize>) -> Result<(u32, u32)> {
    for chunk in chunks(data, range)? {
        if matches!(&chunk.tag, b"VP8 " | b"VP8L") {
            return codec_dimensions(chunk.tag, &data[chunk.body]);
        }
    }
    bail!("WebP raster is missing")
}
fn codec_dimensions(tag: [u8; 4], body: &[u8]) -> Result<(u32, u32)> {
    if tag == *b"VP8L" {
        ensure!(
            body.len() >= 5 && body[0] == 0x2f,
            "WebP lossless header is invalid"
        );
        let bits = u32::from_le_bytes(body[1..5].try_into()?);
        ensure!(bits >> 29 == 0, "WebP lossless version is unsupported");
        Ok(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
    } else {
        ensure!(
            body.len() >= 10 && body[0] & 1 == 0 && body[3..6] == [0x9d, 0x01, 0x2a],
            "WebP lossy header is invalid"
        );
        Ok((
            u32::from(u16::from_le_bytes(body[6..8].try_into()?) & 0x3fff),
            u32::from(u16::from_le_bytes(body[8..10].try_into()?) & 0x3fff),
        ))
    }
}
fn read24(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

fn clear_rect(canvas: &mut [u8], width: u32, frame: &Frame) {
    for row in frame.y..frame.y + frame.height {
        let start = (row as usize * width as usize + frame.x as usize) * 4;
        canvas[start..start + frame.width as usize * 4].fill(0);
    }
}
fn composite(canvas: &mut [u8], width: u32, raster: &[u8], frame: &Frame, has_previous: bool) {
    for row in 0..frame.height as usize {
        let target = ((row + frame.y as usize) * width as usize + frame.x as usize) * 4;
        let source = row * frame.width as usize * 4;
        for (src, dst) in raster[source..source + frame.width as usize * 4]
            .chunks_exact(4)
            .zip(canvas[target..target + frame.width as usize * 4].chunks_exact_mut(4))
        {
            let a = u32::from(src[3]);
            for channel in 0..3 {
                let premultiplied = u32::from(src[channel]) * a / 255;
                let background = if frame.blend && has_previous {
                    u32::from(dst[channel]) * (256 - a) / 256
                } else {
                    0
                };
                dst[channel] = u8::try_from(premultiplied + background)
                    .expect("premultiplied channel is bounded");
            }
            let background = if frame.blend && has_previous {
                u32::from(dst[3]) * (256 - a) / 256
            } else {
                0
            };
            dst[3] = u8::try_from(a + background).expect("premultiplied alpha is bounded");
        }
    }
}

struct FrameSource<'a> {
    header: [u8; 30],
    payload: &'a [u8],
    position: u64,
}
impl<'a> FrameSource<'a> {
    fn new(payload: &'a [u8], width: u32, height: u32) -> Result<Self> {
        let mut header = [0; 30];
        header[..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&u32::try_from(22 + payload.len())?.to_le_bytes());
        header[8..16].copy_from_slice(b"WEBPVP8X");
        header[16] = 10;
        header[20] = if chunks(payload, 0..payload.len())?
            .iter()
            .any(|chunk| matches!(&chunk.tag, b"ALPH" | b"VP8L"))
        {
            0x10
        } else {
            0
        };
        header[24..27].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
        header[27..30].copy_from_slice(&(height - 1).to_le_bytes()[..3]);
        Ok(Self {
            header,
            payload,
            position: 0,
        })
    }
}
impl Read for FrameSource<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let position = usize::try_from(self.position).map_err(|_| io::ErrorKind::InvalidInput)?;
        let bytes = if position < self.header.len() {
            &self.header[position..]
        } else {
            self.payload
                .get(position - self.header.len()..)
                .unwrap_or_default()
        };
        let count = buffer.len().min(bytes.len());
        buffer[..count].copy_from_slice(&bytes[..count]);
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for FrameSource<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let length = (self.header.len() + self.payload.len()) as u64;
        let next = match from {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(n) => self.position.checked_add_signed(n),
            SeekFrom::End(n) => length.checked_add_signed(n),
        };
        self.position = next
            .filter(|n| *n <= length)
            .ok_or(io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests;
