# Channel-isolated waterfall experiment

Status: **implemented for live visual review on `experiment/waterfall-navigation`**

## Research question

Does separating the captured input channels make signals easier to inspect than
the existing combined waterfall, without coupling visualization to direction
finding or changing detector behavior?

## First-pass behavior

- Offer `Combined`, each named input channel, stereo/pair comparisons, and
  useful multichannel groups. Mono inputs expose only `Combined`.
- On 7.1 layouts, comparisons include left/right sides and the front, back, and
  side speaker pairs. Groups include left side, right side, front stage,
  surrounds, and every full-range channel except LFE.
- Temporarily omit `All channels`: eight main lanes and eight fixed overview
  lanes competed for the same window so severely that neither was useful.
- Apply the selected view to both the 140-second overview and the main
  waterfall.
- Keep time zoom, historical position, Live state, and frequency range shared
  across every visible lane.
- Give every lane an internal zero-second time offset so later experiments can
  add independent visual alignment without replacing the viewport model.
- Keep overview navigation fixed. Stereo `L + R` divides the ordinary main
  waterfall allocation evenly between two visible lanes; larger multichannel
  stacks and the existing instruments can scroll vertically inside the central
  analysis area. The resizable Events footer and its own event-list scrolling
  remain unchanged.
- Retain raw and background-subtracted (`excess`) history independently for
  every input channel.
- Project combined-analysis annotations onto channel lanes with subdued
  outlines. They do not claim channel-specific detection.
- Leave capture, export, detectors, and direction finding behavior unchanged.

## Pipeline boundary

The existing Combined path remains authoritative. With direction finding off,
it still downmixes before its single FFT; separate display-only channel FFTs
populate the new histories. With direction finding on, the display reuses the
per-channel FFTs already calculated for bearing rather than duplicating them.
Each channel has a display-only background model for the Excess view; those
models never feed detection.

Group views are derived from the already-retained channel histories by
averaging spectral power. The active group lanes are backfilled once when
selected and then append only newly arrived FFT frames; the overview and main
waterfall render from the same bounded caches. This avoids waveform phase
cancellation and the earlier cost of recomputing every source channel for every
output pixel, without allocating permanent raw and Excess histories for every
possible group.

This adds CPU and retained-memory cost even when direction finding is disabled.
The live review must therefore cover idle resource use as well as visual value.

## Deferred decisions

- Independent per-channel time shifting and alignment controls.
- A usable `All channels` design. One candidate is overview-only; it remains
  deferred until the comparison and group views have been reviewed live.
- Channel-specific detection and annotation confidence.
- Channel-specific export behavior; Export continues to use the existing
  combined spectrogram and capture workflow.
- Collapsing the Azimuth/Periodicity/Channels instrument row.

## Live review

Check Combined/individual/stacked switching, Focused/Full and Excess behavior,
overview click/drag navigation, 15–140 second zooms, resizing the Events footer,
vertical analysis scrolling, and CPU/memory use during a representative Elite
session.
