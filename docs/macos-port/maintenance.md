# Maintenance workflow

## Operating policy

Development and repository maintenance are performed through agentic coding
sessions. The agent is expected to inspect current Git state, execute the branch
and synchronization workflow below, make scoped changes, run validation, update
the evidence documents, and report exactly what was committed or pushed. The
root `AGENTS.md` makes these constraints available automatically in future
sessions.

The fork is currently a personal-use continuation of an early-alpha project.
There is no support commitment and no supported public distribution. A GitHub
fork and occasional downloadable builds are storage and personal deployment
mechanisms, not a promise of compatibility, maintenance service, or readiness
for outside users.

The upstream MIT license is foundational to this port. Preserve the root
license file, Cargo's MIT declaration, A Zimin's copyright/authorship, and the
About/credits attribution in every branch and artifact. The Mac packaging flow
must continue copying `LICENSE` into the app bundle. Forking, modifying, or
publishing a personal build does not remove the MIT notice or transfer
authorship of the original work.

## Remotes and long-lived branches

```text
upstream  https://github.com/kay-dutch/ed-compass.git  original project
origin    https://github.com/sdangit/ed-compass.git    maintained fork

main          clean mirror of upstream/main
macos-port    tested Mac integration branch; fork default branch
```

Do not put port-specific commits on `main`. Create fixes and features from
`macos-port`, validate them, and merge them back into `macos-port`.

```sh
git switch macos-port
git switch -c fix/short-description
# edit, test, commit
git switch macos-port
git merge --no-ff fix/short-description
git push origin macos-port
```

Small, obvious maintenance commits may be made directly on `macos-port`.

The fork deliberately defaults to `macos-port`. This makes fresh clones and
fork-local pull requests start from the maintained product while `main` remains
an untouched upstream mirror. Changing the GitHub default does not change the
ancestry or synchronization role of either branch.

## Synchronize the original project frequently

The original project is under active alpha development. Fetch before starting
new work and at least weekly while the port is active, even when no Mac change
is planned. Integrating small upstream increments is safer than accumulating a
large divergence.

```sh
git fetch upstream --prune
git switch main
git merge --ff-only upstream/main
git push origin main

git switch macos-port
git merge main
```

Resolve conflicts on `macos-port`, never by changing the clean `main` mirror.
Merge upstream rather than repeatedly rebasing the long-lived port branch; this
keeps already-pushed history stable and records exactly which upstream state was
integrated. Avoid cherry-picking upstream commits unless an urgent isolated fix
cannot wait for the next complete merge.

After every upstream merge:

```sh
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
packaging/macos/package.sh
```

Then repeat the short live smoke test: bundled launch, Loopback analysis,
capture/export, journal context, and device disable/re-enable when audio code
changed. Windows CI should remain green because the fork still carries and
tracks the original Windows product.

The CI workflow runs for pushes to both long-lived branches, for pull requests,
and on manual dispatch when a fresh verification run is useful. On `macos-port`
it provides two independent gates:

- Windows tests protect compatibility with the original product;
- a pinned `macos-15` Apple Silicon runner performs formatting, Clippy, the full
  test suite, ad-hoc app packaging, bundle validation, and a seven-day workflow
  artifact upload.

Workflow artifacts are convenient personal build outputs, not GitHub Releases
and not a supported distribution channel.

## Toolchain and dependency currency

The Mac integration branch follows the current stable Rust toolchain and the
latest stable releases of direct and transitive dependencies. Pre-release,
release-candidate, beta, alpha, and git-only dependency revisions are excluded
unless a specific experiment documents why they are needed.

Keep `package.rust-version` and every CI/release toolchain selector pinned to the
same current stable release. Review dependency updates as their own integration
change, run `cargo update`, and validate formatting, Clippy, all tests, native
Mac packaging, and Windows CI. GUI, audio, persistence, and platform-library
updates require focused review and the relevant live smoke tests even when CI
is green.

## Releases and publication

`macos-v0.4.5-1` marks the first validated private/local Mac baseline. Future
local Mac checkpoints may use `macos-v<upstream-version>-<port-revision>` until
a public release/versioning policy is chosen.

An agent may create a GitHub release when explicitly requested for personal
deployment. Unless the policy changes, its notes must say that the Apple
Silicon bundle is for personal/local use, ad-hoc signed, unnotarized, and carries
no support or compatibility promise. Publishing an artifact this way is not a
general public release program.

Do not spend effort on Developer ID signing, notarization, Intel/universal
builds, a formal minimum-macOS matrix, public support, or promotion during the
upstream alpha unless explicitly directed. Reconsider those decisions when the
original project reaches 1.0 and `macos-port` has been kept current and
revalidated against it. At that point, evaluate whether the port should remain
personal, become a maintained public downstream, or be proposed upstream.
