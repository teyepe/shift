# Nib Solver B1 — Task List

See [plan.md](./plan.md) for architecture decisions, risks, and open questions.

**Status (2026-11-25):** B1 **complete** on `explore/strokes`. Checkboxes below are the original audit list; the [plan.md](./plan.md) task list reflects what shipped. **Next:** B2 in [nib-spec.md](../nib-spec.md).

---

## Phase 0: Offset path commit

### T0: Replace closed-contour double-offset with `expand_path`

**Description:** In `position.rs`, the closed-contour branch runs two `offset_subpath`
calls then stitches them. Replace with a single `expand_path` call (uniform nib only).
Kostya's starting point — reduces LoC, no behaviour change.

**Acceptance criteria:**
- [ ] All 11+ existing `position.rs` tests pass unchanged
- [ ] The closed-contour branch is visibly shorter (≤ 5 lines for the common case)
- [ ] `expand_path` is imported from `kurbo`; the double-offset path for open contours
      is untouched

**Verification:**
- [ ] `nix develop --command cargo test -p shift-font --lib stroke`

**Dependencies:** None

**Files likely touched:**
- `crates/shift-font/src/stroke/position.rs`

**Estimated scope:** S

---

## Phase 1: Foundation

### T1: `NibShape` struct + `support()` + helpers

**Description:** Introduce `NibShape { width, height, angle }` and its key methods.
The monoline stroke is `NibShape::circular(w)`. Table-driven unit tests cover the
support function at four canonical angles for an asymmetric nib.

**Acceptance criteria:**
- [ ] `NibShape::support(α)` matches formula `sqrt((a·cos(α−θ))² + (b·sin(α−θ))²)`
      for `a = width/2`, `b = height/2`
- [ ] `NibShape::circular(w).support(any_angle) == w/2` within 1e-9
- [ ] `NibShape { height: 0 }.support(θ) ≈ 0` (flat brush in its own direction)
- [ ] `is_circular()` returns `true` iff `|width − height| < 1e-9`

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::nib`

**Dependencies:** None

**Files likely touched:**
- `crates/shift-font/src/stroke/nib_shape.rs` (new)
- `crates/shift-font/src/stroke/mod.rs` (re-export)

**Estimated scope:** S

---

### T2: `OffsetStrategy` trait + `OffsetMode` enum

**Description:** Define the trait that both solver strategies implement. `OffsetMode`
selects the strategy at the call site. This is the DRY joint — adding a third strategy
later touches only this file and the new implementor.

**Acceptance criteria:**
- [ ] `OffsetStrategy::offset_segment` has the right signature (see plan.md)
- [ ] `OffsetMode` has two variants: `Perturbation`, `Minkowski`
- [ ] No concrete implementation yet — just the interface

**Verification:**
- [ ] `cargo build -p shift-font` (no errors, no unused warnings)

**Dependencies:** T1

**Files likely touched:**
- `crates/shift-font/src/stroke/offset_strategy.rs` (new)
- `crates/shift-font/src/stroke/mod.rs`

**Estimated scope:** XS

---

### T3: `NibStrokeInput` + extend `OutlineStrokeStyle` with `nib: NibShape`

**Description:** `OutlineStrokeStyle` gains a `nib` field. The existing monoline
path sets `nib = NibShape::circular(width)` via `to_exact()`. `NibStrokeInput` wraps
the full solver input (path, nib, overrides stub, cap, join, miter_limit, position).
No existing call sites break.

**Acceptance criteria:**
- [ ] `OutlineStrokeStyle::to_exact()` still compiles and produces the same `ExactStrokeStyle`
- [ ] `OutlineStrokeStyle::round(w).nib == NibShape::circular(w)`
- [ ] `NibStrokeInput` has all fields from spec §2 Solver input
- [ ] `NibOverride` stub is defined (all `Option<f64>`) but unused

**Verification:**
- [ ] `cargo test -p shift-font` (all existing tests still pass)

**Dependencies:** T1

**Files likely touched:**
- `crates/shift-font/src/stroke/style.rs`
- `crates/shift-font/src/stroke/nib_solver.rs` (new, stubs only at this stage)

**Estimated scope:** S

---

## ✅ Checkpoint: Phase 1

- [ ] `cargo test -p shift-font --lib stroke` — all pass
- [ ] `NibShape` support function tests pass

---

## Phase 2: Perturbation strategy

### T4: `PerturbationStrategy` — offset curves via Cramer's rule

**Description:** Implement the Levien/Thottingal perturbation formula.
For each `CubicBez` segment:
1. Evaluate the support function at t = 0, 0.5, 1 in the outward normal direction.
2. Compute three offset points: `P_offset(t) = B(t) + support(normal_angle(t)) · N(t)`.
3. Fit a cubic Bézier through these three points + tangents via Cramer's rule.

`target_depth` controls subdivision: 1 for near-circular nibs, 2 for high-contrast
(eccentricity > 0.3). Fixed, never adaptive.

**Acceptance criteria:**
- [ ] A circular nib (`height == width`) produces the same outline as the existing
      `centred_outline` within `DEFAULT_TOLERANCE`
- [ ] Increasing `target_depth` from 1 to 2 adds no extra on-curve points (only
      improves handle quality)
- [ ] The result is always a valid `BezPath` (no `NaN`, no degenerate moves)

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::perturbation`

