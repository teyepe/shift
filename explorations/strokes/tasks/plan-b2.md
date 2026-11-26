# Implementation Plan: Nib Solver — B2 (Per-Node Overrides)

## Status

**B2.1 complete (2026-11-25)** on `explore/strokes`.  
**B2.2 in progress** — draft code in tree; on-curve-index rewrite pending.

---

## Overview

B2 adds the `NibOverride` layer: each on-curve skeleton node can independently
override any subset of `{width, height, angle}` from the stroke-wide default.
Neighbouring nodes interpolate; unoverridden nodes fall through to the default.

B2 is split into two streams:

| Stream | Scope |
|--------|-------|
| **B2.1** | Single-master (static) overrides: storage, Stage 1 resolve, bridge, model, sidebar |
| **B2.2** | VF-aware interpolation: blend overrides across masters at a design location |

---

## Architecture

### Storage

`ContourStrokeEntry.nib_overrides: BTreeMap<String, NibOverride>` — keyed by
point-id string, serialised as `camelCase` PLIST alongside the existing stroke lib.

```rust
pub struct NibOverride {
    pub width:  Option<f64>,
    pub height: Option<f64>,
    pub angle:  Option<f64>,  // radians
}
```

### Stage 1 resolver (`nib_resolve.rs`)

Three functions:

| Function | Purpose |
|----------|---------|
| `resolved_on_curve_nibs(contour, default, overrides)` | Per on-curve node: merge sparse override with default |
| `segment_nib_pairs(contour, default, overrides)` | Align with path segments: `Vec<(CubicBez, NibShape, NibShape)>` |
| `resolve_nib(default, patch)` | Sparse field merge: each `Some` field wins |

Interpolation between overridden nodes is **linear lerp on each field independently**,
including `lerp_angle` (shortest-arc, `(-π, π]`) for the angle field. This matches
LTTR/INK's interpolation behaviour.

### Solve cache key

`solve_key_style_overrides` hashes the override map into the cache key so that
any override change invalidates cached outlines.

### Bridge / model

- Rust intents: `SetNibOverride { layer_id, contour_id, point_id, patch: NibOverride }`,
  `ClearNibOverride { layer_id, contour_id, point_id }`
- TS types: `NibOverride`, `SetNibOverride`, `ClearNibOverride` in `@shift/types`
- Model: `GlyphLayer.setNibOverride(contourId, pointId, patch)`,
  `GlyphLayer.clearNibOverride(contourId, pointId)`
- Commands: `setPointNibOverride`, `clearPointNibOverride` in `strokeCommands.ts`

---

## B2.1 — Static overrides (shipped)

### What was built

- `NibOverride` serialisable struct in `nib_shape.rs`
- `ContourStrokeEntry.nib_overrides` with `Default`, round-trip serde
- `nib_override_from_lib`, `nib_override_to_lib`, `nib_overrides_from_lib`,
  `nib_overrides_to_lib` helpers in `layer_lib.rs`
- `prune_nib_overrides` to remove stale entries when points are deleted
- Stage 1 resolver in `nib_resolve.rs`
- `LiveSkeleton.nib_overrides` field; `solve_skeleton` calls `segment_nib_pairs` +
  `nib_outline_from_segment_pairs` when overrides are present
- `set_nib_override`, `clear_nib_override`, `prune_stroke_nib_overrides_for_points`
  on `GlyphLayer`
- `RemovePoints::apply` calls `prune_stroke_nib_overrides_for_points` after removing
- `SetNibOverride`, `ClearNibOverride` intents registered in `intents.rs`
- Bridge codegen (`pnpm generate:bridge-types`) produces clean TS types
- `GlyphLayerWriter` / `GlyphLayer` model methods
- `setPointNibOverride` / `clearPointNibOverride` commands
- Sidebar nib `H` / `°` fields in `StrokeSection.tsx`
- `StrokeStylePatch` widened: `Omit<Partial<OutlineStrokeStyle>, "nib"> & { nib?: Partial<NibShape> }`
- `updateStrokeStyle` merges partial nib without resetting the other nib field

### Tests

| Test | Location |
|------|----------|
| `nib_override_on_endpoint_changes_ink_width_at_cap` | `live_stroke.rs` |
| `nib_override_survives_set_live_stroke_restyle` | `live_stroke.rs` |
| `nib_override_is_pruned_when_its_point_is_deleted` | `live_stroke.rs` |
| `b21_per_point_overrides_on_masters_do_not_panic_at_interpolated_location` | `interpolated.rs` |

The VF guard test (`b21_per_point…`) documents the **B2.1 contract**: master
overrides do not influence interpolated reads between masters (they may pass
through `blended_nib_overrides` but the solver must not fail).

---

## B2.2 — VF-aware override interpolation (shipped 2026-11-25)

### The problem

PointIds are layer-local (each layer mints its own). The draft
`blended_nib_overrides` in `interpolated.rs` takes the union of all masters'
override maps and uses raw PointId comparison — which is always empty across
masters. The result is a no-op rather than a panic, but it is not useful.

### Planned solution: on-curve index

Replace PointId-keyed union with on-curve index:

```
(contour_index: usize, on_curve_rank: usize) → NibOverride
```

For each master:
1. Enumerate contours in canonical order.
2. Within each contour, enumerate on-curve points by their traversal rank.
3. Store overrides as `BTreeMap<(usize, usize), NibOverride>`.

Blend across masters using the same weighted lerp as the skeleton geometry:
each `Option<f64>` field blended independently, `lerp_angle` for `angle`.

Inject the blended override map into `LayerStrokeLib` before calling
`set_live_stroke_contours` (or directly into `LiveSkeleton.nib_overrides`).

### Angle interpolation

For the angle field, `lerp_angle(a, b, t)` = shortest-arc interpolation in
`(-π, π]`. The RP¹ mod-π correction (needed when two masters differ by exactly
π — a 180° rotation, which is the same nib orientation) is deferred until a
fixture demonstrates the issue.

### Acceptance criteria for B2.2

- [x] A two-master font with differing width overrides on the same point (by
  on-curve index) produces blended widths at an interpolated location.
- [x] An angle override near π/2 on one master and 0 on the other interpolates
  through the shortest arc (`blend_angles_pairwise` + integration smoke).
- [x] The VF guard test (`b21_per_point…`) still passes (no regression).
- [x] No panics on fonts where masters have different contour counts (graceful
  partial blend or skip) — unchanged skeleton gating; override blend only when
  all samples stroke the contour at `index`.

---

## Open questions

1. **Contour count mismatch.** If masters have different numbers of contours,
   some contours are unmatched. Current plan: skip unmatched contours (same as
   the skeleton blending behaviour). Needs a test.

2. **RP¹ angle wrap.** When one master has `angle = π − ε` and another has
   `angle = −π + ε`, lerp_angle goes the long way around. In practice this
   means someone has drawn a nearly-flat nib at opposite rotation on two masters.
   Decide on arrival using a fixture.

3. **Sidebar override UI.** B2.1 ships only global nib height/angle in the sidebar.
   Per-point override fields (shown when a single on-curve point is selected) are
   deferred to a UI polish pass.
