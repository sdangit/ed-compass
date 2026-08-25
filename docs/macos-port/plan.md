# macOS port plan

## Goal

Run ED Compass natively on macOS as a high-performance, real-time analysis
window while Elite Dangerous runs through CrossOver. Audio arrives through a
user-managed virtual Core Audio device. Journal files are tailed directly from
the CrossOver bottle. No feature may require inspecting or controlling the game
window.

## Scope

### Required

- Native Apple Silicon build; Intel or universal builds are a later decision.
- Enumeration and selection of macOS audio input devices.
- Continuous capture from a Loopback-compatible virtual device.
- Existing spectrogram, detectors, event list, capture, and export behavior.
- Configurable Elite Dangerous journal directory.
- Detection records correlated with useful journal state and nearby actions.
- Clear device-loss, permission, configuration, and journal-path diagnostics.
- Bounded CPU, memory, channel queues, and disk use.

### Explicitly excluded

- Finding, inspecting, focusing, resizing, or tracking the game window.
- The transparent in-game cockpit overlay on macOS.
- Capturing system or process audio directly with ScreenCaptureKit or a custom
  Core Audio tap during the initial port.
- Automating CrossOver or modifying its bottle.
- A SwiftUI rewrite.
- Network services, telemetry, or automatic uploads.

## Gate policy

Each phase ends in one of three states:

- **Pass:** all required evidence is recorded; the next phase may start.
- **Conditional pass:** the limitation and accepted consequence are recorded in
  `decisions.md` before proceeding.
- **Stop:** a critical assumption failed. Do not hide the failure with broader
  implementation work; evaluate the named fallback or end the port.

## Phase 0 — Core Audio feasibility probe

Status: **pass**

### Question

Can a small native Rust process discover a virtual device and continuously
receive the audio routed from Elite Dangerous/CrossOver?

### Deliverable

A disposable CLI under `tools/audio-probe/` using CPAL. It must be independent
of the ED Compass runtime and GUI.

### Probe capabilities

- Enumerate input devices with name and identifier when the backend exposes one.
- Print default and supported input configurations.
- Select a device explicitly; never silently select a physical microphone.
- Open the selected configuration and report negotiated sample rate, channel
  count, sample format, and callback buffer sizes.
- Print periodic frame count, peak, RMS, callback-gap, and error summaries.
- Optionally record a short WAV for listening and offline analysis.
- Exit cleanly on Ctrl-C and report device loss rather than hanging.

### Test procedure

1. Install and enable Loopback or an equivalent virtual audio router.
2. Create a stereo device named clearly for ED Compass.
3. Route the CrossOver/Elite audio source into it.
4. Configure monitoring so the player still hears the game.
5. Run the probe with no game audio and record the idle behavior.
6. Play known game audio and verify nonzero, changing levels.
7. Record 30 seconds and listen to the resulting WAV.
8. Run continuously for at least 30 minutes while playing.
9. Stop and restart CrossOver while the probe remains open.
10. Disable and re-enable the virtual device and record the behavior.

### Pass criteria

- The virtual device is enumerated and can be selected unambiguously.
- Recognizable game audio reaches the Rust callback and recorded WAV.
- Monitoring preserves audible output for the player.
- Channel order, sample rate, and sample interpretation are credible.
- Silence does not replay stale buffers or invent signal.
- A 30-minute run has no unexplained stalls, runaway queue growth, or crashes.
- Device removal produces an error or stopped state that can drive recovery.

### Stop/fallback criteria

- If Loopback cannot be enumerated or opened, repeat with BlackHole or another
  known Core Audio virtual input before rejecting the architecture.
- If CPAL cannot expose a usable stable identity but capture works, permit a
  conditional pass and evaluate a narrow Core Audio identifier adapter.
- If no virtual device can deliver CrossOver audio to a native input client,
  stop the port before modifying ED Compass.

## Phase 1 — Signal-integrity validation

Status: **pass**

### Question

Are the received samples analytically equivalent to usable ED Compass input?

### Work

- Analyze the probe's WAV through the existing `--input` path.
- Compare visible spectrum, duration, channel count, and levels with the heard
  material and, if available, a Windows reference capture.
- Check clipping, DC offset, repeated buffers, channel swapping, unexpected
  mono conversion, sample-rate interpretation, and idle behavior.
- Exercise a known synthetic or reference signal through the virtual route when
  legally and practically available.

### Pass criteria

- WAV duration agrees with wall time within a documented tolerance.
- Frequencies appear in the correct bins and the spectrogram is credible.
- No conversion defect would invalidate periodicity or structure detection.
- Any latency is stable enough for journal correlation; absolute low latency is
  not required for passive analysis.

## Phase 2 — ED Compass live backend

Status: **pass**

### Question

