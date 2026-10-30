# macOS build and use

## Requirements

- Apple Silicon Mac
- Rogue Amoeba Loopback (the tested virtual-audio router)
- Elite Dangerous running through CrossOver for the intended game workflow
- Rust 1.98 or newer when building locally

## Loopback

Create a Loopback device named `ED Compass Audio` and add Elite Dangerous or
its CrossOver audio process as a source. Add the speakers or headphones you use
as a monitor/output inside Loopback; otherwise ED Compass can receive the audio
while the player hears nothing. Leave modest gain headroom because test captures
found isolated samples just above nominal full scale.

ED Compass opens the selected virtual input by its exact Core Audio identifier.
It deliberately never substitutes the Mac's physical microphone. If Loopback is
disabled, the app waits for that same device and reattaches when it returns.

## Build the application

From the repository root:

```sh
packaging/macos/package.sh
```

The result is `dist/ED Compass.app`. It is native arm64 and ad-hoc signed for
local use; it is not Developer ID signed or notarized for redistribution.

## First launch

The one-time setup screen pre-fills:

- `ED Compass Audio`, when that input is present;
- `~/Documents/ED Compass` as the user-visible capture library;
- the journal directory in the standard CrossOver bottle named
  `Elite Dangerous`;
- System appearance.

Review the values and select **Continue**. The same window expands into the
analyzer. Appearance can later be changed among System, Light, and Dark in
**Settings**. The renderer is selected automatically and is not a user setting.

The default journal discovery searches below:

```text
~/Library/Application Support/CrossOver/Bottles/Elite Dangerous/
    drive_c/users/<bottle user>/Saved Games/
    Frontier Developments/Elite Dangerous
```

The bottle-user directory is detected rather than hard-coded. The journal path
remains editable in **Settings > Journal**. An unavailable journal is reported
but does not stop audio analysis.

## Settings

On macOS, choose **ED Compass > Settings…** in the application menu or press
**Command-,**. Settings open in a separate compact window, so the analyzer
remains visible and interactive. General, Audio, Waterfall, Journal, Detection,
and Storage are tabs across the top; the window adjusts to the selected tab
instead of reserving a large empty settings canvas. **Apply** validates and
saves the draft; **Cancel** or closing the window discards it. Changing the
input device restarts capture, and changing direction finding clears analysis
history and relearns the background. Those effects are called out in the
window.

Startup currently uses **Last used**. A normal app exit saves the main view's
time span, physical frequency bounds, Linear/Logarithmic scale, channel view,
and Raw/Excess mode. It never restores a historical timeline position: startup
always returns to live. Named view presets remain separate from this automatic
startup state.

Journal correlation, its directory and route offset, timing-marker visibility,
detector switches, direction finding, appearance, and the selected input device
are also persisted. Capture and export locations are shown read-only for now;
moving the capture library remains deferred until the capture/export worker
coupling is separated.

## Files

Private settings:

```text
~/Library/Application Support/ED Compass/config.toml
```

The run log, `ed-compass.log`, lives beside `config.toml`; the previous run is
kept as `ed-compass.log.old`. Each run stops writing after about 20 MiB. With
`--config`, the log follows that configuration file's directory.

Upstream's per-program audio capture and in-app updater are Windows features.
The Mac build continues using the selected Loopback input and does not check
for or install upstream Windows releases. Update it by synchronizing the fork
and rebuilding the local app bundle.

Default user-visible artifacts:

```text
~/Documents/ED Compass/
├── Captures/   # FLAC/WAV evidence and JSON journal sidecars
└── Exports/    # timestamped spectrogram PNGs
```

The main **Export** action immediately preserves recent audio and a spectrogram;
it intentionally does not interrupt a surprise-signal workflow with a Save
dialog.

## Waterfall view

The **Scale** dropdown beside the time and frequency controls changes the main
waterfall between **Logarithmic** and **Linear** frequency projection. Logarithmic
is the default; Linear is most useful after narrowing the displayed frequency
range. Switching is display-only and does not restart capture or alter
detection.

The overview remains one Combined, Full spectrum, Log scale context strip. Its
viewport box still shows and controls the physical time/frequency slice even
when the main waterfall is Linear. Named view presets save the selected scale
along with their time and frequency bounds. The current Export action remains a
canonical full/logarithmic export rather than an export of the navigated view.

**Settings > Waterfall > Palette** selects **ED Compass**, **Inferno**,
**Viridis**, or **Grayscale**. The preview shows each complete quiet-to-strong
ramp. Palette is global and persisted independently of named view presets. It
changes the overview, main waterfall lanes, and exported PNG pixels without
changing FFT analysis or detector behavior; axes and semantic annotations keep
their established colours.

## Known boundaries

- No macOS cockpit overlay or access to the Elite Dangerous window.
- Sleep/wake recovery is unverified; active-session device loss/recovery is
  tested.
- Loopback is the only virtual-audio product currently validated.
- The current app is Apple Silicon only.
- Precise virtual-route latency remains uncalibrated; sidecars record the zero
  offset as uncalibrated rather than asserting exact synchronization.
