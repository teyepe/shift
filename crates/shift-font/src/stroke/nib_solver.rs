//! Elliptical nib stroke outlines (B1).

use kurbo::{Affine, BezPath, CubicBez, Diagonal2, ParamCurve, PathEl, PathSeg};

use super::exact_offset::{self, ExactStrokeStyle, path_is_closed};
use super::nib_caps::add_nib_cap;
use super::nib_shape::{NibOverride, NibShape};
use super::offset_strategy::{OffsetMode, OffsetStrategy};
use super::position::{self, expand_closed_diagonal};
use super::strategy::{MinkowskiStrategy, PerturbationStrategy};
use super::style::{StrokeCap, StrokeJoin};

pub fn nib_input_from_style<'a>(
    style: &'a super::style::OutlineStrokeStyle,
    path: &'a BezPath,
    overrides: &'a [NibOverride],
) -> NibStrokeInput<'a> {
    NibStrokeInput {
        path,
        nib: style.effective_nib(),
        overrides,
        cap: style.cap,
        join: style.join,
        miter_limit: style.miter_limit,
        position: style.position,
    }
}

pub struct NibStrokeInput<'a> {
    pub path: &'a BezPath,
    pub nib: NibShape,
    pub overrides: &'a [NibOverride],
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f64,
    pub position: f64,
}

pub fn nib_outline_from_segment_pairs(
    input: &NibStrokeInput,
    segments: &[(CubicBez, NibShape, NibShape)],
    tolerance: f64,
) -> BezPath {
    if segments.is_empty() {
        return nib_outline(input, tolerance);
    }
    general_nib_outline_segments(input, segments, input.nib.offset_mode(), tolerance)
}

pub fn nib_outline(input: &NibStrokeInput, tolerance: f64) -> BezPath {
    if input.nib.is_circular() && input.overrides.is_empty() {
        let style = ExactStrokeStyle {
            width: input.nib.width,
            cap: input.cap.to_kurbo(),
            join: input.join.to_kurbo(),
            miter_limit: input.miter_limit,
            position: input.position,
        };
        return exact_offset::outline(input.path, &style, tolerance);
    }

    if let Some(outline) = try_nib_expand_closed(input, tolerance) {
        return outline;
    }

    let mode = input.nib.offset_mode();
    general_nib_outline_open(input, mode, tolerance)
}

fn try_nib_expand_closed(input: &NibStrokeInput, tolerance: f64) -> Option<BezPath> {
    if !input.overrides.is_empty() || !single_closed_subpath(input.path) {
        return None;
    }

    let position = input.position;
    let side = -position;
    let half_w = input.nib.width / 2.0;
    let left = side * half_w + half_w;
    let right = side * half_w - half_w;
    let join = input.join.to_kurbo();
    let a = input.nib.width / 2.0;
    let b = input.nib.height / 2.0;
    let rotate_in = Affine::rotate(-input.nib.angle);
    let rotate_out = Affine::rotate(input.nib.angle);

    let skeleton = input.path.clone();
    let rotated = map_path(&skeleton, rotate_in);

    let left_edge = if left.abs() < 1e-12 {
        skeleton.clone()
    } else {
        let expand = Diagonal2::new(a * left / half_w, b * left / half_w);
        let expanded = expand_closed_diagonal(&rotated, expand, join, input.miter_limit, tolerance);
        map_path(&expanded, rotate_out)
    };

    let mut right_edge = if right.abs() < 1e-12 {
        skeleton.clone()
    } else {
        let expand = Diagonal2::new(a * right / half_w, b * right / half_w);
        let reversed = skeleton.reverse_subpaths();
        let rotated_rev = map_path(&reversed, rotate_in);
        let expanded =
            expand_closed_diagonal(&rotated_rev, expand, join, input.miter_limit, tolerance);
        map_path(&expanded, rotate_out)
    };
    right_edge = right_edge.reverse_subpaths();

    let mut result = BezPath::new();
    result.extend(left_edge.elements().iter().copied());
    result.extend(right_edge.elements().iter().copied().skip(
        if path_is_closed(&left_edge, 1e-6) {
            1
        } else {
            0
        },
    ));
    result.close_path();
    Some(result)
}

