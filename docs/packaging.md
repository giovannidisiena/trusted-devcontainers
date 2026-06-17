# Packaging

The first supported installation path is Cargo.

From a local clone:

```bash
cargo install --path . --locked
```

The package is intentionally marked `publish = false` while command names and
operational defaults are still expected to change.

## Distribution Maturity

Use this order while the project matures:

```text
1. Cargo install from a local clone
   Best for active development and local customization.

2. Cargo install from a Git tag
   Best current user-facing path before release artifacts exist.

3. GitHub Releases with prebuilt binaries
   Adds fast installs without requiring a local Rust toolchain.

4. Installer script for pinned release artifacts
   Adds curl-based onboarding while keeping shell setup explicit.

5. Homebrew formula
   Best eventual macOS-native installation path.
```

Do not make `curl | sh` the primary path until release artifacts are published
and versioned.

The installed artifact is a single host binary named `tdc`. VM-side files are
embedded at compile time from `payload/`, then materialized into a temporary
staging directory and synced to the target VM when needed.

This keeps installation simple while preserving reviewable, modular payload
files in the source tree.

See [lifecycle.md](lifecycle.md) for normal user installation, update, and
cleanup steps.

## Shell Completion

Generate zsh completion after installing `tdc`:

```bash
mkdir -p ~/.local/share/zsh/site-functions
tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc
```

Ensure zsh loads that directory:

```zsh
fpath=("$HOME/.local/share/zsh/site-functions" $fpath)
autoload -Uz compinit
compinit
```

Then restart zsh:

```bash
exec zsh -l
```

`tdc` does not edit shell startup files during installation.

The generated zsh script includes dynamic completion for local Lima VM names,
client slugs derived from `client-*` VM names, and snapshot tags.

After updating `tdc`, regenerate completion output and clear zsh's completion
cache:

```bash
tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc
rm -f ~/.zcompdump*
exec zsh -l
```

## Git Tag Installs

While the package is not published to crates.io, publish versioned Git tags and
install from those tags:

```bash
cargo install --git https://github.com/<org>/trusted-devcontainers --tag v0.1.0 --locked
```

For untagged development installs from a branch:

```bash
cargo install --git https://github.com/<org>/trusted-devcontainers --branch main --locked
```

Prefer tagged installs for repeatability.

## Versioning

`Cargo.toml` is the source of truth for the package version. `tdc` uses that
same package version as the default trusted image tag and writes a generated
`VERSION` file only into the staged payload that is synced to each VM.

For example, release `v0.1.1` should set:

```text
Cargo.toml: version = "0.1.1"
Git tag: v0.1.1
```

## Release Flow

Before the first public tag, finish these repository-level decisions:

```text
choose a license and add LICENSE
set Cargo.toml license or license-file
set Cargo.toml repository once the GitHub URL is final
add the GitHub remote
```

`cargo package` warns until license and repository metadata are present. That
does not block local Git tag installs while `publish = false`, but it should be
resolved before treating the repository as a public package.

Before opening the release PR:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --locked
cargo package --locked --list
cargo install --path . --locked
```

Release PR:

```bash
git checkout -b release/v0.1.1
git add Cargo.toml Cargo.lock
git commit -m "Release v0.1.1"
git push origin release/v0.1.1
```

Open a pull request into `main`, label it `skip-changelog`, and merge it after
CI passes. Normal feature, fix, documentation, and packaging changes should
also merge through pull requests so GitHub can include them in generated release
notes.

Release tag:

```bash
git fetch origin main --tags
git checkout main
git pull --ff-only origin main
git tag -a v0.1.1 -m "v0.1.1"
git push origin v0.1.1
```

GitHub generates release notes from merged pull requests between the previous
release tag and the new tag. Use the labels configured in `.github/release.yml`
to place pull requests under release-note categories, and use `skip-changelog`
or `ignore-for-release` for mechanical release PRs or changes that should not
appear in the notes.

If the protected publish job fails after the tag workflow has already prepared
and reviewed release assets, fix the workflow on `main` and rerun the release
workflow manually with the same tag. Do not bump the package version for an
infrastructure-only publish failure. Bump only when the released package
contents change, and do not move a tag that has already been pushed.

## GitHub Actions

This repository includes two workflows:

```text
.github/workflows/ci.yml
  Runs formatting, tests, clippy, build, and package-content checks on pushes,
  pull requests, tags, and manual dispatch.

.github/workflows/release.yml
  Runs on v* tags and is split into prepare, review, and publish jobs.

.github/release.yml
  Configures GitHub's automatically generated release-note categories and
  exclusions.
```

The release workflow is modeled after
https://github.com/alcuadrado/trusted-publishing-example, adapted to GitHub
Release assets:

```text
prepare
  Checks formatting, tests, clippy, package contents, version metadata, and
  builds the release tarball/checksum.

review
  Downloads the prepared artifacts without write permissions, verifies the
  checksum, and prints the tarball contents for inspection.

publish
  Runs only after review succeeds. This is the only job with contents: write and
  uses the protected release environment before uploading to GitHub Releases.
```

Configure the GitHub Environment named `release` with reviewer approval before
publishing release assets.
Recommended environment protections:

```text
required reviewers
prevent self-review
no admin bypass
wait timer, if you want a cancellation window
```

This mirrors the useful part of trusted-publishing workflows: ordinary checks
run with read-only permissions, and the release asset upload runs in a protected
environment with explicit approval.

The current project is not published to crates.io:

```toml
publish = false
```

Do not add a crates.io publishing job yet. Cargo publishing currently requires a
cargo API token or configured credentials, and crates.io publishes are permanent
for a given version. Keep the first release path as:

```text
git tag -> GitHub Actions checks -> GitHub Release assets -> cargo install --git --tag
```

If this later becomes a crates.io package, remove `publish = false`, add the
required Cargo package metadata, run `cargo publish --dry-run`, and add a
separate protected publishing job.

Future distribution can add:

```text
cargo-dist release artifacts
Homebrew formula
pre-generated shell completion files
pre-generated manpage
```

Generate raw roff source for packaged manpage artifacts:

```bash
tdc manpage --raw > tdc.1
```

For local use, install the generated page into a user man directory:

```bash
tdc manpage --install
```

If the install directory is not in `manpath`, `tdc` prints the required
`MANPATH` line.

## Installer Script

A `curl | sh` installer is appropriate once releases publish prebuilt binaries.
It should install a pinned release artifact, not build from `main` and not
silently edit shell startup files.

Preferred command pattern:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/<org>/trusted-devcontainers/releases/download/v0.1.0/tdc-installer.sh \
  | sh
```

The installer should default to a user-owned install directory such as:

```text
~/.local/bin/tdc
```

At the end, it should print the shell setup steps:

```text
Add this to your shell config if needed:
  export PATH="$HOME/.local/bin:$PATH"

Enable zsh completion:
  mkdir -p ~/.local/share/zsh/site-functions
  tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc

Ensure ~/.zshrc contains:
  fpath=("$HOME/.local/share/zsh/site-functions" $fpath)
  autoload -Uz compinit
  compinit

Then restart zsh:
  exec zsh -l
```

Good installer ergonomics:

```text
install a versioned release binary
avoid sudo by default
support --version
support --install-dir
verify checksums when not using a generated trusted installer
leave Cargo install documented as the source/development path
```

`cargo-dist` is the preferred way to add this later because it can produce
release artifacts, checksums, and installer scripts consistently.
