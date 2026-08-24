# Post-port roadmap proposals

This document preserves product ideas discussed after the native Mac port became
usable. They are proposals, not committed scope, release promises, or authority
to begin implementation. Each item must still receive its own branch, prototype,
go/no-go gate, and explicit promotion decision. The working `macos-port` branch
remains the validated personal-use baseline.

Although these ideas arose from the Mac workflow, their signal-processing and
rendering code should remain platform-neutral where practical. macOS-specific
code should be limited to genuinely platform-specific integration.

## Isolated visualization experiments

### Motivation

The current application is intentionally centered on live capture, a mono
spectrogram, detectors, and a ship-relative bearing. Eight-channel capture now
works through a Loopback Pass-Thru device, which creates useful opportunities
for richer inspection without changing the Core Audio backend.

Candidate experiments include:

- selectable linear and logarithmic frequency scaling;
- genuine per-channel level or spectral views;
- selectable stereo-pair, mid/side, phase, and correlation views;
- phase-vocoder or other pitch-preserving time expansion;
- frequency-hop tracking and visualization;
- spectrogram watermark or drawn-structure enhancement and extraction.

These do not constitute one feature. Each has different data, performance, and
validation requirements and should be investigated independently.

### Development method

Use short-lived branches from the current, synchronized `macos-port` branch:

```text
macos-port
├── experiment/log-frequency
├── experiment/mid-side
├── experiment/phase-vocoder
└── experiment/hopping-analysis
```

One branch should answer one bounded question. Prefer new source modules and a
small registration or call site over broad edits to upstream UI and pipeline
files. Do not combine speculative visualizations in a single branch, and do not
merge an experiment merely because it compiles.

For each experiment:

1. State the research question and measurable pass/fail evidence.
2. Identify the minimum data it needs and where that data is produced.
3. Measure release-mode CPU, resident memory, allocations, and UI responsiveness.
4. Verify that disabled experiments impose negligible ongoing cost.
5. Keep work out of the Core Audio callback and bounded on the analysis path.
6. Exercise saved reference captures before relying on uncontrolled live audio.
7. Run normal cross-platform tests and CI before promotion.
8. Record why an abandoned experiment failed before deleting its branch.

Successful work should be cleaned up on a `feature/...` branch and merged into
`macos-port`. Whether it is later proposed upstream is a separate decision.
Keeping the implementation portable and its commits focused preserves that
option without making upstream contribution a requirement.

### Let abstractions emerge

“Plug-in” means isolated, statically compiled modules for now, not dynamically
loaded libraries. Rust has no stable native ABI for independently compiled
Rust plug-ins, and dynamic loading would add version coupling, failure
isolation, signing, packaging, and eventual notarization problems.

Do not design a general visualization framework before multiple prototypes
demonstrate a common contract. The existing `AnalysisSnapshot` is suitable for
some renderers but does not publish raw PCM, per-channel spectra, complex phase,
or arbitrary history. Prematurely exposing all engine internals would create a
large, unstable API and unnecessary copying, especially with eight channels.

The expected progression is:

1. Implement the first visualization as an isolated module.
2. Implement a second with its own minimum interface.
3. Compare their actual lifecycle, data, configuration, and rendering needs.
4. Extract only demonstrated common behavior into an internal trait or registry.
5. Allow expensive analysis products to be requested only by enabled consumers.

A future internal interface might separate read-only visualization data,
declared data requirements, and egui drawing, but its exact shape is deliberately
deferred until implementation evidence exists.

### Suggested order

1. **Log-frequency rendering:** lowest architectural risk and likely able to
   reuse existing spectrogram history.
2. **Per-channel and mid/side inspection:** validates multichannel semantics and
   establishes the first need for optional channel-specific summaries.
3. **Frequency-hop and watermark analysis:** initially operate over saved or
   bounded time-frequency history.
4. **Phase-vocoder/time stretching:** begin offline; real-time playback would
   introduce an output transport, latency, and buffering responsibilities that
   the live detector currently avoids.

The current upstream channel panel is a layout/presence display, not a true bank
of independent level meters. Preserve upstream behavior until an experiment is
explicitly approved; Loopback remains the available per-channel diagnostic in
the meantime.

## ED Compass Lab companion application

### Motivation and boundary

ED Compass captures, analyzes, visualizes, and writes evidence, but it does not
provide a document-oriented workflow for the artifacts it creates. The user
must currently browse FLAC/WAV, JSON, and PNG files with unrelated tools.

Propose a separate companion application in this repository:

```text
ED Compass       live capture, detection, visualization, and export
ED Compass Lab   browse, replay, compare, align, and investigate artifacts
```

The separation is intentional. Live capture must stay bounded and responsive
beside CrossOver, while offline work may decode several recordings, regenerate
spectrograms, cache thumbnails, perform time stretching, and retain comparison
state. Heavy or failed offline work must not interrupt a capture session.

