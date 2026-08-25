# Linear-frequency waterfall experiment

Status: **implemented on `experiment/linear-frequency`; live review pending**

## Research question

Can the main waterfall switch between its established logarithmic frequency
projection and a linear-Hz projection without restarting capture, discarding
history, or changing analysis and detector behavior?

## Scope

- Add a `linear Hz` toggle to the main-window controls.
- Re-project the retained waterfall immediately when the toggle changes.
- Keep the configured 140-second time window and 200–2400 Hz display band.
- Apply the selected projection consistently to grid labels, detection and
  traced-stroke overlays, drag-to-mute mapping, and PNG export.
- Leave the analysis engine, FFT history, detectors, capture backend, and
  cockpit overlay unchanged.
- Keep logarithmic frequency as the default because it gives low-frequency
  structure more screen space and matches the established decode workflow.

## Implementation evidence

The spectrogram history stores quantized linear FFT bins at full retained
resolution. Frequency projection occurs only in the UI renderer, so changing
the axis can rebuild the current texture from existing history; it does not
need to recompute audio or wait for a new 140-second window.

Unit coverage verifies that equal frequency intervals occupy equal vertical
distances in linear mode, logarithmic mode remains the default, and both
projections invert screen rows back to frequency correctly.

## Go/no-go review

Promote only if a live session confirms that switching is responsive, existing
signals remain interpretable, overlays remain aligned, and repeated toggling
does not create objectionable CPU or UI latency. If linear mode does not add
useful inspection value, retain this document as the result and do not merge
the experiment into `macos-port`.
