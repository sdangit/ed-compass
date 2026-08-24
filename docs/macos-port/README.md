# macOS port workspace

This directory records the investigation and implementation of a native macOS
version of ED Compass. Runtime feasibility phases 0–5 and the Phase 6 local
Apple Silicon package have passed. Work remains local while publication,
Developer ID signing, notarization, and broader support policy are undecided.

## Intended use

Elite Dangerous runs through CrossOver. A separately installed virtual audio
router, initially Rogue Amoeba Loopback, exposes the game's audio as a Core
Audio input device while monitoring it to the player's speakers or headphones.
ED Compass runs as an ordinary window on another display and analyzes that
input in real time.

The macOS version does not inspect the game process or window. The cockpit
overlay, focus tracking, and overlay positioning are out of scope. Journal
tailing remains in scope because it is ordinary file access and supplies the
location and action context needed to interpret a detection.

## Documents

- [`plan.md`](plan.md) — phases, gates, acceptance criteria, and fallback paths
- [`architecture.md`](architecture.md) — implemented boundaries and data flow
- [`test-log.md`](test-log.md) — evidence gathered during prototype runs
- [`decisions.md`](decisions.md) — decisions, assumptions, and unresolved items
- [`usage.md`](usage.md) — building, first launch, Loopback, journals, and files
- [`maintenance.md`](maintenance.md) — branches, remotes, upstream sync, and validation

## Working rules

1. Prove the riskiest assumption before integrating it.
2. Keep Windows behavior unchanged unless a cross-platform correction is
   independently justified.
3. Prefer target-specific backends behind the existing `CaptureMessage`
   interface over changes to the analysis engine.
4. Record observed evidence in `test-log.md`; do not mark a gate passed from
   compilation or unit tests alone.
5. Keep captured game audio, screenshots, CrossOver bottle contents, and local
   configuration out of Git. The repository's `.gitignore` already excludes
   common audio and image artifacts.
6. Make each phase independently reviewable and revertible.
