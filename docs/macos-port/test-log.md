# macOS port test log

Record observed evidence here. Do not commit copyrighted game recordings,
personal journal contents, CrossOver bottle data, account information, or local
absolute paths. Refer to ignored local artifacts by a descriptive label.

## Environment template

- Date:
- Commit:
- Mac model:
- CPU architecture:
- macOS version:
- Rust version:
- CrossOver version:
- Elite Dangerous version:
- Virtual audio product and version:
- Virtual-device configuration:
- Output/monitor device:
- Probe or app command:

## Phase 0 — device enumeration

Status: **pass**

Baseline environment, 2026-08-23:

- Commit: `e482038` plus uncommitted macOS-port prototype files
- CPU architecture: Apple Silicon (`arm64`)
- macOS: 27.0, build 26A5416b
- Rust: 1.97.1 (Homebrew)
- Audio host: CoreAudio through CPAL 0.18.2
- Virtual audio product: Rogue Amoeba Loopback
- Virtual device: `ED Compass Audio`

Before Loopback was installed, the release probe enumerated one input,
`MacBook Pro Microphone`, with stable ID
`coreaudio:BuiltInMicrophoneDevice`. Its default format was 48 kHz, mono,
32-bit float; CPAL reported 44.1, 48, 88.2, and 96 kHz float configurations.
The probe was not permitted to open it because capture requires an explicit
selector and the physical microphone is irrelevant to the gate.

After Loopback installation, the user created `ED Compass Audio`, routed Brave
Browser into it, and ran the probe's bounded WAV capture. The probe opened the
virtual input and recorded the routed browser audio. Listening to the resulting
temporary WAV confirmed that it contained the expected source audio. This
proves the core path independently of Elite/CrossOver:

```text
application audio -> Loopback virtual input -> Core Audio/CPAL -> Rust -> WAV
```

| Check | Result | Evidence/notes |
|---|---|---|
| Core Audio input enumeration works | Pass | Built-in input and formats reported |
| Missing selector cannot fall back to microphone | Pass | Explicit nonexistent selector failed with an actionable error |
| Virtual device appears as an input | Pass | `ED Compass Audio` was selectable by the probe |
| Identity survives probe restart | Not recorded | Capture reliability did not depend on this; verify before persisting selection in Phase 3 |
| Supported formats are reported | Pass | Loopback negotiated 48 kHz stereo float32 |
| Device opens explicitly | Pass | Bounded capture opened `ED Compass Audio` |
| No physical microphone is selected implicitly | Pass | Capture requires `--device`; no default fallback exists |

## Phase 0 — live capture

Status: **pass**

| Check | Result | Evidence/notes |
|---|---|---|
| Idle behavior is truthful | Not run | |
| Routed application audio changes RMS/peak | Pass | Brave Browser used as the initial source |
| CrossOver/Elite audio changes RMS/peak | Pass in subsequent production tests | Phase 0 first proved routing with Brave; later GUI tests used Elite through CrossOver |
| Player can still hear monitored audio | Pass | Adding an output device to the Loopback configuration enabled concurrent monitoring |
| Bounded WAV is recognizable | Pass | Temporary WAV contained the expected Brave audio |
| Duration and frame count agree | Pass, qualitative | Full 30-minute recording played correctly; exact numeric comparison remains available from probe output |
| 30-minute run remains stable | Pass | Entire run recorded successfully |
| CrossOver restart behavior recorded | Pass | Capture remained active and resumed routed audio across the restart |
| Virtual-device removal behavior recorded | Pass | Disabling Loopback stopped capture immediately |

### Phase 0 conclusion

The feasibility gate passes. Loopback provides a usable, monitored, continuous
Core Audio input to the Rust probe. The stream is independent enough of the
source application that restarting CrossOver does not close it. Removing the
virtual device terminates capture promptly, which is safe and observable; the
production backend must turn this into a clear device-lost state and probe for
the same saved device when it becomes available again.

## Phase 1 — signal integrity

Status: **pass for the virtual-input and file-analysis path**