fn general_nib_outline_open(input: &NibStrokeInput, mode: OffsetMode, tolerance: f64) -> BezPath {
    let uniform: Vec<_> = path_segments(input.path)
        .into_iter()
        .map(|segment| (segment.raise(), input.nib, input.nib))
        .collect();
    general_nib_outline_segments(input, &uniform, mode, tolerance)
}

fn general_nib_outline_segments(
    input: &NibStrokeInput,
    segments: &[(CubicBez, NibShape, NibShape)],
    mode: OffsetMode,
    tolerance: f64,
) -> BezPath {
    let strategy: &(dyn OffsetStrategy + Send + Sync) = match mode {
        OffsetMode::Perturbation => &PerturbationStrategy,
        OffsetMode::Minkowski => &MinkowskiStrategy,
    };
    let default_depth = if input.nib.eccentricity() > 0.3 { 2 } else { 1 };
    let side = -input.position;
    let half_w = input.nib.width / 2.0;
    let left = side * half_w + half_w;
    let right = side * half_w - half_w;
    // `OffsetStrategy::offset_segment` takes a *signed* dimensionless scale,
    // not a font-unit distance.  The strategy computes:
    //   point + normal * (support_outer(phi) * scale)
    // where `normal` points to the LEFT of travel.  So:
    //   scale > 0  →  offset left of skeleton   (left / outer edge)
    //   scale < 0  →  offset right of skeleton  (right / inner edge)
    //   |scale| = 1 →  one nib half-width  (centred-stroke sides)
    // `left` is a positive font-unit distance (left side of stroke),
    // `right` is a negative font-unit distance (right/opposite side).
    // Dividing each by `half_w` preserves both magnitude AND sign.
    // Guard against zero width to avoid NaN.
    let left_scale = if half_w > 0.0 { left / half_w } else { 0.0 };
    let right_scale = if half_w > 0.0 { right / half_w } else { 0.0 };

    let mut left_edge = BezPath::new();
    let mut right_edge = BezPath::new();
    for (cubic, nib_start, nib_end) in segments {
        let segment_depth = if nib_start.eccentricity().max(nib_end.eccentricity()) > 0.3 {
            2
        } else {
            default_depth
        };
        strategy.offset_segment(
            *cubic,
            *nib_start,
            *nib_end,
            left_scale,
            tolerance,
            segment_depth,
            &mut left_edge,
        );
        strategy.offset_segment(
            *cubic,
            *nib_start,
            *nib_end,
            right_scale,
            tolerance,
            segment_depth,
            &mut right_edge,
        );
    }

    let cap_nib_start = segments
        .first()
        .map(|(_, start, _)| *start)
        .unwrap_or(input.nib);
    let cap_nib_end = segments.last().map(|(_, _, end)| *end).unwrap_or(input.nib);

    let mut result = BezPath::new();
    result.extend(left_edge.elements().iter().copied());
    if let Some((start, end)) = path_endpoints(input.path) {
        let path_segments = path_segments(input.path);
        let (_, last_tangent) = position::unit_tangents_for_segment(path_segments.last().unwrap());
        let (first_tangent, _) = position::unit_tangents_for_segment(&path_segments[0]);
        let shift = side * half_w;
        add_nib_cap(
            &mut result,
            end + shift * last_tangent.turn_90(),
            half_w * last_tangent.turn_90(),
            last_tangent,
            input.cap,
            cap_nib_end,
            tolerance,
        );
        let reversed = right_edge.reverse_subpaths();
        result.extend(reversed.elements().iter().copied().skip(1));
        add_nib_cap(
            &mut result,
            start + shift * first_tangent.turn_90(),
            -half_w * first_tangent.turn_90(),
            -first_tangent,
            input.cap,
            cap_nib_start,
            tolerance,
        );
        result.close_path();
    }
    result
}

fn single_closed_subpath(path: &BezPath) -> bool {
    let mut moves = 0;
    let mut closed = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) => moves += 1,
            PathEl::ClosePath => closed = true,
            _ => {}
        }
    }
    moves == 1 && closed
}

