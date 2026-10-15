# Waterfall timeline-navigation experiment

Status: **first live review passed on `experiment/waterfall-navigation`; owner confirmed follow-up live testing on 2026-10-14**

## Research question

Can the main waterfall provide useful display-only time zoom and historical
inspection within its retained 140 seconds without changing capture, analysis,
detection, buffer capacity, or export behavior?

## Interaction model

- A 162 px overview spectrogram always spans the complete 140-second history,
  full available spectrum, and combined signal, regardless of the selected
  channel presentation or main frequency slice.
- The overview retains its own time and frequency axes, detection strip, and
  event/stroke annotations.
- The main viewport offers 140, 70, 35, and 15-second durations.
- Clicking the overview centers the selected duration on that time, clamped to
  retained-history boundaries.
- Dragging the viewport rectangle moves it continuously while preserving the
  point where it was grabbed. A drag beginning outside first centers the box on
  that time and then continues; a click inside without movement is a no-op.
- A bright two-dimensional overview rectangle shows both the time and frequency
  slice rendered in the main waterfall; the surrounding context is dimmed.
- The viewport box can be dragged in both dimensions. Full time or frequency
  locks only that dimension; the other remains movable. Clicking outside the
  box recentres the active dimensions at that point. The earlier dedicated
  vertical rail is hidden while this unified interaction is evaluated.
- Dragging an edge resizes one dimension and dragging a corner resizes both.
  Custom time and frequency sizes are saved with the last-used startup view. Time is clamped to
  at least five seconds and frequency to at least half an octave so an
  accidental microscopic selection remains useful and recoverable.
- Live mode pins the viewport to the newest audio. Historical inspection pins
  it to absolute analysis time, so its rectangle moves left as new audio arrives.
- The Live button returns to the newest edge. The 140-second view is inherently
  live because it already displays the complete retained history.
- Historical x-axis labels remain relative to live now. Only a live viewport
  labels its right edge `now`.
- The top edge is reserved for journal timing, separate from the detector-state
  strip along the bottom. A white dashed `HS` stem and white solid `FSD` stem
  identify the journal's explicit hyperspace boundaries. A top bracket joins a
  paired jump and shows its duration. `HS` and `FSD` labels remain visible in
  both the overview and main waterfall; hovering the bracket lists the recorded
  milestones. Violet is reserved for a timing outlier and does not reuse the
  detector-box palette. The complete journal overlay is controlled by the
  Settings > Journal presentation toggle; hiding it will not disable
  journal collection or correlation.
- Journal timestamps are projected onto the audio-analysis clock after each
  poll, including the existing configurable audio-route offset.

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
annotations remain aligned, frequency navigation updates both views, and the
added rendering cost is acceptable beside CrossOver.

The first live review found the interaction useful and click navigation
intuitive. It also found that the initial 72 px overview crowded frequency
labels and that the main waterfall's fixed four-Hz Mac rebuild cadence became
visibly discontinuous at short time windows. The overview is now 108 px tall.
Main-waterfall cadence adapts to visible time-per-pixel, bounded between four
and fifteen rebuilds per second; the overview retains the lower-cost cadence.
Historical viewport position, raster selection, axes, timeline paint, and
annotations use the analysis engine's current audio clock. Using the less
frequently refreshed GUI snapshot let retained FFT history advance between
clock updates, making a pinned slice alternately creep forward and snap back.
The raster is additionally pinned to an absolute FFT-column index chosen at
navigation time. This removes the remaining boundary jitter from repeatedly
rounding a continuous seconds offset while discrete history columns arrive.

## Hyperspace journal corpus check

An expanded local corpus of 584 journals spanning 25 game builds contained
6,193 hyperspace starts and 6,189 complete pairs. Arrival was usually 18 or 19
seconds after departure (94% of completed jumps); 310 were longer than 21
seconds, closely matching the upper five-percent tail, and the maximum was 130
seconds. `ShipLocker` appeared in every completed jump at a median five seconds.
Most added time accumulated before `FSDTarget`, although a few extreme cases
also delayed after it. Four starts were incomplete; three were superseded by a
different `StartJump` and one reached end-of-file. The tracker therefore aborts
an open observation on any intervening start and never persists an incomplete
pair.

The separately cited 2026-07-25 Ipilyak jump is present in the expanded corpus.
It is nominal: `ShipLocker` and `Music` at +5 seconds, `FSDTarget` at +7, then
`ReceiveText`, discovery records, and `FSDJump` at +18 seconds.

## Timing observations

The runtime pairs each hyperspace start with the next `FSDJump` and records the
first `ShipLocker`, `Music`, `FSDTarget`, and `ReceiveText` milestones found
between them. Completed observations include total and milestone-relative
timings, journal provenance, origin and destination when known, jump distance,
and boost use. They append as deduplicated JSON lines to
`hyperspace-jumps.jsonl` beside the active configuration. Existing observations
are loaded to seed the baseline; an unreadable log is preserved and disables
new persistence rather than being overwritten.

Outlier styling starts only after 20 earlier observations. It uses the median
plus three median absolute deviations, with a minimum three-second allowance
for the journal's one-second timestamp precision. The classification is
explicitly “timing outlier,” not an audio anomaly.
