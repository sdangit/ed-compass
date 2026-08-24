# ED Compass fork instructions

This fork is maintained through agentic coding sessions. Read
`docs/macos-port/maintenance.md` and `docs/macos-port/decisions.md` before
changing platform behavior, branches, packaging, or release policy.

## Branches and remotes

- `upstream` is the original `kay-dutch/ed-compass` repository and is
  fetch-only locally. Never push to it.
- `origin` is `sdangit/ed-compass`.
- Keep `main` an exact, fast-forward-only mirror of `upstream/main`; never add
  port commits there.
- `macos-port` is the tested Mac integration branch and the fork's GitHub
  default branch. Default does not mean upstream mirror; it means the branch a
  fresh clone and new fork-local pull request should use.
- Start non-trivial fixes, enhancements, and experiments from `macos-port` on a
  short-lived `fix/*`, `feature/*`, or `experiment/*` branch. Merge validated
  work back into `macos-port` without rewriting its published history.
- Merge updated `main` into `macos-port`; do not repeatedly rebase the
  long-lived port or cherry-pick routine upstream changes.

## Required agent workflow

1. Inspect the branch, remotes, worktree, and recent history before editing.
2. Fetch and integrate upstream before substantial new work when authorized;
   upstream is an active alpha and stale merges accumulate quickly.
3. Preserve unrelated user changes and Windows behavior.
4. Keep macOS capture behind the existing platform boundary. Do not add game
   window access, a cockpit overlay, a custom audio tap, or a SwiftUI rewrite
   without an explicit scope decision.
5. Update the macOS plan, decisions, test log, usage, or maintenance documents
   whenever their claims change.
6. Before a Mac integration commit, run:

   ```sh
   cargo test --all-targets
   cargo clippy --all-targets -- -D warnings
   packaging/macos/package.sh
   git diff --check
   ```

7. Ask for the relevant live smoke test when audio, journal, reconnect, export,
   first-launch, windowing, or packaging behavior changes.
8. Push only to `origin`, and only as part of an authorized GitHub workflow.
9. Keep CI green on both supported concerns: Windows regression checks and the
   native Apple Silicon test/package job. CI artifacts are temporary personal
   build outputs, not supported releases.

## Product and release policy

- This is currently a personal-use Apple Silicon port of an early-alpha
  project. There is no user-support commitment and no supported public
  distribution.
- Local or GitHub releases may be created when explicitly requested for the
  owner's use. Current bundles are ad-hoc signed, not Developer ID signed or
  notarized, and must not be described as generally distributable.
- Do not promise compatibility beyond the validated machine and scope. Sleep,
  Intel/universal binaries, minimum macOS versions, alternate virtual-audio
  products, and precise route latency remain unclaimed unless tested.
- Reconsider formal public distribution, support, Developer ID signing, and
  notarization only after upstream reaches 1.0 and this branch has remained
  current with it, unless the owner explicitly changes that policy earlier.

## License and attribution

- The original project is generously released under the MIT License. Preserve
  the root `LICENSE`, the `license = "MIT"` package metadata, A Zimin's copyright
  and authorship, and the existing About/credits attribution.
- Every packaged or published source or binary artifact must continue to carry
  the MIT license notice. `packaging/macos/package.sh` copies it into the app as
  `Contents/Resources/LICENSE.txt`; do not remove that step.
- Never imply that the macOS port changes ownership of the original work or
  replaces the upstream author's attribution.