Can the proven stream drive the existing runtime without GUI changes?

### Work

- Add a target-specific macOS dependency on CPAL.
- Implement macOS enumeration, explicit selection, capture, stop, and error
  handling behind the existing audio abstractions.
- Convert supported input formats to interleaved `f32` outside expensive or
  blocking work.
- Preserve the existing bounded-channel design and timeline gap semantics.
- Run first through headless status/analysis paths.
- Keep WASAPI behavior and its loopback-only safety policy unchanged.

### Pass criteria

- Live samples produce the same runtime state transitions as file input.
- Detectors and snapshots continue to operate.
- The callback performs no FFT, GUI, journal, logging-heavy, or disk work.
- Stop and device-loss behavior cannot deadlock application shutdown.
- Existing tests pass; new format, selection-policy, and lifecycle tests pass.

## Phase 3 — Main GUI integration

Status: **pass**

### Question

Does the existing egui application provide a stable real-time Mac experience?

### Work

- Show macOS virtual inputs in the existing device picker.
- Persist the selected device identity, with a documented name fallback if
  necessary.
- Prefer the wgpu/Metal renderer on macOS unless measurement finds a defect.
- Hide or clearly mark unsupported overlay controls on macOS.
- Replace Windows-specific device and setup wording conditionally.
- Validate the main spectrogram, controls, event list, export, and capture UI.

### Pass criteria

- The app starts without selecting a physical microphone automatically.
- Device choice persists and failures are actionable.
- The UI remains responsive while analysis and recording run.
- A second monitor and normal window movement require no game-window access.
- No overlay viewport is created on macOS.

## Phase 4 — Journal discovery and correlation

Status: **pass**

### Question

Can detections be reliably tied to location and nearby player actions using the
journal inside the CrossOver bottle?

### Work

- Expose `journal_path` in the GUI with validation and, if justified, a native
  folder picker.
- Retain the current newest-file and rotation behavior.
- Store a bounded timeline of relevant recent journal events using their source
  UTC timestamps.
- Add nearby events, source journal filename, and a reproducible source position
  such as byte offset or line identity to capture sidecars.
- Preserve the convenient derived snapshot: system, coordinates, body, music,
  supercruise state, and other explicitly selected state.
- Define correlation windows using measured virtual-device latency rather than
  assuming zero latency.

### Pass criteria

- A deliberate in-game action appears in the expected journal and is associated
  with a nearby test detection/capture.
- Journal rotation does not duplicate or lose complete lines.
- Missing, stale, or inaccessible paths are visible but do not stop audio.
- Sidecars contain enough source information to audit the correlation later.

## Phase 5 — Reliability and performance

Status: **pass; sleep/wake explicitly deferred**

Known debt: capture and export currently perform encoding and filesystem work
on the main UI/analysis-pump thread. This did not invalidate the observed live
test, but the subsystem has policy, lifecycle, retention, memory, and UI
coupling that makes a naive worker-thread change unsafe. See
[`technical-debt.md`](technical-debt.md).

### Test matrix

- Multi-hour active and idle sessions.
- CrossOver/game restart.
- Virtual-device disable/re-enable.
- Default output changes while Loopback monitoring remains active.
- macOS sleep/wake.
- GUI minimized, covered, moved between monitors, and left idle.
- Manual export and automatic capture during active analysis.
- Journal creation and rotation during capture.
- Stereo first; multichannel only if it has an identified research benefit.

### Measurements

- CPU usage in release mode, overall and by major thread when practical.
- Resident memory and whether it remains bounded.
- Audio callback intervals, gaps, dropped frames, and queue pressure.
- GUI frame responsiveness.
- Audio-to-journal correlation offset and variance.

### Pass criteria

- No monotonic memory or queue growth.
- No callback blocking attributable to analysis, UI, journal, or disk work.
- Long runs preserve a truthful timeline or explicitly mark gaps.
- Performance is suitable to run beside CrossOver without materially harming
  gameplay. Record the actual hardware and measured figures in `test-log.md`.

## Phase 6 — Packaging and release decision

Status: **pass for local Apple Silicon use**

### Local package first

- Produce an Apple Silicon `.app` with an icon and `Info.plist`.
- Include the required audio-input permission description.
- Store private settings in Application Support. On first launch, ask once for
  confirmation of a pre-filled, user-visible capture library; thereafter,
  captures and exports are one-click operations with no save dialog.
- Document Loopback and journal-path setup.

### Deferred decisions

- Personal/private use versus a public GitHub fork.
- Signing and notarization.
- Intel or universal binary support.
- Alternative virtual-device documentation.
- Whether upstream maintainers would welcome the portable backend.

The publication decision occurs only after Phases 0–5 establish feasibility,
correctness, and acceptable operating cost.
