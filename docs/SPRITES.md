# Zerochi sprite contract

The final character is original pixel art supplied later. M0 uses an unmistakable CSS placeholder; no downloaded or “temporary final-looking” art is allowed.

## Asset

- PNG with transparent background.
- Square, consistently sized frames; `frameSize` is manifest/runtime input, not hardcoded. Expected candidates include 32, 48, or 64 px.
- Rows and frames are zero-indexed. Empty cells may be transparent.
- Render with `image-rendering: pixelated` and integer scale 1×–4×; never smooth or fractionally scale.

## Definition

Each animation declares `id`, `row`, `startFrame`, `frameCount`, `fps`, and `loop`. Components select ids; they do not hardcode coordinates. Required ids:

`idle_satisfied`, `idle_happy`, `idle_busy`, `idle_tired`, `idle_exhausted`, `idle_overwhelmed`, `new_mail`, `look_mail`, `eat_mail`, `inbox_reduced`, `inbox_zero`, `sleep`.

Animation lengths and FPS may differ. One-shot animations call completion once and return to the current mood's idle. Replacing an animation resets its frame. The asset and manifest must version together.

## Performance acceptance

The animator owns no timer when `visible=false`; closing the popup stops playback immediately. Use one low-frequency interval only while visible, clean it up on definition/visibility/unmount changes, and avoid permanent canvas or `requestAnimationFrame` loops.

## Delivery checklist

Original-art provenance recorded; transparent edges inspected; all frames aligned; ids complete; manifest validates within sheet bounds; integer scales visually checked; reduced-motion fallback defined; hidden timer test passes.
