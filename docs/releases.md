# Alpha releases

Only future checkpoints with newly usable, tested functionality receive releases.
Routine fixes, refactoring and documentation do not automatically increase the
alpha number. Do not backfill historical tags. The two repositories number their
alpha releases independently.

## Prepare the committed version

Choose the next number manually, then run from the repository root:

```sh
python tools/release.py prepare 0.1.0-alpha.1
```

This synchronizes the workspace package version, exact internal dependency
versions and Cargo.lock. Review, test and commit that change with the feature
checkpoint. The tool does not commit, push, create tags or publish anything.
The first prospective release is `0.1.0-alpha.1`; historical checkpoints are not tagged.

## GitHub Actions

- Workflow name: **Release Rust alpha**.
- Workflow file: `.github/workflows/release.yml`.
- Run it manually on the default branch with the committed version, notes naming
  the new functionality and its verified limits, and `usable_feature` selected.
- Store the crates.io token in the repository Actions secret **CARGO_API_TOKEN**.
  It must be authorized to publish `crabxl-core`, `crabxl-xlsx` and `crabxl`.
  The workflow maps it to Cargo's `CARGO_REGISTRY_TOKEN` only for publication.

Formatting, strict Clippy, workspace tests, documentation and first-package
verification must pass before publication. The crates are packaged, verified and
published in dependency order: core, XLSX, facade. Each package is verified against
its crates.io checksum before continuing. Registry propagation is retried.
Third-party notices are included in core/XLSX crate archives.

Only after all three uploads succeed does the workflow create the exact
`0.1.0-alpha.N` tag and GitHub prerelease with the crate archives attached. It uses
the workflow's checked-out commit, even if the default branch advances meanwhile.
The alpha suffix is never generated or incremented automatically.

## Failed or interrupted runs

crates.io uploads cannot be made atomic across three crates. A failed run may
leave some packages published without a GitHub tag/Release. Resume the same
workflow commit and version; already uploaded packages are accepted only when
their checksum matches the newly packaged archive and they are not yanked.
Different content, another commit's tag or an obsolete alpha version fails.
Do not move an existing tag or overwrite an immutable crates.io package.

GitHub OIDC publication is configured separately in the Python repository; its
workflow does not use this Cargo token.

Starting with the next release, the Rust crate MSRV is 1.88.0. The immutable
`0.1.0-alpha.1` release still requires 1.99.0. Both normal CI and release CI run
the workspace tests with Rust 1.88.0; publication waits for that check as well as
the current development toolchain checks. Formatting and Clippy use 1.99.0.

An explicit committed `.github/release-request.json` also starts this workflow
on push to the default branch. It must contain the prepared version, nonempty
feature_notes and boolean usable_feature=true. This is a manual release request,
not automatic versioning: unrelated pushes cannot trigger publication.
