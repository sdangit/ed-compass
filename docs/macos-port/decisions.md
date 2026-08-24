# Decisions and open questions

This is a lightweight decision log. Add entries when a choice affects later
phases or narrows the supported product.

## Accepted decisions

### 2026-08-24 — Agentic maintenance with no current support program

Development, upstream synchronization, validation, packaging, and repository
maintenance are performed through agentic coding workflows documented in the
root `AGENTS.md` and `maintenance.md`. The GitHub fork and any interim release
artifacts exist for personal storage and deployment. They do not establish
public support, compatibility promises, or a distribution program. Formal
Developer ID signing, notarization, broader platform support, and a public
maintenance posture will be reconsidered after upstream reaches 1.0 and the Mac
branch has remained current, unless the owner changes this decision earlier.

The upstream MIT license and A Zimin's attribution must remain in the source,
Cargo metadata, About/credits presentation, and packaged artifacts. The Mac
bundle therefore includes a copy of `LICENSE`; downstream maintenance does not
claim ownership of the original work.

### 2026-08-24 — Separate private settings from user-visible evidence

On macOS, `config.toml` belongs under
`~/Library/Application Support/ED Compass`. Audio captures, journal sidecars,
and spectrogram exports are research artifacts the user needs to browse, so
they live under a user-confirmed capture library, suggested as
`~/Documents/ED Compass`. The first-launch screen pre-fills the Loopback input,
capture library, detected journal directory, and System appearance. Confirmation
is required once; routine Capture and Export actions never open a save dialog.
The renderer remains an internal default and is not exposed during setup.

### 2026-08-24 — Package an ad-hoc-signed Apple Silicon app locally

`packaging/macos/package.sh` builds `dist/ED Compass.app` with bundle identifier
`com.edcompass.ed-compass`. It carries the original icon, Cargo version,
original-author copyright and MIT attribution, credits, license, and the Core
Audio input purpose string. An ad-hoc signature gives the local bundle a valid
code identity without claiming Developer ID distribution or notarization.
Launch Services opened the bundle successfully and the bundled arm64 process
remained running. The original icon contains native artwork only through 256 px;
a higher-resolution source is optional visual polish, not a packaging blocker.

### 2026-08-23 — Work locally until feasibility is established

Development remains on the local `macos-port` branch through the prototype
gates. Whether the result becomes a private repository, a public GitHub fork,
or an upstream contribution is deferred.

### 2026-08-23 — Use a virtual audio device

The initial port depends on a user-managed virtual audio router such as
Loopback. ED Compass will consume the resulting Core Audio input instead of
implementing system/process audio capture.

### 2026-08-23 — Keep the existing Rust GUI

The existing `egui`/`eframe` main window is the Mac interface. There will be no
SwiftUI rewrite during the port.

### 2026-08-23 — Exclude game-window features on macOS

The macOS build will not find or track the Elite Dangerous window. The cockpit
overlay, focus following, and window-relative placement are excluded. The use
case is a normal ED Compass window on another display.

### 2026-08-23 — Retain and strengthen journal integration

The app will tail journals in the CrossOver bottle from a configured directory.
Location snapshots remain, and the port should add auditable correlation with
nearby player-action events.

### 2026-08-23 — Use CPAL 0.18.2 for the feasibility probe

The isolated probe uses CPAL 0.18.2. On the baseline Mac it exposes a stable
Core Audio device ID and the input's supported configurations. This decision
applies only to the probe until virtual-device capture passes the Phase 0 gate.

### 2026-08-23 — Virtual-device capture is feasible

The Phase 0 probe enumerated and opened a Loopback device named
`ED Compass Audio`. With Brave Browser configured as the source, it captured a
temporary WAV whose playback contained the expected browser audio. The critical
Core Audio/CPAL feasibility assumption passed. CrossOver-specific routing,
long-run continuity, stable identity, and interruption behavior remain separate
tests rather than reasons to withhold this architectural result.

Follow-up testing completed a continuous 30-minute recording successfully.
Loopback monitoring worked after an output device was added to its
configuration. Restarting CrossOver did not stop capture. Disabling Loopback
stopped capture immediately. Phase 0 therefore passes; device removal is an
explicit terminal stream event, and automatic reattachment belongs in the
production backend rather than the disposable probe.

### 2026-08-23 — Probe recordings are valid analysis input

Four Phase 0 float WAVs, from 30 seconds through 30 minutes, loaded and completed
the existing ED Compass offline analysis path. They had the expected 48 kHz
stereo format, negligible DC, no non-finite samples, credible spectrograms, and
normal detector execution. Phase 1 passes. Two long recordings had a single
sample slightly above nominal full scale, so setup documentation should
recommend modest headroom in Loopback and the source application.

### 2026-08-23 — macOS live capture requires explicit device identity

The production backend uses CPAL 0.18.2 and accepts only an exact configured
Core Audio input ID. It never substitutes the default input. If macOS privacy
rules withhold enumeration from a newly built executable, an explicit ID may be
opened directly so the OS can authorize that executable without weakening the
microphone-safety rule.

### 2026-08-23 — Retain egui with wgpu/Metal on macOS

The existing main window passed live macOS testing with the wgpu renderer:
Loopback selection and persistence, real-time levels and spectrogram rendering,
movement between displays, and audio/image export all worked. The Mac build does
not create the unsupported cockpit overlay viewport. Phase 3 passes without a
SwiftUI rewrite.

A later live comparison measured roughly 560 MB with wgpu versus 750 MB with
Glow. wgpu/Metal remains the macOS default on both performance and memory
grounds; Glow is a startup fallback, not a recommended Mac configuration.

### 2026-08-23 — Preserve raw journal provenance in capture sidecars

Journal correlation stores the complete JSON line plus its source filename and
byte offset, rather than only a hand-picked subset of fields. This keeps the
record auditable and avoids losing newly introduced Elite event fields. Memory
is bounded to the newest 4096 valid events. Sidecars also record the estimated
audio interval, correlation window, and configured virtual-route offset; the
initial zero offset is explicitly uncalibrated and will be replaced by a live
measurement.

Live testing subsequently confirmed that a deliberate Elite action was attached
to the expected capture with the correct source journal, byte offset, raw JSON,
and correlation timing metadata. This satisfies the Phase 4 auditability gate.

### 2026-08-24 — Defer sleep/wake recovery

The intended use is an active Mac session beside Elite Dangerous, not unattended
operation across system sleep. Direct Loopback removal tested the critical
device-loss and reattachment code more deterministically: the app remained
open, retried the exact device, preserved same-format history, and inserted a
truthful outage gap. Sleep/wake remains explicitly unverified and is not a Phase
5 blocker unless the supported use case later expands.

## Remaining questions

| Question | Earliest decision point | Evidence needed |
|---|---|---|
| What latency and variance does the virtual route add? | Before claiming precise action/audio timing | Measured known-event experiment |
| What minimum macOS version should be supported? | Before distribution | Tests on the oldest claimed system |
| Should the result remain private or be published as a fork? | Phase 6 | Feasibility, quality, and maintenance decision |
| Are signing, notarization, Intel, or universal builds required? | Phase 6 | Distribution audience |
| Should alternative virtual-audio products be supported/documented? | Before broader distribution | A tested alternative route |
