# macOS audio probe

This directory is reserved for the Phase 0 feasibility probe described in
[`../../docs/macos-port/plan.md`](../../docs/macos-port/plan.md).

The probe is intentionally separate from ED Compass. Its only job is to prove
or disprove that a native Rust process can enumerate a user-selected virtual
Core Audio input and receive truthful, continuous samples routed from
Elite Dangerous running through CrossOver.

Do not import the ED Compass analysis engine or GUI here during Phase 0. Keep
the observable surface small: enumeration, explicit selection, negotiated
format, level/continuity metrics, optional WAV output, and clean error handling.

## Usage

From this directory, enumerate inputs first:

```sh
cargo run --release -- list
```

Capture by the exact ID printed above or by an unambiguous, case-insensitive
fragment of the device description:

```sh
cargo run --release -- capture --device "ED Compass Audio"
```

Record a bounded sample for listening and Phase 1 analysis:

```sh
cargo run --release -- capture \
  --device "ED Compass Audio" \
  --duration 30 \
  --wav /tmp/ed-compass-loopback.wav
```

The capture command never selects the default device implicitly. This is
deliberate: a missing virtual device must not cause the probe to record a
physical microphone.

Status lines report interval RMS and peak level, frame delivery, suspected
callback gaps, packets dropped by the probe's bounded diagnostic channel, and
stream errors. The final summary compares received audio duration with wall
time. A WAV is always written as interleaved 32-bit float samples regardless of
the source device's native sample representation.

## Gate evidence

Record results in
[`../../docs/macos-port/test-log.md`](../../docs/macos-port/test-log.md). Audio
files remain local and are excluded by the repository's `.gitignore`.
