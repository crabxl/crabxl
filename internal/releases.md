# Releases

Versions are `X.Y.Z`. Before 1.0, a minor version (`0.2.0`) can change the
public API and a patch version (`0.1.1`) cannot. Only tested, usable changes
get a release. The core and the Python package number their releases
independently.

## Prepare the committed version

Choose the version manually, then run from the repository root:

```sh
python tools/release.py prepare 0.1.0
```

This sets the workspace package version, the exact internal dependency
versions and Cargo.lock. Review, test and commit the change with a changelog
entry for the version. The tool does not commit, push, create tags or publish.

## Release

After the version commit is on `main`, tag it and push the tag:

```sh
git tag 0.1.0
git push origin 0.1.0
```

The tag starts the release workflow. It checks that the tag matches the
committed version, that the tagged commit is on `main` and that `CHANGELOG.md`
has a section for the version. That section becomes the release notes.

## GitHub Actions

- Workflow name: **Release Rust**.
- Workflow file: `.github/workflows/release.yml`.
- Store the crates.io token in the repository Actions secret **CARGO_API_TOKEN**.
  It must be allowed to publish `crabxl-core`, `crabxl-xlsx` and `crabxl`.
  The workflow gives it to Cargo as `CARGO_REGISTRY_TOKEN` only to publish.

Formatting, strict Clippy, workspace tests, documentation and first-package
verification must pass before publication. The crates are packaged, verified
and published in dependency order: core, XLSX, facade. Each package is checked
against its crates.io checksum before the next one. Registry delays are
retried. Third-party notices are included in the core and XLSX crate archives.

Only after all three uploads succeed does the workflow create the GitHub
release for the tag, with the crate archives attached.

## Failed or interrupted runs

crates.io uploads cannot be atomic across three crates. A failed run can leave
some packages published without a tag or GitHub release. Rerun the failed jobs of
the same workflow run; packages that are already uploaded are accepted only
when their checksum matches the new archive and they are not yanked. Different
content, another commit's tag or an older version fails. Do not move an
existing tag or overwrite a crates.io package.

The Python repository publishes with GitHub OIDC; its workflow does not use
this Cargo token.

The minimum supported Rust version is 1.88.0. Normal CI and release CI run the
workspace tests with Rust 1.88.0; publication waits for that check and for the
current toolchain checks. Formatting and Clippy use 1.99.0.
