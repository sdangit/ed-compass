# Waterfall frequency-projection experiment

Status: **implemented on `experiment/waterfall-appearance`; owner confirmed live testing on 2026-10-14**

## Scope

The main waterfall lanes offer **Logarithmic** and **Linear** frequency scales
through a dropdown. Both are post-FFT display projections over the same retained
linear FFT bins. Switching is immediate and does not restart capture, recompute
the FFT, change detector bands, or alter stored audio.

Logarithmic remains the default because it preserves low-frequency real estate
across the full audio spectrum and matches the original application's intended
Landscape/Thargoid-image workflow. Linear gives equal screen distance per hertz
and is especially useful inside a narrowed frequency viewport for inspecting
absolute tone spacing, harmonics, and drift.

## Stable overview and exports

The single overview remains Combined, Full spectrum, and Logarithmic regardless
of the main selection. Its viewport box continues to express the physical
low/high frequency bounds on that stable log map. The box therefore does not
move or resize merely because the main rendering changes projection, and users
retain visibility of activity elsewhere in the spectrum.

Canonical PNG export remains unchanged and logarithmic. A future Full/View
export choice must decide explicitly whether a view export carries its selected
projection, channel layout, timeline position, and frequency bounds.

## Extensibility and validation

The projection is an enum rather than a Boolean so a later experiment can add
another mapping without redesigning persistence or the control. Mel, Bark, ERB,
and genuine Constant-Q are intentionally not exposed without an Elite-specific
research case; a cosmetic Constant-Q-like remap must not be presented as a real
Constant-Q transform.

One `FreqScale` supplies forward and inverse mapping to waterfall rasterization,
axes, annotations, and gestures. Tests cover equal-Hz linear placement,
logarithmic defaults/reference ticks, inverse mapping, unchanged physical view
bounds, a permanently logarithmic overview, preset round-trip, and migration of
presets that predate the projection field.

## Live review

Switch repeatedly while live and while pinned in history. Verify main-lane
rerendering, sensible ticks at Full and Narrow bounds, annotation alignment,
vertical mute gestures, equal projection across comparison lanes, unchanged
overview pixels and viewport box, preset save/apply/restart, and unchanged
canonical PNG export.