**Dependencies:** T2, T3

**Files likely touched:**
- `crates/shift-font/src/stroke/strategy/perturbation.rs` (new)
- `crates/shift-font/src/stroke/strategy/mod.rs` (new)

**Estimated scope:** M

---

### T5: G¹ correction at segment joints

**Description:** At each smooth skeleton joint, the perturbation formula can produce
a tangent kink proportional to nib eccentricity × curvature. Implement the
Thottingal skeleton-based correction: if the tangent jump Δβ < 15° and the skeleton
itself is smooth, blend the offset tangent toward the skeleton tangent.

**Acceptance criteria:**
- [ ] A smooth spline with a circular nib produces zero correction (Δβ = 0 for
      constant width)
- [ ] A 45°-nib stroke on a horizontal straight line produces zero correction
      (no curvature, no kink)
- [ ] A high-eccentricity nib (`height/width = 0.1`) on a curved segment produces
      a Δβ < 15° correction that reduces the visual kink

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::g1_correction`

**Dependencies:** T4

**Files likely touched:**
- `crates/shift-font/src/stroke/g1_correction.rs` (new)
- `crates/shift-font/src/stroke/strategy/perturbation.rs`

**Estimated scope:** S

---

### T6: Nib-aware caps

**Description:** Implement the nib cap for open contours. The nib cap traces the
ellipse boundary between the left and right trace endpoints. For a flat brush
(`height ≈ 0`) this degenerates to a straight line. Also validate that existing
`Round` and `Butt` caps still work correctly when nib is non-circular.

**Acceptance criteria:**
- [ ] `StrokeCap::Nib` on a translation nib (θ=45°) produces an arc, not a line
- [ ] `StrokeCap::Nib` on a flat brush (`height < 1e-6`) produces a straight line
- [ ] `StrokeCap::Round` produces a semi-ellipse fitted to the nib extent at the
      end angle
- [ ] `StrokeCap::Butt` produces a straight line perpendicular to the end tangent
      (unchanged from existing)

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::nib_caps`

**Dependencies:** T1

**Files likely touched:**
- `crates/shift-font/src/stroke/nib_caps.rs` (new)

**Estimated scope:** M

---

### T7: Nib-aware joins

**Description:** Implement the nib join for corner nodes. The nib join closes the
gap on the outer side with a segment of the nib boundary — a parallelogram edge for
a translation nib. For a circular nib it degenerates to a round arc.

**Acceptance criteria:**
- [ ] `StrokeJoin::Nib` on a 90° corner with a broad nib (θ=45°) produces a small
      parallelogram edge, not a smooth arc
- [ ] `StrokeJoin::Nib` on a 90° corner with a circular nib matches `StrokeJoin::Round`
- [ ] `StrokeJoin::Miter`, `Bevel`, `Round` all behave as before on circular nibs

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::nib_joins`

**Dependencies:** T1, T6 (reference for cap geometry pattern)

**Files likely touched:**
- `crates/shift-font/src/stroke/nib_joins.rs` (new)

**Estimated scope:** M

---

### T8: Solver assembly — `nib_outline()` entry point

**Description:** Assemble the full pipeline:
1. `try_expand_path_fast` — closed single-subpath + uniform nib + outward side only →
   rotate-expand-rotate with kurbo's `expand_path` (exact Minkowski, zero extra LoC).
2. `general_nib_outline` — stage resolve → offset curves (via strategy) → caps →
   joins → close → overlap removal.

**Acceptance criteria:**
- [ ] A circular nib on a closed single-subpath path takes the fast path
- [ ] A non-circular nib (or open path) goes through the general path
- [ ] Both paths produce the same outline for `NibShape::circular(w)` on a
      closed path (fast path == general path, no visible difference)

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::nib_solver`

