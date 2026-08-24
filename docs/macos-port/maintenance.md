# Maintenance workflow

## Remotes and long-lived branches

```text
upstream  https://github.com/kay-dutch/ed-compass.git  original project
origin    https://github.com/sdangit/ed-compass.git    maintained fork

main          clean mirror of upstream/main
macos-port    tested Mac integration branch
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

## Releases and publication

`macos-v0.4.5-1` marks the first validated private/local Mac baseline. Future
local Mac checkpoints may use `macos-v<upstream-version>-<port-revision>` until
a public release/versioning policy is chosen.

Creating the fork does not itself commit to publishing releases, soliciting
users, or offering support. Developer ID signing, notarization, minimum macOS
version, Intel/universal builds, and upstream contribution remain explicit
future decisions.