The Lab should use Rust and egui initially and share the repository's audio,
sidecar, journal, and analysis types. Keeping it in this repository allows
capture-format and reader changes to remain atomic and covered by one CI graph.
It should be a separate process/binary rather than another heavy panel inside
the live application. Packaging details for a second user-facing Mac app are a
later phase, not a prerequisite for a source-level prototype.

### Canonical evidence model

Treat audio plus its JSON sidecar as canonical evidence. Treat exported PNGs as
presentations that may still be browsed and compared, but not as substitutes
for the source audio:

- PNGs discard phase and channel information;
- historical exports may use different FFT, dimensions, gain, frequency range,
  background subtraction, or color mapping;
- images with incompatible render settings can appear aligned while representing
  different time-frequency geometry;
- audio can be re-rendered consistently, filtered, slowed, and reanalyzed.

Sidecar/schema evolution should eventually retain enough provenance to recreate
a view: application and schema version, sample rate and channel layout, FFT and
hop sizes, frequency range and scale, gain/color mode, capture timeline origin,
and journal correlation data. Old or incomplete sidecars must remain readable
and visibly identify unavailable provenance rather than inventing it.

### Proposed stages

#### Lab 0 — Corpus and format audit

- Inventory representative automatic captures, manual captures, exports, and
  their sidecars without committing private artifacts.
- Document filename and timestamp relationships and missing-data cases.
- Define compatibility fixtures with synthetic or legally redistributable data.
- Confirm that current FLAC/WAV and JSON can be read independently of the live
  application.

#### Lab 1 — Artifact library

- Scan the configured `Captures` and `Exports` directories.
- Present list and thumbnail views with date, system, band, duration, score,
  bearing, and capture reason where available.
- Filter and sort by time, system, frequency, score, and manual/automatic origin.
- Play, pause, seek, loop, and reveal the selected source in Finder.
- Render a canonical spectrogram from selected audio.
- Handle missing, corrupt, orphaned, and older-version files without preventing
  the rest of the library from opening.

Start with an in-memory directory scan and filesystem thumbnail cache. Do not
add a database until measured library size or startup/search performance
justifies one.

#### Lab 2 — Comparison workspace

- Select two or more recordings or exports.
- Re-render audio using identical analysis and display parameters.
- Synchronize pan and zoom across views.
- Align timelines with a user-controlled offset slider.
- Overlay with adjustable opacity and offer blink, additive/max, and difference
  comparisons where their meaning is defined.
- Align from event onset, selected tone, cross-correlation, or journal timestamp.
- Support synchronized playback and bounded loop regions.
- Save a non-destructive workspace that references, but never rewrites, sources.
- Warn when direct PNG comparison uses incompatible or unknown provenance.

#### Lab 3 — Offline analysis bench

Use the Lab as the initial proving ground for expensive or exploratory work:

- logarithmic and alternate spectrogram projections;
- channel isolation and conventional-layout verification;
- stereo-pair, mid/side, phase, and correlation tools;
- pitch-preserving slow playback and phase-vocoder inspection;
- frequency-hop tracks and repeated-pattern alignment;
- watermark/drawn-structure enhancement and extraction;
- comparison reports or derived exports with explicit provenance.

Only mature views with a demonstrated need during live play should later be
considered for the capture application.

#### Lab 4 — Optional live-app handoff

After the standalone workflow is useful, consider lightweight actions in ED
Compass such as **Open Capture Library**, **Inspect Latest Capture**, or **Open
in Comparison**. These should launch the companion with a path or stable
identifier; they should not embed its processing or memory in the live process.

### Storage policy

User evidence remains visible under the configured Documents library. Private,
recreatable application state belongs in Application Support:

```text
~/Documents/ED Compass/
├── Captures/
└── Exports/

~/Library/Application Support/ED Compass Lab/
├── thumbnails/
├── workspaces/
├── preferences.toml
└── index.sqlite        # only if later justified
```

Favorites, notes, alignment offsets, and workspaces should be non-destructive.
The design must define whether user-authored notes are portable Documents or
private application state before implementing them.

### Initial implementation strategy

Begin on `experiment/ed-compass-lab` from a freshly synchronized
`macos-port`. Avoid a workspace-wide restructuring until the prototype proves
which existing modules need to be shared. Prefer a small second binary using
the current library, then extract common code only when concrete duplication
appears.

The first go/no-go milestone is deliberately narrow: open a real capture and
sidecar, show searchable metadata, render a canonical spectrogram, and provide
responsive audio scrubbing without affecting a concurrently running ED Compass
session. Comparison and advanced DSP follow only after that foundation passes.

## Relationship between the proposals

The two roadmap items reinforce one another but are not coupled. Visualization
experiments can remain live-app modules, while the Lab provides a safer home
for offline, CPU-intensive, or uncertain techniques. Shared algorithms should
live below either UI when their interfaces become clear; the Lab must not become
a reason to expose platform capture internals or destabilize the live engine.

No implementation branch should silently broaden either proposal. Record newly
accepted scope, rejected assumptions, and promotion evidence here or in a
dedicated successor design document before merging it into `macos-port`.