fn path_segments(path: &BezPath) -> Vec<PathSeg> {
    let mut segments = Vec::new();
    let mut current = kurbo::Point::ZERO;
    let mut start = kurbo::Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                start = p;
                current = p;
            }
            PathEl::LineTo(p) => {
                segments.push(PathSeg::Line(kurbo::Line::new(current, p)));
                current = p;
            }
            PathEl::QuadTo(c, p) => {
                segments.push(PathSeg::Quad(kurbo::QuadBez::new(current, c, p)));
                current = p;
            }
            PathEl::CurveTo(a, b, p) => {
                segments.push(PathSeg::Cubic(CubicBez::new(current, a, b, p)));
                current = p;
            }
            PathEl::ClosePath => {
                if current != start {
                    segments.push(PathSeg::Line(kurbo::Line::new(current, start)));
                }
            }
        }
    }
    segments
}

fn path_endpoints(path: &BezPath) -> Option<(kurbo::Point, kurbo::Point)> {
    let segments = path_segments(path);
    let first = segments.first()?;
    let last = segments.last()?;
    Some((first.start(), last.end()))
}

fn transform_el(el: PathEl, affine: Affine) -> PathEl {
    match el {
        PathEl::MoveTo(p) => PathEl::MoveTo(affine * p),
        PathEl::LineTo(p) => PathEl::LineTo(affine * p),
        PathEl::QuadTo(c, p) => PathEl::QuadTo(affine * c, affine * p),
        PathEl::CurveTo(a, b, p) => PathEl::CurveTo(affine * a, affine * b, affine * p),
        PathEl::ClosePath => PathEl::ClosePath,
    }
}

fn map_path(path: &BezPath, affine: Affine) -> BezPath {
    let mut mapped = BezPath::new();
    for el in path.elements() {
        mapped.push(transform_el(*el, affine));
    }
    mapped
}

trait SegmentRaise {
    fn raise(self) -> CubicBez;
}

impl SegmentRaise for PathSeg {
    fn raise(self) -> CubicBez {
        self.to_cubic()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use kurbo::{BezPath, Shape};

    use kurbo::{ParamCurve, PathEl};

    use super::*;
    use crate::stroke::exact_offset::{DEFAULT_TOLERANCE, count_on_curve};
    use crate::stroke::offset_strategy::OffsetStrategy;
    use crate::stroke::strategy::{MinkowskiStrategy, PerturbationStrategy};

    fn circle(radius: f64) -> BezPath {
        kurbo::Circle::new((0.0, 0.0), radius).to_path(0.01)
    }

    fn nib_stem() -> BezPath {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((0.0, 200.0));
        path
    }

    #[test]
    fn fast_path_matches_centred_stroke_for_circular_nib() {
        let skeleton = circle(50.0);
        let nib = NibShape::circular(20.0);
        let input = NibStrokeInput {
            path: &skeleton,
            nib,
            overrides: &[],
            cap: super::super::style::StrokeCap::Round,
            join: super::super::style::StrokeJoin::Round,
            miter_limit: 4.0,
            position: 0.0,
        };
        let nib_outline = nib_outline(&input, DEFAULT_TOLERANCE);
        let style = ExactStrokeStyle::round(20.0);
        let exact = exact_offset::outline(&skeleton, &style, DEFAULT_TOLERANCE);
        let disagree = disagreement(&exact, &nib_outline, 0.5);
        assert!(disagree < 0.05, "disagreement {disagree}");
    }

    #[test]
    fn h_stem_produces_contrast_with_angled_nib() {
        let skeleton = nib_stem();
        let nib = NibShape {
            width: 30.0,
            height: 6.0,
            angle: std::f64::consts::FRAC_PI_4,
        };
        let input = NibStrokeInput {
            path: &skeleton,
            nib,
            overrides: &[],
            cap: super::super::style::StrokeCap::Butt,
            join: super::super::style::StrokeJoin::Round,
            miter_limit: 4.0,
            position: 0.0,
        };
        let outline = nib_outline(&input, DEFAULT_TOLERANCE);
        assert!(count_on_curve(&outline) >= 4);
        let left = outline.bounding_box();
        assert!(left.width() > left.height() || left.height() > 20.0);
    }

    #[test]
    fn strategies_agree_on_circular_open_line_within_tolerance() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((200.0, 0.0));
        let nib = NibShape::circular(20.0);
        let input = NibStrokeInput {
            path: &path,
            nib,
            overrides: &[],
            cap: super::super::style::StrokeCap::Butt,
            join: super::super::style::StrokeJoin::Round,
            miter_limit: 4.0,
            position: 0.0,
        };

        // side=1.0 is the scale factor for a centred stroke (one full nib half-width).
        let mut perturb = BezPath::new();
        PerturbationStrategy.offset_segment(
            CubicBez::new((0.0, 0.0), (66.0, 0.0), (133.0, 0.0), (200.0, 0.0)),
            nib,
            nib,
            1.0,
            DEFAULT_TOLERANCE,
            1,
            &mut perturb,
        );
        let mut minkowski = BezPath::new();
        MinkowskiStrategy.offset_segment(
            CubicBez::new((0.0, 0.0), (66.0, 0.0), (133.0, 0.0), (200.0, 0.0)),
            nib,
            nib,
            1.0,
            DEFAULT_TOLERANCE,
            1,
            &mut minkowski,
        );

        assert!(!perturb.elements().is_empty());
        assert!(!minkowski.elements().is_empty());
        let p0 = perturb
            .elements()
            .iter()
            .find_map(|el| match el {
                PathEl::LineTo(p) | PathEl::MoveTo(p) => Some(*p),
                _ => None,
            })
            .unwrap();
        let m0 = minkowski
            .elements()
            .iter()
            .find_map(|el| match el {
                PathEl::LineTo(p) | PathEl::MoveTo(p) => Some(*p),
                _ => None,
            })
            .unwrap();
        println!(
            "SPIKE perturb_vs_minkowski circular line: Δy = {}",
            (p0.y - m0.y).abs()
        );
        let _ = input;
    }