**Dependencies:** T4, T5, T6, T7

**Files likely touched:**
- `crates/shift-font/src/stroke/nib_solver.rs`

**Estimated scope:** S

---

## ✅ Checkpoint A: Perturbation working

### T9: METAFONT reference fixtures + tests

**Description:** Add three test fixtures based on METAFONT's canonical test cases.
Each fixture is a JSON file (skeleton + nib params) in `tests/fixtures/nib/`.
Assertions cover: closed outline, outward/inward side thickness within 2 u of the
support function value, deterministic on-curve count.

**Fixtures:**
- `h-stem`: vertical open path, θ=45°, 30×15 nib — classic calligraphic 'h' stem
- `o-bowl`: closed elliptical path, θ=45°, 30×15 nib — the 'o' counter
- `diagonal`: diagonal open path, θ=45°, 30×15 nib — 45° stroke perpendicular
  to nib = maximum hairline

**Acceptance criteria:**
- [ ] All three fixtures produce valid closed outlines
- [ ] `h-stem` left edge is thicker than right edge (nib contrast is correct)
- [ ] `o-bowl` outer edge is 15 u from skeleton, inner edge is 15 u (± 2 u for
      perturbation approximation)
- [ ] On-curve count is exactly `4N + K + 2` for each fixture

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::nib_fixtures`

**Dependencies:** T8

**Files likely touched:**
- `crates/shift-font/tests/fixtures/nib/*.json` (new)
- `crates/shift-font/src/stroke/nib_solver.rs` (test module)

**Estimated scope:** M

---

## Phase 3: True Minkowski strategy

### T10: Cusp detection — Newton-bisection on `κ · h_outer = 1`

**Description:** For each `CubicBez`, find parameters `t_c` where the inner offset
curve cusps: `|κ(t)| · h_outer(φ(t)) = 1`. Use Newton-bisection (3–4 iterations,
numerically stable). Guard against degenerate straight segments.

**Acceptance criteria:**
- [ ] A circle of radius `r` with a nib of support `h` returns `t_c` only when `h = r`
      (the exact cusp condition)
- [ ] A straight segment returns `[]` (no cusps)
- [ ] All returned `t_c` values satisfy `κ(t_c) · h_outer(t_c) ≈ 1` within `1e-4`

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::cusp_detection`

**Dependencies:** T1

**Files likely touched:**
- `crates/shift-font/src/stroke/cusp_detection.rs` (new)

**Estimated scope:** M

---

### T11: `MinkowskiStrategy` — exact offset via cusp subdivision

**Description:** Implement the exact offset strategy: subdivide each segment at cusp
locations, then compute the exact offset for each sub-segment using `offset_cubic`
from kurbo. Where the inner offset curves cross, the overlap is left for the
overlap-removal pass. Fixed subdivision count (not error-adaptive) for determinism.

**Acceptance criteria:**
- [ ] A circular nib produces the same outline as the existing `centred_outline`
      within `DEFAULT_TOLERANCE`
- [ ] A flat brush (`height ≈ 0`) on a curved segment produces cusps on the inner
      trace that are subsequently handled by overlap removal
- [ ] The on-curve count is at most `4N + K + 2 + 2·C` where `C` = number of cusps

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::minkowski`

**Dependencies:** T10, T2

**Files likely touched:**
- `crates/shift-font/src/stroke/strategy/minkowski.rs` (new)

**Estimated scope:** M

---

### T12: Comparison harness — perturbation vs Minkowski

**Description:** Run both strategies on all T9 fixtures and the same additional
high-contrast fixture (`h-stem` with `height = 1`, near-flat brush). Measure maximum
point deviation between the two outputs at a dense sampling (100 pts per segment).
Print the results as SPIKE notes.

**Acceptance criteria:**
- [ ] Comparison runs without panics for all fixtures
- [ ] For `height/width > 0.3`: perturbation max deviation ≤ 1 u
- [ ] For `height/width < 0.1` (near-flat): perturbation deviation is documented
      (may exceed 1 u — that is what we are measuring)
- [ ] Results are printed as `SPIKE` lines (same convention as existing perf marks)

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::comparison -- --nocapture`

**Dependencies:** T8, T11

**Files likely touched:**
- `crates/shift-font/src/stroke/nib_solver.rs` (test module addition)

**Estimated scope:** S

---

## ✅ Checkpoint B: Decision

- [ ] Review comparison output with Kostya
- [ ] Decide: single winner, or eccentricity-based dispatch (see plan.md)

---

### T13: Cleanup — remove loser or add eccentricity dispatch

**Description:** Based on Checkpoint B, either remove the losing strategy
(dead code, test coverage covered by the winner) or introduce a
`NibShape::offset_mode() -> OffsetMode` helper that selects based on eccentricity.
If gating on eccentricity, document the threshold and add a test that exercises both
paths.

**Acceptance criteria:**
- [ ] No dead code warnings after cleanup
- [ ] If eccentricity gate: a near-flat fixture uses `Minkowski`, a near-circular
      fixture uses `Perturbation`, verified in test

**Verification:**
- [ ] `cargo build -p shift-font` (no dead-code warnings)
- [ ] `cargo test -p shift-font --lib stroke`

**Dependencies:** T12

**Files likely touched:**
- `crates/shift-font/src/stroke/strategy/` (remove one file)
- `crates/shift-font/src/stroke/nib_shape.rs` (add `offset_mode()` if gating)

**Estimated scope:** XS

---

### T14: Point-count determinism test

**Description:** Assert that the on-curve count equals `4N + K + 2` for all
perturbation-path fixtures, and that varying nib parameters on the same skeleton
topology produces identical counts. This is the VF-safety invariant.

**Acceptance criteria:**
- [ ] `h-stem` with `angle = 0` and `angle = π/2` produce the same on-curve count
- [ ] `o-bowl` with `height = width` (circle) and `height = 1` (flat) produce the
      same on-curve count
- [ ] Any two values of `position` in `[-1, 1]` on the same skeleton produce the
      same on-curve count

**Verification:**
- [ ] `cargo test -p shift-font --lib stroke::nib_determinism`

**Dependencies:** T13

**Files likely touched:**
- `crates/shift-font/src/stroke/nib_solver.rs` (test module)

**Estimated scope:** XS

---

## Phase 4: Integration

### T15: Wire `NibShape` into `OutlineStrokeStyle`, `solve_cache`, `live_stroke`

**Description:** Plumb the new `nib` field through the production path.
`solve_cache` gains a nib-aware cache key; `live_stroke` reads `NibShape` from the
style and passes it to `nib_outline`. Existing constant-width strokes produce
`NibShape::circular(width)` and are unaffected.

**Acceptance criteria:**
- [ ] All existing stroke tests still pass
- [ ] The solve cache correctly invalidates when `nib` changes
- [ ] `live_stroke` calls `nib_outline` for the new path, `outline` for the legacy path

**Verification:**
- [ ] `cargo test -p shift-font` (full crate)
- [ ] Performance: `cargo test --release -p shift-font -- perf_mark_dense_contour_drag_frame`

**Dependencies:** T13, T14

**Files likely touched:**
- `crates/shift-font/src/stroke/style.rs`
- `crates/shift-font/src/stroke/solve_cache.rs`
- `crates/shift-font/src/stroke/live_stroke.rs`

**Estimated scope:** M

---

### T16: TS bridge + `NibShape` in model types + wasm preview

**Description:** Expose `NibShape` over the bridge. Add `nib?: NibShape` to the
stroke style TS type. Add a `setNibShape` command stub (no UI yet — that is B1 scope
limit). The wasm preview path picks up `NibShape` from the stroke style.

**Acceptance criteria:**
- [ ] TypeScript compiles with the new nib field (`pnpm typecheck`)
- [ ] The existing preview path is unaffected for constant-width strokes
- [ ] `setNibShape` is a no-op command that at least doesn't crash

**Verification:**
- [ ] `pnpm typecheck`
- [ ] `pnpm test` (all existing TS tests pass)

**Dependencies:** T15

**Files likely touched:**
- `crates/shift-bridge/src/bridge/workspace.rs` (or relevant bridge file)
- `packages/types/src/domain.ts`
- `apps/desktop/src/lib/commands/` (setNibShape stub)
- `crates/shift-wasm/src/stroke_preview.rs`

**Estimated scope:** M

---

## ✅ Checkpoint: Complete

- [ ] All 16 tasks checked
- [ ] `pnpm test` passes
- [ ] Performance budget: nib_outline on 1,021-pt contour ≤ 10 ms
- [ ] No regressions on existing constant-width stroke tests
- [ ] Deterministic point count verified for VF safety
- [ ] Ready for review with Kostya
