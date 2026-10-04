# AI agent instructions

These instructions govern AI work on this repository. They are not a contributor guide. Do not create CONTRIBUTING.md or a separate contributor-rule document unless the user requests one.

## Language and identity

- All project content must be English: source, comments, documentation, examples, configuration text, and commit messages. Do not introduce Chinese into files or published commit history.
- Use repository-local Git identity for both author and committer: `cmostw <300344883+cmostw@users.noreply.github.com>`, unless the user specifies another identity.
- Do not inherit the environment's unrelated global Git identity. Verify author and committer before publishing commits.
- The initial published history consists of two coherent commits: the development plan and AI project rules. Do not retain the superseded setup/cleanup history.

## Product contract and architecture

- Read docs/architecture.md, docs/roadmap.md, and docs/openpyxl-mr-review.md before implementing affected features. Update plans when architecture changes.
- Internals may be deeply idiomatic Rust: ownership, borrowing, enums, traits, iterators, builders, typed IDs, and Result are encouraged.
- The external Rust API must support the capabilities and observable behavior expected from the pinned openpyxl public-feature baseline. Names and syntax may differ; this must not remove functionality or silently change value, formula, date, editing, or preservation semantics.
- Track read, create, edit, and preserve capabilities separately. Preservation alone is not readable/editable support. Staged delivery keeps all baseline features in the roadmap; do not narrow scope to upstream Rust capabilities.
- Python binding work is now authorized in the standalone crabxl-python repository for shared compatibility tests and migration. Its public interface must match openpyxl call names, object access and supported observable behavior; additional capabilities may be extensions. Do not require users to learn a different Python API. Keep core Rust APIs idiomatic and runtime-independent with explicit ownership, typed errors and safe owned handles. Other bindings follow the ordered tiers in docs/binding-contract.md; they are planned, not implemented.
- Python adapter implementation/tests belong to https://github.com/crabxl/crabxl-python. Keep core changes independent of Python and verify cross-repository changes against a pinned core revision.
- Reuse selected pinned openpyxl tests with license/provenance and unchanged assertions. Track unresolved expected failures explicitly; do not count them as passing or claim the full suite runs. Do not silently fall back to openpyxl or accept unsupported properties that are lost on save.
- Use openpyxl docs, directory/module names, public behavior, and interoperability fixtures for architecture. Do not broadly read its implementation. The user separately authorized important pending MR diffs and necessary surrounding code for defect review; keep this exception scoped. Rust upstream implementation may be inspected for porting.

- Follow [the binding and performance contract](docs/binding-contract.md): Rust core is canonical; match each dominant reference public API/mental model; keep compatibility and idiomatic extensions clearly separate and independently evolvable. Never constrain core capabilities to a reference package or force Rust API shape onto a binding.

## Memory and performance

- Performance acceptance targets: faster than openpyxl is required; faster than calamine and rust_xlsxwriter is desired for overlapping capabilities only. Lower peak RSS than all three is desired. Record unmet targets and compare equivalent behavior/modes using the benchmark contract.
- Correctness, memory management, and performance are joint requirements. Do not claim optimization without measurement.
- Streaming readers must not materialize an entire worksheet, decoded cell range, or worksheet XML. Full-model loading is an explicit separate mode.
- Bound buffers and batches by bytes as well as row/cell count. Account for metadata, strings, styles, caches, and unusually large values; document limits and disk-backed large-string strategies.
- Avoid redundant allocations, full-file clones, repeated per-cell metadata, and unbounded caches. Use shared typed IDs and useful borrowing while retaining safe owned paths.
- Measure temporary-file space, I/O, cleanup, and failure behavior. Moving RAM to disk is a tradeoff, not automatically an improvement.
- For parser/writer, allocation, and streaming changes, run relevant correctness checks and representative release benchmarks. Record time, peak RSS, temporary storage when applicable, and checksums/feature assertions across relevant sizes and workloads.
- Separate runtime baseline, streaming/editable costs, and future binding conversion. Do not sacrifice correctness or required features for benchmark results.
- Keep deterministic correctness tests separate from timing benchmarks; avoid fixed timing thresholds in ordinary tests.

## Porting and shared models

- Clone and pin upstream sources before porting; record repository, commit, original files/symbols, destination, license, notices, behavior changes, tests, and inventory entries.
- Port selected coherent modules into the architecture. Do not wrap or re-export calamine/rust_xlsxwriter wholesale as the core.
- Foundational ZIP/XML/date/tempfile crates may be normal dependencies.
- Core owns the common value, style, formula, address, error, and feature models; XLSX owns format codecs and I/O. Do not expose upstream project types through the public facade.
- Preserve mature algorithms where useful, while separating model, validation, codecs, and I/O. Remove duplicate helpers, validators, coordinate logic, style tables, and error schemes.
- Use zero-based internal coordinates with explicit A1 conversions. Use Rust naming, Result, iterators, and builders; do not mimic dynamic Python arguments internally.
- Return typed errors with part/cell/source context for fallible input and I/O. Do not panic or unwrap on user input.
- Retain required upstream copyright/license notices. Record provenance for code, tests, and adapted documentation; source URLs alone are insufficient.
- For each port: identify dependencies/defects, retain focused behavioral tests, integrate shared models, refactor I/O/streaming, verify correctness/memory/performance, and update progress.
- Deliver compilable reviewable checkpoints, not an entire imported project left with indefinite integration debt.

## Style and verification

- Share workspace edition, MSRV, dependency versions, and lint policy. Use rustfmt and Clippy consistently.
- Public Rust documentation, examples, error messages, and fixtures use consistent English.
- Use module tests plus integration/interoperability/round-trip tests as appropriate. Fixtures need provenance or generation instructions; avoid importing huge upstream data collections.
- Unsafe requires documented invariants and relevant verification. Do not add unsafe or C ABI solely for hypothetical future adapters.
- Evaluate upstream updates by diffing pinned revisions and selectively porting validated changes, rather than blindly merging an entire upstream project.

## Mandatory commit skill and push workflow

- Before preparing, splitting, or creating commits, read and follow [Coherent Commits](.agents/skills/commit/SKILL.md), installed from cmostw/coherent-commits.
- Every commit MUST use `<type>(<scope>): <description>` Conventional Commits and describe the resulting change rather than the work process.
- Group by coherent development intent. Related implementation, tests, refactoring, configuration, and docs may stay together; unrelated intents MUST be separate.
- Commit at meaningful checkpoints. Prompt boundaries, time, file count, and diff size are not commit boundaries.
- Review staged changes, verify identity/language, run appropriate checks, and exclude generated artifacts and unrelated changes.
- After every commit, immediately push to the configured remote branch before creating the next commit. Verify the published HEAD.
- Use ordinary non-force updates by default. Force-push only with explicit user authorization, such as the authorized initial two-commit history rewrite; prefer an expected-HEAD safeguard where transport supports it.
- If normal Git transport is unavailable but authenticated GitHub Git Data APIs work, publishing exact local Git objects and updating the ref is an acceptable transport fallback. Preserve local commit SHAs and enforce non-force updates unless separately authorized.
- If publishing fails, report the blocker, retain the local commit, and retry before creating another commit.
