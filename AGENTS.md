# Instructions for AI agents

These rules apply to AI agents that work on this repository. Where they differ
from [CONTRIBUTING.md](CONTRIBUTING.md), these rules win.

Read [internal/STATUS.md](internal/STATUS.md) before you start. It gives the
current state and the next tasks.

## Goal

CrabXL is an XLSX library with the capabilities of openpyxl 3.1.5. It has two
hard requirements:

1. **Fast.** Faster than openpyxl. Faster than calamine (reading) and
   rust_xlsxwriter / XlsxWriter (writing) is the target.
2. **Low memory.** Lower peak memory than openpyxl, calamine and
   rust_xlsxwriter.

Everything else (code style, module layout, how files are split or merged,
internal names) is up to you. Pick whatever makes the code fast, small in
memory and quick to write.

## How to work

- **Plan a whole stage, then build it.** Before you start a stage, decide the
  data model and the XML elements it reads and writes. Then implement the whole
  stage in one pass. Do not ship the stage one small feature at a time.
- **Test after building.** When the stage compiles, run round-trip tests:
  create files with openpyxl, read them with CrabXL, write them back, and
  compare with openpyxl. Fix all failures in one pass.
- **Copy code freely.** You may read and copy any code whose license is
  compatible with MIT (MIT, Apache-2.0, BSD, zlib). This includes openpyxl,
  calamine, umya-spreadsheet and rust_xlsxwriter. Copy whole modules and adapt
  them. Keep the original copyright notice in the copied file and add the
  project once to `third_party/` (repository, commit, license). Do not copy
  code with a GPL, LGPL or unknown license.
- **Measure at the end of a stage.** Run the fixed benchmark suite once per
  stage and compare with openpyxl, calamine and XlsxWriter. Do not write a
  benchmark script or a report for each feature.
- **Keep records short.** Update `internal/STATUS.md` when a stage ends. Update
  `internal/architecture.md` only when the architecture changes. Do not write
  new ADRs, checkpoint reports or validation records. Update
  `internal/features.json` once per stage.

## Memory rules

These rules exist to keep memory low. Follow them.

- Cell data must stream. A streaming reader must not load a full worksheet,
  decoded range or worksheet XML. Full loading is a separate, explicit mode.
- Limit buffers and batches by bytes and by row or cell count.
- Do not add unbounded caches, full-file copies or repeated per-cell metadata.
- Small worksheet and workbook metadata (tables, comments, validation,
  conditional formats, names, properties, drawings, charts) can be loaded
  fully. It does not need streaming or per-item limits.

## Product rules

- Support the capabilities and observable behavior of openpyxl 3.1.5: values,
  formulas, dates, styles, editing and preservation of unknown parts. The Rust
  API can use different names and syntax.
- Do not drop data on save. Return an `Unsupported` error for a feature that is
  not implemented.
- The Rust core is the canonical implementation. Keep it independent of Python.
- Keep upstream types out of the public API.

## Repository map

| Path | Use |
|---|---|
| `crates/crabxl` | Public facade crate. |
| `crates/crabxl-core` | Format-independent models. |
| `crates/crabxl-xlsx` | XLSX reader, writer, editor and loaded workbook. |
| `crates/crabxl-bench` | Benchmark drivers and fixture generators. Not published. |
| `docs/` | Public user documentation. The documentation site publishes this folder. |
| `internal/` | Status, roadmap and architecture. Not published. Older ADRs and reports are history; do not add to them. |
| `benchmarks/` | Benchmark scripts and results. |
| `third_party/` | Licenses and sources of copied code. |

The Python binding is in [crabxl-python](https://github.com/crabxl/crabxl-python).
The documentation site is in [crabxl-docs](https://github.com/crabxl/crabxl-docs).

## Code rules

- Do not panic on user input. Return a typed error with context.
- Do not use `unsafe`.
- Use zero-based coordinates internally.
- Keep `cargo fmt`, `cargo clippy -D warnings` and `cargo test` passing at the
  end of a stage. Intermediate states inside a stage can be broken.

## Writing

- Write all repository content in English.
- User documentation in `docs/` and the README is for people. Follow the
  documentation style in [CONTRIBUTING.md](CONTRIBUTING.md) and the content
  specification in crabxl-docs.
- Code comments, internal records and commit messages only need to be clear
  and short. No word limits apply.

## Commits

- Use this Git identity for author and committer, unless the user gives another
  identity: `cmostw <300344883+cmostw@users.noreply.github.com>`. Set it in the
  repository configuration.
- Use Conventional Commits: `<type>(<scope>): <description>`.
- Commit at natural boundaries. A whole stage can be one commit or a few.
- Push when a stage is complete or when the user asks. Do not force-push unless
  the user authorizes it.
