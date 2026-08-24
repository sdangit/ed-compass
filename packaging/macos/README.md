# macOS application bundle

Run from the repository root:

```sh
packaging/macos/package.sh
```

The script builds the native Apple Silicon release binary and creates:

```text
dist/ED Compass.app
```

The bundle includes the original ED Compass icon, version and copyright
metadata, an audio-input usage description, credits, and the MIT license. It is
ad-hoc signed for local use. It is not Developer ID signed or notarized and is
not intended for redistribution in its current form.

The version comes from `Cargo.toml`. Mutable settings and user artifacts are
not placed inside the bundle.
