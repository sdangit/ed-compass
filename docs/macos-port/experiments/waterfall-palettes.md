# Waterfall palette experiment

Status: **implemented on `experiment/waterfall-appearance`; owner confirmed live testing on 2026-10-14**

## Question

Can alternate post-FFT colour ramps improve visual inspection without changing
analysis, detection, navigation, or the meaning of semantic annotations?

## First pass

**Settings > Waterfall > Palette** offers four global choices with an inline
gradient preview:

- **ED Compass** preserves the original dark blue/magenta/yellow ramp and is
  the default for existing and new configurations;
- **Inferno** provides a dark purple/red/yellow perceptual ramp;
- **Viridis** provides a dark purple/blue/green/yellow ramp with strong
  color-vision accessibility;
- **Grayscale** provides a black-to-white structural reference.

The selected ramp maps the same normalized display intensities in the combined
full-spectrum overview, all main channel lanes, the Windows cockpit strip, and
PNG exports. Axes, viewport handles, journal timing, detector strips, and event
boxes keep their semantic colours. Selecting a palette therefore invalidates
display textures but does not rerun the FFT, rebuild retained histories, or
change any detector input.

Palette is a global persisted appearance choice rather than part of a named
time/frequency view preset. A purpose-built light-canvas palette is deferred;
none of these choices is a naive RGB inversion.
