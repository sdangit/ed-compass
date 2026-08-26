# Technical debt

This register records known structural problems that are not safe to treat as
isolated implementation chores. An entry here is not a commitment to solve it
immediately; it preserves the coupling and acceptance criteria that must be
understood before work starts.

## Capture and export mix policy, snapshots, encoding, retention, and UI state

Status: **deferred after coupling audit (2026-08-25)**

### Symptom

Automatic captures and the manual Export action perform PCM copying, FLAC/WAV
encoding, sidecar serialization, high-resolution spectrogram rendering, file
writes, and retention scans synchronously on the main UI/analysis-pump thread.
The Core Audio thread continues delivering into its bounded queue, but the app
does not drain that queue while the synchronous work runs. A long operation can
therefore freeze the UI and eventually cause queue pressure, dropped frames, or
a reported timeline gap.

This is not merely a missing worker thread. The current boundaries contain
several single-responsibility violations:

- `App` owns live capture draining, analysis, trigger admission, post-roll
  scheduling, ring extraction, journal correlation, event mutation, recent-save
  state, disk-usage caching, and reconnect behavior.
- `CaptureWriter` owns trigger policy and its mutable cooldown/hourly state as
  well as naming, encoding, sidecar construction, filesystem publication,
  capture counters, destination configuration, and retention enforcement.
- The UI owns export naming and destination, snapshots the live spectrogram,
  invokes both audio and PNG writes sequentially, and constructs the combined
  success/failure message.
- Retention enforcement, disk-usage scans, and the destructive “erase
  recordings” operation independently access the same directories as writers.

### Coupling that a worker design must preserve or remove

- Capture admission currently mutates/prunes rate-limit state, but an accepted
  capture is counted only after a successful write. Asynchronous requests need
  an explicit reservation/in-flight model or multiple detections can pass the
  cooldown and hourly limits before the first completion.
- Detection event records are currently inserted after the write completes.
  Async completion needs a stable job identity and an explicit pending,
  succeeded, or failed state; `captured_to: Option<PathBuf>` cannot distinguish
  “still saving” from “not captured” or “write failed.”
- Worker inputs must be owned snapshots. Neither the PCM ring, analysis engine,
  spectrogram histories, journal watcher, nor UI state may be borrowed or
  mutated from the worker.
- Copying selected PCM and spectrogram history on the main thread remains
  necessary unless those live data structures are redesigned. Moving encoding
  does not make snapshot creation free.
- A 150-second, 48 kHz, 7.1 float PCM snapshot is roughly 230 MB. FLAC encoding
  adds a similarly sized integer buffer plus encoded output. A worker and even
  a small job queue can multiply peak memory, so bounding by job count alone is
  insufficient without an explicit memory policy.
- Export is a compound operation whose audio and PNG results produce one UI
  outcome. Automatic captures, manual exports, cleanup, and shutdown need a
  defined ordering and cancellation policy.
- Published files currently use their final names while being written. Once
  writes are concurrent with scans or cleanup, audio, JSON, and PNG output must
  use temporary files plus atomic rename so other operations never observe a
  partial artifact.
- Retention, disk accounting, and erase-all must be serialized with publication
  or redesigned around a shared filesystem coordinator.
- Capture destination and relevant format/budget settings must be snapshotted
  per job or updated through the worker in a defined order.
- Shutdown must decide whether to finish and join outstanding writes (possibly
  delaying exit) or cancel them (possibly losing a user-requested artifact).
- `App::last_capture` and `App::recent_capture()` say Export can reuse a recent
  recording, but the current Export path does not consult them. This stale
  contract must not silently become new behavior during the refactor.

### Safe target boundary

If this debt is addressed, prefer one long-lived, tightly bounded artifact
worker rather than detached threads per operation:

1. The main thread performs admission and creates an immutable owned job with
   PCM, spectral history, format, render options, detection/periodicity data,
   journal correlation, game state, paths, and timestamps.
2. The worker owns encoding, atomic publication, sidecar writing, and retention.
3. Completion messages carry stable job IDs and typed audio/PNG outcomes back
   to the main thread.
4. The main thread alone updates event records, counters, recent-save state,
   disk-usage invalidation, and UI status.
5. Cleanup and other mutations of artifact directories are coordinated through
   the same serialization boundary.
6. The UI exposes a truthful `Saving…`/busy state and does not admit another
   memory-heavy manual export when doing so would exceed the chosen bound.

Before implementation, separate trigger admission from filesystem writing and
define the job lifecycle, memory budget, failure semantics, atomic-publication
scheme, cleanup ordering, and shutdown behavior. Simply moving the existing
methods to a thread would preserve the SRP violations while adding races.

### Validation required

- Manual Export and automatic capture while audio is continuously flowing.
- Queue health and truthful gap reporting during worst-case 7.1 FLAC plus PNG.
- Concurrent trigger arrival, cooldown/hourly enforcement, and completion order.
- Write failure, full disk, worker failure, and app shutdown mid-save.
- Destination or relevant configuration changes with a job in flight.
- Disk accounting, budget eviction, and erase-all around in-flight publication.
- Peak resident memory with the largest supported PCM and PNG snapshots.
- macOS live smoke test plus Windows regression coverage.
