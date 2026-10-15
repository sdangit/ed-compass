# Waterfall frequency-navigation experiment

Status: **implemented on `experiment/waterfall-navigation`; owner confirmed live testing on 2026-10-14**

## Research question

Can the retained post-FFT waterfall support useful log-frequency zoom and pan
without changing capture, FFT geometry, detection, stored history, or canonical
export behavior?

## Interaction model

- One shared frequency viewport controls every main channel lane, keeping
  comparisons vertically aligned. The overview remains full-spectrum and shows
  the selected band as the height and vertical position of its viewport box.
- Full displays 20 Hz through 24 kHz, clamped to the negotiated Nyquist limit.
- Wide, Medium, and Narrow display 6, 3, and 1 octave respectively.
- Resizing the overview box creates a Custom width, clamped to a minimum half
  octave and saved with the last-used startup view. Selecting a named width replaces it.
- Changing width preserves the geometric frequency centre and clamps the band
  at the full-range boundaries.
- The two-dimensional overview is the active frequency navigator: its box can
  be moved or resized vertically while the full-spectrum context stays visible.
  The earlier dedicated vertical rail is hidden and its width reclaimed, but
  its implementation is retained during the experiment in case precise
  frequency-only navigation proves necessary later.
- Axes, mute gestures, event boxes, and traced-stroke boxes use the same scale
  as the raster and clip naturally at the viewport edges.
- The main lanes can project the selected physical band logarithmically or
  linearly. This changes only retained-bin-to-row mapping. The combined
  full-spectrum overview stays logarithmic, including the viewport-box
  geometry, so changing the main projection never removes low-frequency
  context or makes overview navigation ambiguous.

The initial Medium view is centred on the geometric midpoint of the prior
200–2400 Hz focused band. It is exactly three octaves rather than preserving
that older 3.6-octave special case. Future named recipes may supply exact
display bounds and timeline durations; generic navigation does not anticipate
their schema.

## Boundaries and performance

This is a render projection over the existing full-bin spectrogram history. It
does not filter PCM, alter the FFT, change detector bands, or provide the future
pre-FFT low/high/single-frequency analysis controls.

The rail is vector geometry rather than another live spectrogram texture, so it
adds no transform, history, image allocation, or texture upload. Frequency
changes explicitly invalidate the existing overview and main textures; a
stationary historical time/frequency slice remains cached. Texture updates stay
in place and raster dimensions remain bounded by screen pixels and available
FFT columns.

PNG export remains independent of both navigation axes and uses the configured
canonical frequency band, logarithmic projection, and latest retained timeline.
A future view-specific export requires its own explicit action.

## Go/no-go review

Live review should confirm the four widths, centre preservation, boundary
clamping, click and grab-preserving drag, axis and annotation alignment,
multi-lane synchronization, and acceptable performance during continuous drag.
