# Implementation Plan: Nib Solver — B1 (Elliptical Translation Nib)

## Status

**Complete (2026-11-25)** on `explore/strokes`. Implementation matches this plan except: both offset strategies ship behind an eccentricity gate (`NibShape::offset_mode`) pending Kostya sign-off; Minkowski was not removed. **Next stream work:** B2/B3 in [nib-spec.md](../nib-spec.md).

## Overview

Implement the B1 nib stroke solver in `crates/shift-font/src/stroke/`. B1 covers a
stroke-wide elliptical nib (`{width, height, angle}`) with no per-node overrides.
The solver must handle open and closed paths, nib-aware caps and joins, and produce
variable-font-safe deterministic point counts.

Kostya's starting point is the **offset path commit** — replacing the double-offset
logic for closed contours with kurbo's `expand_path`. That commit is T0 and is fully
backwards-compatible with the existing constant-width stroke.

---

## Architecture Decisions

### 1. Single entry point, two strategy implementations

All paths — circular, elliptical, rotated, open, closed — route through one function:

```rust
pub fn nib_outline(input: &NibStrokeInput, mode: OffsetMode) -> BezPath
```

An `OffsetMode` enum selects the offset-curve strategy:

```rust
pub enum OffsetMode { Perturbation, Minkowski }
```

The strategies implement one trait:

```rust
pub trait OffsetStrategy {
    fn offset_segment(
        &self,
        seg: CubicBez,
        nib_start: NibShape,
        nib_end: NibShape,
        side: f64,
        out: &mut BezPath,
    );
}
```

Caps, joins, overlap removal, and point-count stabilisation are **shared pipeline
steps** outside the trait — not duplicated in each strategy. This prevents
Frankenstein code: adding per-node overrides (B2) only touches the resolution step,
not either strategy.

### 2. `expand_path` as a transparent fast path

For the special case **closed single-subpath + uniform nib + outward side only**,
the rotate-expand-rotate trick gives the **exact** Minkowski offset in one call,
bypassing the strategy entirely:

```
rotate_path(path, -nib.angle)
→ expand_path(rotated, Diagonal2::new(nib.width/2, nib.height/2), join, ...)
→ rotate_path(result, nib.angle)
```

This is not an approximation — `expand_path` with a `Diagonal2` scales normals by
`Diagonal2`, which produces `sqrt((a·cos α)² + (b·sin α)²)` — exactly the ellipse
support function at angle 0, with the rotation baked in by the frame change.

The fast path is a checked `Option`-returning helper called at the top of
`nib_outline`. All other cases fall through to the general strategy. No branching
leaks into the shared pipeline.

### 3. `NibShape::support(normal_angle)` is the unifying formula

```rust
impl NibShape {
    pub fn support(&self, normal_angle: f64) -> f64 {
        let a = self.width  / 2.0;
        let b = self.height / 2.0;
        let d = normal_angle - self.angle;
        ((a * d.cos()).powi(2) + (b * d.sin()).powi(2)).sqrt()
    }

    pub fn is_circular(&self) -> bool {
        (self.width - self.height).abs() < 1e-9
    }
}
```

The existing `OutlineStrokeStyle` becomes the special case
`NibShape::circular(width)`. Every offset-curve calculation — fast path, perturbation,
Minkowski — calls `support()`. Nothing duplicates the formula.

### 4. Two solver strategies, compared before shipping

| Strategy | Speed | Accuracy | Point count |
|---|---|---|---|
| **Perturbation** (Levien/Thottingal) | O(1) per segment | ε ≤ 2 u at depth=2 | Fixed: `4N + K + 2` |
| **Minkowski** (exact) | O(few) per segment | Exact envelope | Same with fixed subdivision |

Both are built against the same METAFONT reference fixtures. After comparison:

- If perturbation error ≤ 1 u on all fixtures → ship perturbation, remove Minkowski.
- If error > 1 u on high-contrast fixtures → gate on eccentricity:
  `height/width < 0.3` → Minkowski; otherwise → Perturbation.

`OffsetMode` is an internal solver parameter, never exposed to the user.

### 5. Nib-aware caps and joins are dispatch, not duplication

A `NibCap` and `NibJoin` module dispatch on `(StrokeCap/Join, NibShape)`:

