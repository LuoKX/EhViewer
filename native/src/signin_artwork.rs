//! Original sign-in vector geometry, generated from the reviewed Android asset.
//!
//! Geometry and paint order preserve original paths, colors, round caps and
//! viewport/group transforms. This does not establish Android-versus-egui
//! antialiasing or pixel equality; those remain screenshot/device gates.

use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2};

#[path = "signin_artwork_data.rs"]
mod data;

pub(super) struct SourceStroke {
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
    color: [u8; 3],
}

pub(super) enum SourceCommand {
    Move([f32; 2]),
    Line([f32; 2]),
    Cubic([[f32; 2]; 3]),
    Close,
}

/// Original opaque round-capped stroke after viewport/group transformation.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtworkStroke {
    /// Original path start in logical image coordinates.
    pub from: Pos2,
    /// Original path end in logical image coordinates.
    pub to: Pos2,
    /// Transformed Android stroke width, not a default widget border.
    pub width: f32,
    /// Original RGB value.
    pub color: Color32,
}

/// Ready-to-paint original asset geometry, never a placeholder icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Artwork {
    /// Original ordered stroke paths.
    pub strokes: Vec<ArtworkStroke>,
    /// Single convex closed contour in original final-layer draw order.
    pub fill: Vec<Pos2>,
    /// Original final-path fill color.
    pub fill_color: Color32,
    /// Source viewport fit inside caller's measured Image bounds.
    pub viewport: Rect,
}

/// Invalid, empty or unsupported image allocation; no approximate fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidArtworkBounds;

/// Reviewed original XML content identity used by generator and source gate.
#[must_use]
pub const fn source_sha256() -> &'static str {
    data::SOURCE_SHA256
}

/// Android vector's intrinsic sign-in image size, in logical dp.
#[must_use]
pub const fn intrinsic_size() -> Vec2 {
    Vec2::new(data::VIEWPORT[0], data::VIEWPORT[1])
}

/// Fit original vector to measured image bounds using centered ContentScale.Fit.
///
/// # Errors
/// Rejects nonfinite/empty/inverted or excessive allocations before generating
/// points. No default icon or guessed layout is substituted.
pub fn geometry(bounds: Rect) -> Result<Artwork, InvalidArtworkBounds> {
    let size = bounds.size();
    if !bounds.is_finite() || size.x <= 0.0 || size.y <= 0.0 || size.max_elem() > 16_384.0 {
        return Err(InvalidArtworkBounds);
    }
    let image_scale = (size.x / data::VIEWPORT[0]).min(size.y / data::VIEWPORT[1]);
    let fitted_size = intrinsic_size() * image_scale;
    let fitted_min = bounds.min + (size - fitted_size) * 0.5;
    let viewport = Rect::from_min_size(fitted_min, fitted_size);
    let origin =
        fitted_min + Vec2::new(data::GROUP_TRANSLATE[0], data::GROUP_TRANSLATE[1]) * image_scale;
    let group_scale = data::GROUP_SCALE * image_scale;
    let transform = |point: [f32; 2]| origin + Vec2::new(point[0], point[1]) * group_scale;
    let strokes = data::STROKES
        .iter()
        .map(|source| ArtworkStroke {
            from: transform(source.from),
            to: transform(source.to),
            width: source.width * group_scale,
            color: Color32::from_rgb(source.color[0], source.color[1], source.color[2]),
        })
        .collect();
    let mut fill = Vec::new();
    let mut current = Pos2::ZERO;
    for command in data::FILL {
        match command {
            SourceCommand::Move(point) | SourceCommand::Line(point) => {
                current = transform(*point);
                fill.push(current);
            }
            SourceCommand::Cubic(points) => {
                let curve = egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        current,
                        transform(points[0]),
                        transform(points[1]),
                        transform(points[2]),
                    ],
                    false,
                    Color32::TRANSPARENT,
                    Stroke::NONE,
                );
                // Backend flattening tolerance is explicit, not a pixel-parity claim.
                fill.extend(curve.flatten(Some(0.05)).into_iter().skip(1));
                current = transform(points[2]);
            }
            SourceCommand::Close => {}
        }
    }
    // epaint closes this contour implicitly. Preserve source commands but avoid
    // a zero-length closing edge and split corner normals in backend tessellation.
    if fill.len() > 1 && fill.first() == fill.last() {
        fill.pop();
    }
    Ok(Artwork {
        strokes,
        fill,
        fill_color: Color32::from_rgb(
            data::FILL_COLOR[0],
            data::FILL_COLOR[1],
            data::FILL_COLOR[2],
        ),
        viewport,
    })
}

