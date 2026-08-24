# Implemented macOS architecture

## Runtime data flow

```text
Elite Dangerous in CrossOver
             │
             ▼
  user-managed virtual audio device ──────► speakers/headphones (monitor)
             │
             ▼
 CPAL input callback on Core Audio
             │  interleaved samples
             ▼
 existing bounded CaptureMessage channel
             │
             ▼
 existing App and AnalysisEngine ─────────► existing egui main window
             │
             ├────────────────────────────► WAV/FLAC + JSON sidecar
             │
CrossOver journal directory
             │
             ▼
 existing JournalWatcher + event timeline
```

The virtual audio product owns application-specific routing and monitoring. ED
Compass sees only a normal Core Audio input device and does not need permission
to inspect CrossOver, Elite Dangerous, or any window.

## Backend boundary

The macOS capture implementation satisfies the protocol used by
the portable runtime:

- `CaptureMessage::Format` when a stream opens or its format changes
- `CaptureMessage::Audio` for interleaved samples
- `CaptureMessage::Gap` when elapsed audio time is known to be missing
- `CaptureMessage::Health` for best-effort bounded callback/queue telemetry
- `CaptureMessage::Error` for a terminal capture failure
- `CaptureMessage::Stopped` after capture ends

The analysis engine must not know whether samples came from WASAPI, Core Audio,
a file, or a synthetic source.

## Source layout

The smallest reviewable integration kept the platform implementations inside
the existing shared modules rather than reorganizing stable Windows code:

```text
src/audio/
    capture.rs       shared handle/messages plus target-specific capture modules
    device.rs        shared descriptor plus target-specific enumeration/selection
tools/
    audio-probe/     disposable Phase 0 executable
```

The target-specific modules remain internal to those files. Stable Windows code
was not moved solely for aesthetic symmetry.

## Device-selection policy

- Windows continues to select render endpoints opened in WASAPI loopback mode
  and must never fall back to a physical microphone.
- macOS selects an explicit input device because virtual audio routers present
  their streams as inputs.
- macOS must also avoid silently falling back to a physical microphone. A saved
  device that disappears should produce a waiting/error state and require an
  explicit replacement unless an unambiguous stable identity returns.
- User-facing names are labels, not assumed stable identifiers.

## Real-time rules

The Core Audio callback must remain bounded and predictable:

- no FFT or detector work;
- no file or journal I/O;
- no UI calls;
- no device enumeration;
- no blocking wait for a full queue;
- no routine per-sample logging;
- minimize allocation by transferring owned buffers or using a small pool when
  measurement shows it is useful.

The pipeline already accepts arbitrary supported stream formats and performs
analysis away from the capture callback. Prefer the virtual device's native
sample rate to adding resampling before it is proven necessary.

## GUI

The existing `egui`/`eframe` UI remains the application. On macOS:

- create only the ordinary main window;
- prefer wgpu, backed by Metal, subject to validation;
- exclude the cockpit overlay and all game-window polling;
- keep the display refresh independent of audio callback timing.

SwiftUI would duplicate the interface and require a Rust/Swift boundary around
large, frequently updated analysis data. It is not part of this port.

## Journal correlation

The journal is a timestamped evidence stream, not a window integration. Its path
may point anywhere readable, including inside a CrossOver bottle.

The existing derived state remains useful, but precise correlation should also
retain a bounded sequence of source events around detections. Sidecars should
carry source timestamps and enough journal identity to reconstruct the match.
Virtual-routing latency must be measured and recorded rather than assumed.

## Packaging and permissions

`packaging/macos/package.sh` produces an ad-hoc-signed Apple Silicon app with an
`Info.plist`, the original icon, credits, license, and an audio-input usage
description. Settings live in Application Support; captures and exports live in
the user-confirmed capture library. Screen Recording and Accessibility
permissions are not requested because the scoped app does not inspect windows
or screens. Developer ID signing, notarization, and non-Apple-Silicon builds
remain distribution decisions rather than runtime requirements.