- `StrokeCap::Nib` on a translation nib → ellipse-arc trace between endpoints
- `StrokeCap::Nib` on a flat brush (`height ≈ 0`) → straight line segment  
- `StrokeJoin::Nib` → edge of the nib boundary (parallelogram edge)
- `StrokeCap::Round` → semi-ellipse fitted to nib extent
- `StrokeCap::Butt` / `StrokeJoin::Bevel/Miter` → unchanged from existing

All existing `StrokeCap` and `StrokeJoin` values remain valid. No breaking changes.

---

## Task List

### Phase 0: Offset path commit (Kostya's starting point)

- [x] T0: Replace closed-contour double-offset with `expand_path` in `position.rs`

### Checkpoint: Phase 0

- [x] All existing stroke tests pass: `cargo test -p shift-font --lib stroke`

### Phase 1: Foundation (shared types)

- [x] T1: `NibShape`, `support()`, `NibShape::circular()`, `NibShape::is_circular()`
- [x] T2: `OffsetStrategy` trait + `OffsetMode` enum
- [x] T3: `NibStrokeInput`, extend `OutlineStrokeStyle` with `nib: NibShape`

### Checkpoint: Phase 1

- [x] `cargo test -p shift-font --lib stroke::nib` — support function table tests pass

### Phase 2: Perturbation strategy

- [x] T4: Closed-path fast path (`try_nib_expand_path`) + `PerturbationStrategy` for open paths
- [x] T5: G¹ correction at segment joints (15° threshold, Thottingal)
- [x] T6: Nib-aware caps (`nib_caps.rs`)
- [x] T7: Nib-aware joins (`nib_joins.rs`)
- [x] T8: Solver assembly — `nib_outline()` with fast path + perturbation strategy

### Checkpoint A: Perturbation working

- [x] T9: METAFONT reference fixtures + tests (`h` stem, `o` bowl, diagonal)
- [x] All tests pass, outlines are closed, deterministic point count verified

### Phase 3: True Minkowski strategy (comparison)

- [x] T10: Cusp detection — Newton-bisection on `κ · h_outer = 1`
- [x] T11: `MinkowskiStrategy` — cusp-subdivide → exact offset via `support()` → Bézier fit
- [x] T12: Comparison harness — both strategies on all fixtures, measure max point deviation

### Checkpoint B: Decision

- [ ] Review comparison with Kostya
- [x] Eccentricity gate ships (`NibShape::offset_mode`); both strategies kept for now

- [x] T13: Eccentricity-based dispatch (loser not removed yet)
- [x] T14: Point-count determinism test (angle independence on closed topology)

### Phase 4: Integration

- [x] T15: Wire `NibShape` into `OutlineStrokeStyle`, update `solve_cache`, `live_stroke`
- [x] T16: TS bridge + `NibShape` in model types + `setNibShape` stub (wasm preview uses existing stroke read path)

### Checkpoint: Complete

- [x] All acceptance criteria met for B1 (UI deferred to B2)
- [x] Performance budget: existing dense-contour perf gate passes
- [ ] Ready for review (after Kostya checkpoint B)

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Cusp detection (Newton-bisection) is flaky on degenerate cubics | High | Clamp t to [ε, 1−ε]; use the existing `is_straight` guard first |
| `expand_path` inward-side artifacts | Low | The fast path only triggers for the outward side; inward goes through the general solver |
| G¹ kinks visible at 15° threshold | Medium | Make threshold configurable; default 15°; test at 5° and 30° too |
| Perturbation error > 1 u on flat-brush (`height ≈ 0`) | High | The spec says this is likely at high eccentricity; the Minkowski path is the mitigation |
| Point-count non-determinism from adaptive subdivision | High | Never use error-adaptive subdivision; always use fixed `target_depth` |
| `expand_path` does not handle multi-subpath closed paths | Low | Fast path checks `subpath_count == 1`; falls through for compound glyphs |

## Open Questions

- After comparison (Checkpoint B): do we keep `OffsetMode` as a testing affordance
  behind a `#[cfg(test)]` gate, or as a runtime option for future overlap-removal
  debugging? (Suggested: keep it internal, hidden behind a feature flag.)
- Should the nib cap default be `StrokeCap::Nib` for new strokes, or `StrokeCap::Round`
  for compatibility with existing strokes? (Suggested: `StrokeCap::Nib` for new nib
  strokes; existing constant-width strokes keep `Round`/`Butt` as today.)