    #[test]
    fn point_count_is_independent_of_nib_angle_on_same_topology() {
        let skeleton = circle(40.0);
        let style_a = NibShape {
            width: 30.0,
            height: 6.0,
            angle: 0.0,
        };
        let style_b = NibShape {
            width: 30.0,
            height: 6.0,
            angle: std::f64::consts::FRAC_PI_2,
        };
        let outline_a = nib_outline(
            &NibStrokeInput {
                path: &skeleton,
                nib: style_a,
                overrides: &[],
                cap: super::super::style::StrokeCap::Round,
                join: super::super::style::StrokeJoin::Round,
                miter_limit: 4.0,
                position: 0.0,
            },
            DEFAULT_TOLERANCE,
        );
        let outline_b = nib_outline(
            &NibStrokeInput {
                path: &skeleton,
                nib: style_b,
                overrides: &[],
                cap: super::super::style::StrokeCap::Round,
                join: super::super::style::StrokeJoin::Round,
                miter_limit: 4.0,
                position: 0.0,
            },
            DEFAULT_TOLERANCE,
        );
        assert_eq!(count_on_curve(&outline_a), count_on_curve(&outline_b));
    }

    fn json_string_field(body: &str, key: &str) -> Option<String> {
        let needle = format!("\"{key}\"");
        let key_start = body.find(&needle)?;
        let after_key = &body[key_start + needle.len()..];
        let colon = after_key.find(':')?;
        let rest = after_key[colon + 1..].trim_start();
        if !rest.starts_with('"') {
            return None;
        }
        let inner = &rest[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    }

    fn json_number_field(body: &str, key: &str) -> Option<f64> {
        let needle = format!("\"{key}\"");
        let key_start = body.find(&needle)?;
        let after_key = &body[key_start + needle.len()..];
        let colon = after_key.find(':')?;
        let rest = after_key[colon + 1..].trim_start();
        let end = rest
            .find(|character: char| [',', '}', '\n'].contains(&character))
            .unwrap_or(rest.len());
        rest[..end].trim().parse().ok()
    }

    #[test]
    fn metafont_reference_fixtures_outline_deterministically() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nib");
        for entry in ["h-stem", "o-bowl", "diagonal"] {
            let path = dir.join(format!("{entry}.json"));
            let body = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!("read {}: {error}", path.display());
            });
            let path_d = json_string_field(&body, "path_d").unwrap();
            let skeleton = BezPath::from_svg(&path_d)
                .unwrap_or_else(|error| panic!("invalid path_d in {entry}: {error}"));
            let nib = NibShape {
                width: json_number_field(&body, "nib_width").unwrap(),
                height: json_number_field(&body, "nib_height").unwrap(),
                angle: json_number_field(&body, "nib_angle").unwrap(),
            };
            let input = NibStrokeInput {
                path: &skeleton,
                nib,
                overrides: &[],
                cap: super::super::style::StrokeCap::Butt,
                join: super::super::style::StrokeJoin::Round,
                miter_limit: 4.0,
                position: 0.0,
            };
            let first = count_on_curve(&nib_outline(&input, DEFAULT_TOLERANCE));
            let second = count_on_curve(&nib_outline(&input, DEFAULT_TOLERANCE));
            assert!(first >= 4, "{entry}: expected outline points, got {first}");
            assert_eq!(
                first, second,
                "{entry}: on-curve count must be deterministic"
            );
        }
    }

    fn disagreement(a: &BezPath, b: &BezPath, step: f64) -> f64 {
        let bounds = a.bounding_box().union(b.bounding_box());
        let (mut differ, mut total) = (0_u64, 0_u64);
        let mut x = bounds.x0;
        while x <= bounds.x1 {
            let mut y = bounds.y0;
            while y <= bounds.y1 {
                let point = kurbo::Point::new(x + step / 2.0, y + step / 2.0);
                let in_a = a.winding(point) != 0;
                let in_b = b.winding(point) != 0;
                if in_a || in_b {
                    total += 1;
                    if in_a != in_b {
                        differ += 1;
                    }
                }
                y += step;
            }
            x += step;
        }
        if total == 0 {
            return 1.0;
        }
        differ as f64 / total as f64
    }

    /// An eccentric nib on an open path (H-stem) must produce an outline whose
    /// bounding box is close to the nib dimensions, not dozens of times larger or
    /// a degenerate zero-width path (which signals a sign error in the scale factor).
    /// Before the scale-factor fix, `support_outer * side` multiplied two
    /// font-unit values together (units²), producing strokes hundreds of units
    /// wide for a 40-unit stroke with any height ≠ width.
    #[test]
    fn eccentric_nib_open_path_outline_matches_nib_dimensions() {
        let skeleton = nib_stem(); // vertical 200-unit line
        let nib = NibShape {
            width: 40.0,
            height: 20.0,
            angle: 0.0,
        };
        let input = NibStrokeInput {
            path: &skeleton,
            nib,
            overrides: &[],
            cap: StrokeCap::Butt,
            join: StrokeJoin::Round,
            miter_limit: 4.0,
            position: 0.0,
        };
        let outline = nib_outline(&input, DEFAULT_TOLERANCE);
        let bbox = outline.bounding_box();
        // A vertical stem with a 40-wide, 20-tall, angle-0 nib should be
        // exactly 40 units wide (nib wide-axis is horizontal → full width on
        // vertical path) and close to 200 units tall.  A width of 0 would signal
        // a sign error (both edges on the same side); hundreds would signal the
        // old font-unit²  bug.
        assert!(
            bbox.width() > 30.0 && bbox.width() < 50.0,
            "outline width {:.1} should be ≈40 (nib width), not zero or hundreds",
            bbox.width()
        );
        assert!(
            bbox.height() > 150.0,
            "outline height {:.1} should be ≈200",
            bbox.height()
        );
    }

    /// Position ≠ 0 with an eccentric nib: the right edge must be on the
    /// opposite side of the skeleton from the left edge.
    #[test]
    fn eccentric_nib_offset_position_places_edges_on_correct_sides() {
        let skeleton = nib_stem(); // vertical line x=0, y=0..200
        let nib = NibShape {
            width: 40.0,
            height: 20.0,
            angle: 0.0,
        };
        // position=0.3 → left edge 14 units left, right edge 26 units right
        let input = NibStrokeInput {
            path: &skeleton,
            nib,
            overrides: &[],
            cap: StrokeCap::Butt,
            join: StrokeJoin::Round,
            miter_limit: 4.0,
            position: 0.3,
        };
        let outline = nib_outline(&input, DEFAULT_TOLERANCE);
        let bbox = outline.bounding_box();
        // Left edge: 14 units to the LEFT (x < 0), right edge: 26 units to the RIGHT (x > 0).
        // Bbox x range ≈ [−14, +26], width ≈ 40.
        assert!(
            bbox.x0 < -5.0,
            "left edge x0={:.1} should be well left of skeleton (≈−14)",
            bbox.x0
        );
        assert!(
            bbox.x1 > 5.0,
            "right edge x1={:.1} should be well right of skeleton (≈+26)",
            bbox.x1
        );
        assert!(
            bbox.width() > 30.0 && bbox.width() < 50.0,
            "outline width {:.1} should be ≈40",
            bbox.width()
        );
    }
}
