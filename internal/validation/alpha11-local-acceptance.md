# A11 local acceptance and publication gate

Status on 2026-10-21: accepted integration; release preparation, **not a published A11 release**.
The native checkpoint `dbb911f131d24c7e72b7638e4e17099527828ef1` is published
on the review branch in [core PR 1](https://github.com/crabxl/crabxl/pull/1).
Initial automatic review rejections were resolved by verifying the existing
destinations as public repositories and the project owner's admin/push access.
The adapter now resolves that exact Git revision without a local Cargo patch.
No A11 release request, tag or public package has been created.

## Local evidence

- Rust 1.99 full workspace: 262 tests/doctests pass, with one test thread.
- Strict native Clippy passes for the workspace/all targets. Existing injected
  I/O-failure tests deliberately print ZIP-drop diagnostics; those cases pass.
- Rust 1.88 workspace library compilation passed earlier in the candidate work;
  the exact final candidate also passes Rust 1.88 workspace library compilation.
- CPython 3.12 release-extension final regression run: 575 passed, 2 skipped. Both skips
  concern POSIX removal of open files on Windows; they are not counted as passes.
  The A11 integration module has 23 passing cases against CrabXL/openpyxl where applicable.
- After selective alias relocation, 28 focused rich/hyperlink/A11 cases pass.
  A fresh installed Git-pinned wheel also passes all 575 cases, with the same two skips.
- Ruff formatting/checking and strict adapter Clippy pass. The native crate package verifies successfully; the sdist has 210 entries and excludes local patches and native debug artifacts.
- Release measurements of owned, loaded and write-only feature work are recorded
  in the adapter's `benchmarks/alpha11-acceptance.md`, with independent saved-file
  assertions, CPU, working set, sampled temporary storage and cleanup evidence.

The integration covers named styles/themes, row/column appearance and grouping,
raw/live merged metadata and bulk replacement, rich values/shared objects,
hyperlinks and applicable structural edits, preserving repeated saves and explicit
sequential close boundaries (core ADR 0102 and adapter ADR 0042).

## Remaining publication work

Both integration checkpoints are merged. Native Linux/Windows/macOS MSRV and Python platform CI pass. Prepare matching manual Alpha 11 versions and release requests, validate all 25 wheels plus sdist,
publish through the existing workflows and verify fresh public installations.

Full M4/M5/M6/M7, unimplemented affected source feature graphs and all unassigned
baseline APIs remain under their owning later releases. This local checkpoint
does not substitute preservation for typed feature support or claim the complete
openpyxl suite.

Installed Git-pinned candidate wheel SHA256: `05005127cc57a98173706384ae021ac0956b655d7dd436719de6ea7279b320a3`.
Native CI: https://github.com/crabxl/crabxl/actions/runs/39616313813 .
Python PR CI: https://github.com/crabxl/crabxl-python/actions/runs/39617455308 .
