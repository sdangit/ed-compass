# Runtime performance investigation log

This is a running evidence log for intermittent sluggishness, stalls, hangs,
and possible eventual lockups in the long-running macOS app. It intentionally
separates observations from diagnoses and proposed fixes. Add new samples here
before restructuring the affected code so that short-lived and long-lived runs
can be compared against the same baseline.

The local `sample` reports named below are temporary diagnostic artifacts and
are not committed. The useful evidence from each report is summarized here.

## Observation protocol

For each observation, record as much of the following context as practical:

- date and local time, app PID, executable identity, launch time, and uptime;
- whether the app was responsive, sluggish, stalled, or fully hung before any
  diagnostic command was run;
- selected device and whether it remained present, selected, and actively
  receiving audio (device presence and silence are different conditions);
- app status, channel mode, time and frequency viewport, frequency scale,
  palette, and whether the viewport was live-following or pinned;
- recent user actions, captures, exports, device changes, journal activity,
  sleep/wake, and how long the app had been unattended;
- several CPU and resident-memory observations rather than one instantaneous
  value;
- a 10–15 second stack sample and its dominant inclusive and leaf stacks;
- Core Audio logs from before the sample when audio continuity is relevant;
- any behavior immediately after sampling that could be an observer effect.

Whenever possible, verify that the sampled executable UUID matches the bundle
on disk. Do not infer a memory leak from one peak-footprint value, or infer a
deadlock merely because the UI is temporarily blocked by expensive synchronous
work.

### Important sampling caveat

Attaching `/usr/bin/sample` has previously caused a Core Audio overload and
stream reconnect. A device or audio event immediately after attachment is not
evidence of the original failure unless logs show that it began beforehand.
Similarly, a device that is present but silent must not be described as lost.

## Current findings

The evidence currently supports two independent main-thread costs:

1. Epoch-folding search causes periodic, multi-second, near-full-core bursts as
   the long-term history fills.
2. Rebuilding the primary waterfall and overview images causes a persistent
   roughly 30% one-core rendering load in the observed view.

Neither finding yet explains an eventual full lockup. The first can create a
visible stall, while the second reduces available UI-thread headroom and may
amplify another problem. Capture/export blocking and long-run memory behavior
remain separate concerns.

## Observations

### 2026-08-28 — Long-running instance, sample caught epoch folding

Context:

- PID: `14674`
- launch time: 19:28:55 local
- observed uptime: approximately two hours
- CPU: approximately 35% baseline, with repeated 100–102% bursts lasting about
  two seconds
- resident memory: varied between approximately 0.9 and 1.2 GB
- sample physical footprint: 598.4 MB; reported peak: 1.8 GB
- sample artifact: `/tmp/ed-compass-repeat.sample.txt`
- loaded executable UUID matched the executable on disk

Stack evidence:

- main-thread samples: 5,781
- `App::pump`: 5,112
- `AnalysisEngine::push_interleaved`: 4,859
- `fold::fold`: 4,405, approximately 76% of all sampled main-thread time and
  approximately 90% of live processing in that sample
- waterfall `render_rgb_many`: 199

Interpretation:

The sample definitively caught synchronous epoch-folding work on the main UI
and analysis-pump thread. The coarse and refined branches in the stack are two
stages of one search. While this runs, the UI, analysis drain, journal polling,
and detector checks cannot progress. Core Audio capture remains on its own
thread, so messages can accumulate behind the blocked consumer.

The search runs every 30 seconds over as much as one hour of 1 FPS, 256-band
history. A controlled release benchmark of the unchanged `fold::search` took
approximately 101 ms for 300 frames, 511 ms for 900, 888 ms for 1,800, and
1.483 seconds for 3,600. Per-candidate allocations in `fold()` and
`Folded::sharpness()` also appeared in the sample. This explains the recurring
bursts but not the persistent baseline CPU.

No code was changed. Moving the work to a worker was deferred pending a coupling
audit because folded results feed detection, automatic capture, sidecars,
overlays, and CLI output, and stale results must not cross device rebuilds,
direction changes, or timeline gaps.

### 2026-08-28 21:38 — Relaunched instance, sample caught steady rendering

Context:

- PID: `30324`
- launch time: 21:31:10 local
- sample taken after approximately seven minutes
- CPU before/after sampling: approximately 27–33%
- resident memory after sampling: approximately 819 MB
- sample physical footprint: 917.7 MB; reported peak: 1.5 GB
- sample artifact: `/tmp/ed-compass-relaunch.sample.txt`

Stack evidence from the 10-second sample:

- `waterfall::render_rgb_many`: 1,386 leaf samples, approximately 33% of one
  core over the sample
- `Palette::color`: 418 leaf samples, approximately 10% of one core, as part of
  per-pixel waterfall rendering
- primary waterfall rendering: roughly 970 samples in its UI call path
- overview rendering: roughly 216 samples in its UI call path
- `AnalysisEngine::push_interleaved`: 38 leaf samples
- epoch folding: absent from this sample

Interpretation:

This sample caught steady image generation rather than the periodic fold burst.
Most active repaint time was spent regenerating waterfall pixels, including a
second render for the overview, plus large-buffer allocation/clearing and
conversion to egui's `ColorImage`. Palette settings UI was not involved; the
measured palette cost is the color calculation inside the per-pixel loop.

The Core Audio capture callback and waiting capture/disconnect threads looked
normal in this sample. There was no stack evidence of a missing device, dead
capture thread, blocked callback, or reconnect loop. The fresh run's high peak
footprint warrants comparison over time, but one retained-allocation peak does
not establish a leak.

No code was changed.

## Open questions and next comparisons

- Does resident memory reach a stable plateau for a fixed view, or continue to
  grow across several hours and repeated fold searches?
- Do palette, channel comparison/group modes, window dimensions, and overview
  visibility change render cost in proportion to rendered pixel count?
- Is the waterfall regenerated at the repaint heartbeat even when no new
  display frame is available or the viewed historical slice is pinned?
- When the UI becomes visibly sluggish, is it inside folding, rendering,
  capture/export, filesystem/journal polling, GPU submission, or a different
  call path?
- Does a reported full hang still service the Core Audio callback, repaint
  heartbeat, and event thread, or is there an actual wait cycle/deadlock?
- Are long-run memory peaks CPU-side image buffers, queued audio, retained GPU
  textures, fold scratch allocations, capture/export snapshots, or normal
  allocator high-water behavior?
- Can the suspected lockup be correlated with a fold boundary, automatic
  capture, manual export, device switch, journal rollover, display change, or
  system idle/sleep transition?

Do not optimize from the aggregate percentages alone. The next useful samples
are one during an observed periodic CPU burst, one during ordinary responsive
operation with the same view, and one taken before interacting with an app that
has become genuinely sluggish or hung.
