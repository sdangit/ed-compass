# Waterfall timeline-navigation experiment

Status: **implemented on `experiment/waterfall-navigation`; live review pending**

## Research question

Can the main waterfall provide useful display-only time zoom and historical
inspection within its retained 140 seconds without changing capture, analysis,
detection, buffer capacity, or export behavior?

## Interaction model

- A compact overview spectrogram always spans the complete 140-second history.
- The overview follows the focused/full-spectrum display toggle and retains its
  own time and frequency axes, detection strip, and event/stroke annotations.
- The main viewport offers 140, 70, 35, and 15-second durations.
- Clicking the overview centers the selected duration on that time, clamped to
  retained-history boundaries.
- A bright overview rectangle shows the slice rendered in the main waterfall;
  the surrounding history is dimmed.
- Live mode pins the viewport to the newest audio. Historical inspection pins
  it to absolute analysis time, so its rectangle moves left as new audio arrives.
- The Live button returns to the newest edge. The 140-second view is inherently
  live because it already displays the complete retained history.
- Historical x-axis labels remain relative to live now. Only a live viewport
  labels its right edge `now`.

## Boundaries

The same retained FFT frames are re-rendered; analysis and detector work never
depends on the viewport. Detection and traced-stroke boxes are clipped and
re-projected into both views without being recomputed. The original waterfall
drag code is untouched.

Export remains the established independent operation: it renders the latest
140 seconds at high resolution and captures the latest audio ring, regardless
of the inspected viewport. A future view-specific export requires a separate,
explicit action.

## Go/no-go review

Live review should confirm that overview clicks select the expected evidence,
zoom preserves the inspected center, Live reliably follows the newest audio,
annotations remain aligned, focused/full-spectrum switching updates both views,
and the added rendering cost is acceptable beside CrossOver.