Four Phase 0 WAVs were found in temporary storage and inspected without
modification:

| Recording | Duration | Format | Overall peak | Overall RMS |
|---|---:|---|---:|---:|
| 1 | 30.005 s | 48 kHz, stereo, float32 | -21.91 dBFS | -38.01 dBFS |
| 2 | 30.005 s | 48 kHz, stereo, float32 | -18.35 dBFS | -35.14 dBFS |
| 3 | 1301.547 s | 48 kHz, stereo, float32 | +0.09 dBFS | -29.65 dBFS |
| 4 | 1800.011 s | 48 kHz, stereo, float32 | +0.03 dBFS | -34.97 dBFS |

FFmpeg reported zero NaNs, infinities, or denormal samples in every file and
negligible DC offset. The two long files each contained an isolated float sample
slightly above nominal full scale. Float capture preserves it rather than
clipping it, but the Loopback/source gain should be reduced slightly to leave
headroom in future sessions.

All four files loaded through ED Compass's existing headless `--input` path and
exported readable spectrogram PNGs. The 30-second files completed with no
detections, as is plausible for ordinary routed material. The 1301.5-second file
completed with 22 anomaly detections and the 1800-second file with 8; both
reported zero timeline gaps and completed normal periodicity, keying, and
structure analysis. Their exported spectrograms contained coherent audio
features separated by truthful silent regions rather than malformed or stale
sample patterns.

| Check | Result | Evidence/notes |
|---|---|---|
| Existing file-input path accepts probe WAV | Pass | All four completed offline analysis |
| Spectrogram is credible | Pass | Both long exports were visually inspected |
| No clipping or DC-offset concern | Conditional pass | Negligible DC; isolated samples at +0.03/+0.09 dBFS suggest lowering source gain |
| No repeated/stale buffers | Pass | Long spectrograms and playback show changing audio and genuine silence |
| Channel mapping is understood | Pass | Stereo interleaved float32 throughout |
| Audio/journal latency measured | Deferred | Required when real journal correlation is exercised in Phase 4 |

### Phase 1 conclusion

The probe's native output is directly consumable by the existing ED Compass
file loader and analysis engine. Sample representation, duration, channel layout,
and spectral content are suitable for integration. The small headroom issue is
a Loopback/source configuration recommendation, not a blocker. A known-signal
round trip can later strengthen detector validation, but the existing repository
acceptance tests already cover detector behavior independently of Core Audio.

## Regression baseline

On 2026-08-23, before macOS-port implementation, `cargo test --all-targets`
passed on macOS: 407 tests passed, 0 failed.

The isolated Phase 0 probe passed its two unit tests and strict Clippy checks on
2026-08-23. Its release build successfully enumerated the Core Audio host.

## Phase 2 — production live backend

Status: **pass**

The production audio abstraction now has a CPAL/Core Audio backend with exact
device selection, typed sample conversion to interleaved `f32`, bounded
nonblocking delivery, gap reporting, clean stop, and terminal stream-error
handling. WASAPI remains unchanged. macOS never falls back to the default input,
because that could silently open the physical microphone.

On 2026-08-23, `cargo test --all-targets` passed 403 tests and strict Clippy
passed with no warnings. The lower count than the pre-port baseline is expected:
four Windows-only selection tests are now correctly compiled only on Windows,
while two macOS-specific backend tests run on macOS.

The previously authorized Phase 0 probe still enumerated both inputs, including
`ED Compass Audio` at 48 kHz, stereo, float32. The newly built production
executable received an empty Core Audio enumeration and could not open the same
exact ID. This isolates the remaining gate to macOS privacy authorization for
the new executable, not Loopback routing or CPAL generally. Startup permits an
explicit ID to be attempted even when enumeration is withheld, while retaining
the no-default-input safety policy.

The user then ran the production executable directly. It completed the live
headless run with no capture or analysis warning; the only warning concerned
the old native-Windows journal default. This confirms that the selected
Loopback stream reached normal ED Compass runtime initialization. The macOS
journal default was subsequently changed to the standard `Elite Dangerous`
CrossOver bottle beneath the user's `Library/Application Support` directory.