/// Paint source-derived asset only; no UI controls, authentication or OS effects.
///
/// # Errors
/// Same measured-image restrictions as [`geometry`]. Android Skia rasterization
/// and egui antialiasing are not asserted equivalent by successful painting.
pub fn paint(painter: &egui::Painter, bounds: Rect) -> Result<(), InvalidArtworkBounds> {
    let artwork = geometry(bounds)?;
    let painter = painter.with_clip_rect(bounds);
    for stroke in artwork.strokes {
        // Generic paths avoid line-segment pixel snapping. Explicit round caps
        // preserve the Android strokeLineCap, independent of default stroke caps.
        painter.add(egui::epaint::PathShape::line(
            vec![stroke.from, stroke.to],
            Stroke::new(stroke.width, stroke.color),
        ));
        painter.circle_filled(stroke.from, stroke.width * 0.5, stroke.color);
        painter.circle_filled(stroke.to, stroke.width * 0.5, stroke.color);
    }
    // Original final path has one convex contour, so evenOdd has no holes.
    painter.add(Shape::convex_polygon(
        artwork.fill,
        artwork.fill_color,
        Stroke::NONE,
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    #[test]
    fn canonical_source_identity_size_path_order_and_colors() {
        assert_eq!(
            source_sha256(),
            "7b5508de9a8564a86cd7461bfcf0d88b52a492653011f4b7e0bf20cc48570c89"
        );
        assert_eq!(intrinsic_size(), Vec2::splat(108.0));
        let artwork = geometry(Rect::from_min_size(Pos2::ZERO, intrinsic_size())).unwrap();
        assert_eq!(artwork.strokes.len(), 6);
        assert_eq!(
            artwork
                .strokes
                .iter()
                .map(|path| path.color)
                .collect::<Vec<_>>(),
            vec![
                Color32::from_rgb(251, 189, 6),
                Color32::from_rgb(251, 189, 6),
                Color32::from_rgb(53, 170, 83),
                Color32::from_rgb(251, 189, 6),
                Color32::from_rgb(69, 134, 243),
                Color32::from_rgb(235, 67, 52),
            ]
        );
        assert_eq!(artwork.fill_color, Color32::from_rgb(251, 189, 6));
        close(artwork.strokes[0].from.x, 30.68 + 11.0 * 0.7425);
        close(artwork.strokes[0].from.y, 30.68 + 9.0 * 0.7425);
        close(artwork.strokes[0].to.x, 30.68 + 34.0 * 0.7425);
        close(artwork.strokes[0].width, 8.0 * 0.7425);
        close(artwork.fill[0].x, 30.68 + 15.0 * 0.7425);
        assert!(artwork.fill.len() > 8);
    }

    #[test]
    fn content_scale_fit_centers_non_square_and_preserves_source_transform() {
        let bounds = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(216.0, 108.0));
        let wide = geometry(bounds).unwrap();
        assert_eq!(
            wide.viewport,
            Rect::from_min_size(Pos2::new(64.0, 20.0), Vec2::splat(108.0))
        );
        let tall = geometry(Rect::from_min_size(Pos2::ZERO, Vec2::new(108.0, 216.0))).unwrap();
        assert_eq!(tall.viewport.min, Pos2::new(0.0, 54.0));
        let small = geometry(Rect::from_min_size(Pos2::ZERO, Vec2::splat(54.0))).unwrap();
        close(small.strokes[0].width, 4.0 * 0.7425);
        close(small.strokes[0].from.x, wide.strokes[0].from.x * 0.5 - 32.0);
    }

    #[test]
    fn closed_source_contour_has_nonzero_edges_and_finite_backend_mesh() {
        let context = egui::Context::default();
        let bounds = Rect::from_min_size(Pos2::ZERO, intrinsic_size());
        let artwork = geometry(bounds).unwrap();
        assert!(artwork.fill.len() > 2);
        assert_ne!(artwork.fill.first(), artwork.fill.last());
        for index in 0..artwork.fill.len() {
            let next = (index + 1) % artwork.fill.len();
            assert!((artwork.fill[index] - artwork.fill[next]).length_sq() > 0.0);
        }
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            ui.painter().add(Shape::convex_polygon(
                artwork.fill.clone(),
                artwork.fill_color,
                Stroke::NONE,
            ));
        });
        output.textures_delta.clear();
        let primitives = context.tessellate(output.shapes, output.pixels_per_point);
        assert!(!primitives.is_empty());
        let mut vertices = 0;
        for primitive in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                panic!("Original logo contour unexpectedly requires a paint callback");
            };
            assert!(mesh.is_valid());
            vertices += mesh.vertices.len();
            assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
        }
        assert!(vertices > 0);
    }

    #[test]
    fn invalid_allocations_never_emit_approximate_geometry() {
        for bounds in [
            Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(-1.0, 108.0)),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(f32::INFINITY, 108.0)),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(f32::NAN, 108.0)),
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(16_385.0)),
        ] {
            assert_eq!(geometry(bounds), Err(InvalidArtworkBounds));
        }
    }

    #[test]
    fn paints_original_strokes_and_fill_without_platform_commands() {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            paint(painter, Rect::from_min_size(Pos2::ZERO, intrinsic_size())).unwrap();
            assert_eq!(paint(painter, Rect::NOTHING), Err(InvalidArtworkBounds));
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(output.platform_output.commands.is_empty());
    }
}
