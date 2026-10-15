# Waterfall view presets experiment

Status: **implemented on `experiment/waterfall-appearance`; owner confirmed live testing on 2026-10-14**

## Scope

Named user presets persist the post-FFT waterfall viewport only: timeline
duration, exact lower and upper display frequencies, and the main-view
frequency projection. They deliberately do
not store historical timeline position, channel selection, Raw/Excess mode,
colour treatment, detector settings, exports, or pre-FFT filters.

Applying a preset preserves the historical point currently under inspection;
when live, it remains attached to now. A saved frequency range is clamped to
the negotiated Nyquist limit for display without changing its stored value.
Manual changes to duration, frequency centre, or frequency width mark the
selected preset as Modified while retaining it as the Manage target. Moving
only through retained time does not. Explicitly selecting Custom clears that
relationship.

## User workflow

- `Save Current As…` asks for a unique name in a modal dialog.
- A case-insensitive collision requires explicit `Update Existing` or `Choose
  Another Name`; it never overwrites silently.
- Manage offers explicit update, rename, and delete confirmations for the
  selected user preset.
- Rename collisions remain in the dialog as correctable errors and never
  replace another preset.
- Delete leaves the current viewport unchanged and marks it Custom.
- Names are trimmed, limited to 64 characters, and may contain punctuation;
  `Custom` is reserved.

Bundled presets, hiding/resetting built-ins, and a global Reset Defaults action
are deferred to the future Settings design.

## Persistence and failure behavior

`view-presets.toml` is a versioned private settings file beside the active
`config.toml`, normally under `~/Library/Application Support/ED Compass` on
macOS. It is independent of the capture library in Documents.

Each mutation creates a missing parent directory and writes, flushes, and
atomically replaces through a temporary file in the same directory. Memory is
updated only after persistence succeeds. Errors remain visible in the open
dialog for retry; failed updates and deletes preserve the previous state.

A malformed or unsupported-version file is preserved and reported in the UI.
Preset mutations remain disabled for that session rather than replacing the
file. Already loaded presets remain applicable without further disk access.

The projection field is optional on read so files written before selectable
frequency projections remain valid and default to Logarithmic. New writes
include the field. This additive migration keeps schema version 1 and remains
readable by the earlier parser, which ignores unknown TOML fields.

## Live review

Create Landscape Signal and Titan Signal presets, restart the app, and confirm
both reload. Exercise apply while live and while inspecting history, explicit
overwrite, rename collision, delete cancellation and confirmation, and manual
viewport changes returning the selector to Custom.
