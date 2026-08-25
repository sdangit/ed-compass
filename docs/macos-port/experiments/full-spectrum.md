# Full-spectrum waterfall experiment

Status: **implemented on `experiment/linear-frequency`; live review pending**

## Research question

Can the main waterfall switch between its detector-focused frequency band and
an Adobe Audition-like full spectral-frequency display without restarting
capture, discarding history, or changing analysis and detector behavior?

## Scope

- Add a `full spectrum` toggle to the main-window controls.
- Re-render the retained waterfall immediately when the toggle changes.
- Keep the configured 140-second time window in both views.
- Use the existing 200–2400 Hz band for the focused view.
- Use 20–22050 Hz, clamped to stream Nyquist, for the full-spectrum view.
- Apply the selected band consistently to grid labels, detection and
  traced-stroke overlays, drag-to-mute mapping, and PNG export.
- Leave the analysis engine, FFT history, detectors, capture backend, and
  cockpit overlay unchanged.

## Implementation evidence

The spectrogram history retains every FFT bin through Nyquist; the configured
display band is only a rendering choice. Switching bands can therefore rebuild
the current texture from the existing 140-second history without recomputing
audio or waiting for new data.

Both views retain the existing logarithmic frequency scale. This experiment is
about focused versus full frequency coverage, not Audition's separate Frequency
Analysis amplitude plot.

## Go/no-go review

Promote only if a live session confirms that switching is responsive, the full
spectrum adds useful context, overlays remain aligned, and repeated toggling
does not create objectionable CPU or UI latency. If the full view is not useful,
retain this document as the result and do not merge the experiment.