| Check | Result | Evidence/notes |
|---|---|---|
| Production backend compiles | Pass | Native Apple Silicon debug build |
| Unit/regression suite | Pass | 403 passed, 0 failed |
| Strict Clippy | Pass | No warnings |
| Loopback opens in production executable | Pass | Direct user-run completed without a capture error |
| Live runtime state transitions | Pass | Only the unrelated missing-journal warning was emitted |
| Stop and device-loss lifecycle | Pass in subsequent Phase 5 test | Live Loopback removal, waiting, exact-device reattachment, and gap rendering passed |

## Phase 3 — native GUI integration

Status: **pass**

The existing egui device picker now persists changes to the active config file.
An explicit `--device` selection is also saved, making it suitable for the first
Mac launch. New macOS configs prefer the wgpu renderer, which uses Metal through
wgpu. The root window does not request transparency on macOS, the unsupported
overlay control is omitted, and overlay synchronization exits before polling
for a game window or creating a secondary viewport.

The full 404-test suite and strict Clippy pass after these changes.

The user completed those interactive checks on 2026-08-23. The native main
window opened without an overlay window; `ED Compass Audio` appeared in the
picker; live levels and the spectrogram responded to routed audio; the UI stayed
responsive and moved normally between displays; Export produced both audio and
spectrogram output; and a restart without `--device` restored the persisted
selection. All Phase 3 pass criteria are therefore satisfied.

## Phase 4 — journal discovery and correlation

Status: **pass**

On 2026-08-23 the user observed journal-derived information in the bottom area
of the main window while Elite Dangerous was running through CrossOver. This
confirms that the default CrossOver path is readable and that journal polling is
feeding the GUI.

Before the Phase 4 changes, the watcher chose the newest `Journal*.log` and
reduced selected events to current state—system and coordinates, body, music
track, supercruise, game-running state, and the last applied timestamp. Capture
sidecars stored only a snapshot of some of that state, without an event timeline
or source provenance.

The watcher now retains up to 4096 complete, valid journal events. Each retained
record includes its timestamp, event name, raw JSON, source journal filename,
and byte offset. Capture sidecars include the records found around an estimated
audio UTC interval, along with the search window and the explicitly recorded
virtual-route offset. The default offset is zero and is documented as
uncalibrated rather than asserted to be zero. Correlation is assembled when the
post-roll completes so journal actions immediately after an audio event are not
missed.

The main window now exposes the effective journal directory and applies and
persists edits without restarting or disturbing audio. Existing sidecars remain
readable because the new correlation object is optional. A source-offset and
time-window regression test passes; the full suite now contains 405 tests.

The remaining gate is a live experiment: perform a deliberate timestamped
in-game action near a manual or automatic capture, then verify the generated
sidecar references the correct newest journal file, source offset, raw action,
and plausible time separation.

The user completed this live experiment on 2026-08-23 and confirmed every
required field. A deliberate in-game action appeared in the generated capture
sidecar inside `journal_correlation`; the recorded audio interval, route offset,
search window, nearby raw journal event, newest source filename, and byte offset
were all present and correct. Phase 4 passes. The route offset remains explicitly
uncalibrated at zero; measuring its value and variance belongs to Phase 5.

## Phase 5 — reliability and performance

Status: **pass; sleep/wake explicitly deferred**

The Core Audio callback now publishes best-effort cumulative health once per
second without blocking. The GUI shows audio-queue full events and dropped
frames, with a tooltip for callback count, input and delivered frames, largest
callback, and device-gap frames. A full queue still becomes an explicit
timeline gap; telemetry itself may be discarded rather than competing with
audio.

The release throughput harness was changed from whole-run pre-rendering to
bounded ten-second batches. Its memory usage no longer scales with simulated
duration. Results on the baseline Apple Silicon Mac:

| Simulation | Wall time | Realtime factor | Approx. one-core cost | Accounted analysis memory |
|---|---:|---:|---:|---:|
| 1 hour, 48 kHz stereo, direction off | 116.77 s | 31x | 3.24% | 34.8 MB |
| 10 min, 48 kHz stereo, direction off | 12.15 s | 49x | 2.02% | 34.8 MB |
| 10 min, 48 kHz 7.1, direction on | 9.77 s | 61x | 1.63% | 227.0 MB |

The one-hour simulation exercised bounded histories, detectors, periodic
folding, and 36,000 UI-rate snapshots. The shorter figures are useful for
configuration comparison; differences at this size include run-to-run noise.
The memory figure covers the PCM ring and spectrogram tiers, not process/runtime
overhead. Direction finding is intentionally off in the supported Mac stereo
configuration.

The reliability matrix closed as follows; deferred entries are not part of the
supported active-session claim:

| Check | Result |
|---|---|
| Extended active Loopback/game session | Pass; no operational problem reported after rebuilt endurance run |
| Multi-hour idle Loopback session | Not claimed; bounded simulation and ten-minute live headless run passed |
| Queue drops stay at zero in ordinary use | Pass; telemetry showed no reported operational drops |
| CrossOver/game restart | Pass; audio capture remained active across game/CrossOver lifecycle testing |
| Loopback disable/re-enable and automatic attachment | Pass; exact device returned automatically with a truthful gap |
| macOS sleep/wake | Deferred outside the supported active-session use case |
| Minimize, cover, and move between displays | Pass in exercised GUI tests |
| Export/capture during active analysis | Pass for manual and automatic FLAC plus PNG export |
| Journal rotation during capture | Pass in unit and exercised live watcher paths |
| Process CPU and RSS remain bounded | Pass; user found both acceptable after GUI throttling, with stable memory |

### Live endurance observations and follow-up

During the 2026-08-24 live run, ED Compass remained operational while Elite,
CrossOver, and other Loopback sources were active. Disabling Loopback produced
a clear Core Audio `Device disconnected` state; the app stayed open, retried the
exact saved device, attached automatically after it returned, renegotiated 48
kHz stereo, and resumed analysis. Manual and automatic FLAC capture plus PNG
export succeeded during the run. The user subsequently ran the rebuilt binary
for an extended period and reported no further problems.

Activity Monitor showed approximately 60% of one core with Elite running and
38% after Elite quit while Brave continued supplying audio. RSS remained stable
at approximately 560 MB across that comparison. The stability is encouraging,
but both figures exceed the analysis-only baseline and point to presentation,
Metal/shared-GPU accounting, and CrossOver contention as the remaining costs.

Two optimizations follow from that evidence:

- macOS repaints are capped near 15 FPS and full waterfall CPU rebuild/upload
  near 4 FPS; audio capture and analysis cadence are unchanged;
- a reconnect within 30 seconds at the identical stream format preserves the
  analysis engine and inserts the outage as a timeline gap. A longer outage or
  changed format still resets safely.

The prior production build reset the spectrogram on every reattachment. The
new same-format continuity behavior first gained regression coverage, then
received the live disable/re-enable confirmation described below.

Live confirmation subsequently passed: disabling Loopback froze spectrogram
scrolling without closing the app; re-enabling the identical 48 kHz stereo
device preserved the prior image, shifted it left by the measured outage, and
rendered a blank gap for the missing interval. This is the intended truthful
timeline behavior and replaces the former full analysis reset.

Final live review found CPU and memory acceptable after the macOS presentation
throttling. Headless and GUI memory remained stable, queue/drop telemetry found
no reported operational problem, reconnect recovery passed, and capture/export
continued to work under use. Sleep/wake was deliberately not tested because it
is outside the expected session model. It remains an unverified platform edge
case, not a claimed pass and not a blocker for this personal-use port. Phase 5
passes on the exercised requirements.

