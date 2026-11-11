# Releases

Versions are `X.Y.Z`. Before 1.0, a minor version (`0.2.0`) can change the
public API and a patch version (`0.1.1`) cannot. Only tested, usable changes
get a release.

Describe every change under `## [Unreleased]` in `CHANGELOG.md` as you make it.

## Release

```sh
python tools/release.py prepare 0.1.3
```

This sets the workspace version, the exact internal dependency versions and
Cargo.lock, renames `## [Unreleased]` to `## [0.1.3]`, and updates the
version in the README, the documentation and the bug report form. Review the
changes, commit them as `Release 0.1.3` and push to `main`. Nothing else is
manual.

The **Release Rust** workflow runs on every push to `main`. When the committed
version has no tag yet, it checks the changelog section and the version
history, runs formatting, Clippy, the tests on the current and minimum Rust,
the documentation build and package verification, publishes the three crates,
and then creates the tag and the GitHub release on that commit. The changelog
section becomes the release notes. Pushes whose version already has a tag stop
after the version check.

Release the core before a Python release that needs it: the Python package
takes the core from crates.io.

## GitHub Actions

- Workflow file: `.github/workflows/release.yml`.
- Store the crates.io token in the repository Actions secret **CARGO_API_TOKEN**.
  It must be allowed to publish `crabxl-core`, `crabxl-xlsx` and `crabxl`.
  The workflow gives it to Cargo as `CARGO_REGISTRY_TOKEN` only to publish.

The crates are packaged, verified and published in dependency order: core,
XLSX, facade. Each package is checked against its crates.io checksum before the
next one. Registry delays are retried. Third-party notices are included in the
core and XLSX crate archives.

## Failed or interrupted runs

crates.io uploads cannot be atomic across three crates. Rerun the failed jobs
of the same run, or run the workflow manually on `main`. Packages that are
already uploaded are accepted only when their checksum matches the new archive
and they are not yanked; different content fails. Do not move a tag or
overwrite a crates.io package.

The minimum supported Rust version is 1.88.0. Normal CI and release CI run the
workspace tests with Rust 1.88.0; publication waits for that check and for the
current toolchain checks. Formatting and Clippy use 1.99.0.
