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
analyzer. Appearance can later be changed among System, Light, and Dark in the
main window. The renderer is selected automatically and is not a user setting.

The default journal discovery searches below:

```text
~/Library/Application Support/CrossOver/Bottles/Elite Dangerous/
    drive_c/users/<bottle user>/Saved Games/
    Frontier Developments/Elite Dangerous
```

The bottle-user directory is detected rather than hard-coded. The journal path
remains editable in the main window. An unavailable journal is reported but
does not stop audio analysis.

## Files

Private settings:

```text
~/Library/Application Support/ED Compass/config.toml
```

Default user-visible artifacts:

```text
~/Documents/ED Compass/
├── Captures/   # FLAC/WAV evidence and JSON journal sidecars
└── Exports/    # timestamped spectrogram PNGs
```

The main **Export** action immediately preserves recent audio and a spectrogram;
it intentionally does not interrupt a surprise-signal workflow with a Save
dialog.

## Known boundaries

- No macOS cockpit overlay or access to the Elite Dangerous window.
- Sleep/wake recovery is unverified; active-session device loss/recovery is
  tested.
- Loopback is the only virtual-audio product currently validated.
- The current app is Apple Silicon only.
- Precise virtual-route latency remains uncalibrated; sidecars record the zero
  offset as uncalibrated rather than asserting exact synchronization.