Renderer comparison on the same Mac found approximately 560 MB in Activity
Monitor with the default wgpu/Metal renderer and approximately 750 MB with
Glow/OpenGL. Glow is therefore rejected as a memory optimization; wgpu remains
the supported Mac default. The difference also confirms that a material part of
the reported footprint belongs to presentation/driver resources rather than
the explicitly accounted analysis buffers.

A subsequent ten-minute headless live run retained Core Audio, journal polling,
and the full analysis/detection pipeline while removing egui and the GPU
presentation path. Activity Monitor fell as low as approximately 3% CPU and
70 MB. This closely matches the release analysis benchmark and rules out audio,
journal correlation, and detector history as the source of the GUI process's
larger footprint. The remaining roughly 490 MB and most GUI-mode CPU belong to
windowing/rendering/driver resources and waterfall presentation. Because the
GUI footprint remained stable during endurance testing, this is overhead rather
than evidence of unbounded application-state growth.

## Phase 6 — first-launch and storage integration

The macOS configuration default now uses
`~/Library/Application Support/ED Compass/config.toml`. A new installation
opens a one-time setup screen pre-filled with the detected `ED Compass Audio`
input, `~/Documents/ED Compass` as the capture library, the discovered
CrossOver journal directory, and System appearance. Confirmation creates
`Captures` and `Exports` below that library. Subsequent manual exports remain a
single immediate action and use timestamped names; no per-export dialog was
introduced. System, Light, and Dark remain selectable after setup. The compact
setup layout and its Continue transition were corrected after live review; the
user confirmed that setup now enters the analyzer without reopening or hanging
the already-active Core Audio device. The full suite contains 407 tests,
including a regression for retaining that active stream.

The repeatable `packaging/macos/package.sh` flow subsequently produced
`dist/ED Compass.app` for Apple Silicon. The bundle reports version 0.4.5 and
identifier `com.edcompass.ed-compass`, contains the original icon, standard
About-panel attribution, MIT license, and the virtual-audio input purpose
string, and passes strict code-signature verification with an ad-hoc local
signature. Launch Services opened this exact bundle and its arm64 process
remained running, validating Finder-style startup and working-directory
independence.

The user completed the final bundle review on 2026-08-24: **About ED Compass**
was present, live Loopback audio analysis worked, manual captures and
spectrogram exports were written successfully, and CrossOver journal access
showed the expected current context. The local Apple Silicon package gate
therefore passes. Developer ID signing, notarization, broader architecture and
OS support, and publication remain separate distribution decisions.

## 2026-08-25 — waterfall experiment endurance profile

After roughly two hours of live Elite/Loopback use on the multichannel
waterfall experiment, the process remained bounded at about 1.2 GB RSS and
roughly 30–36% of one CPU core. `vmmap` attributed most of the footprint to the
expected multichannel analysis histories plus Metal/graphics allocations; the
process had peaked near 1.9 GB, consistent with the already-recorded synchronous
capture/export buffer debt, but there was no evidence of continuing growth.

A 15-second stack sample attributed about 16% of one core to repeated waterfall
rasterization and about 2% to repeated long-term periodicity derivation. The
follow-up changes therefore leave a pinned historical main viewport cached
until its view, display options, or layout changes, and derive periodicity only
when the one-Hz long-term tier gains a row. Live views, overview strips,
detector overlays, and analysis cadence remain active.

The intrusive stack sample itself suspended the process long enough for Core
Audio to tear down its AUHAL stream. This exposed a recovery case that ordinary
Loopback disable/re-enable testing had not: CPAL can lose all input callbacks
without promptly reporting another stream error. The Mac capture worker now
uses a lock-free callback heartbeat, declares a stream dead after three seconds
without callbacks, and retries the exact configured device through the existing
reconnect path. Stream format is announced only after Core Audio accepts the
replacement stream. This recovery and the profile-derived optimizations require
a fresh live endurance/reconnect confirmation; the automated validation result
is recorded with the experiment commit.

The same review found and corrected an unrelated pre-existing detector bug:
the prior Landscape state was captured after assigning the new state, making
its rising edge impossible and preventing Landscape alone from initiating its
intended automatic capture.
