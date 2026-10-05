# Alpha.3, Alpha.4 and Alpha.5 delivery plan

## Objective and release order

Complete and publish `0.1.0-alpha.3`, then `0.1.0-alpha.4` and `0.1.0-alpha.5`, including the
canonical Rust crates and the separately maintained Python package. Python
versions are `0.1.0a3`, `0.1.0a4` and `0.1.0a5`. Published alpha.2 artifacts and tags remain
immutable. Manual version increments occur only for verified, usable changes.

This plan sets delivery goals, not completion claims. M2 remains open until its
acceptance criteria are satisfied. The existing [roadmap](roadmap.md) and
[Rust completion plan](completion-plan.md) define the broader scope.

## Alpha.3: Editing and writing performance

Completed: Rust and Python alpha.3 are published and public installations are
verified. See [measurements](../benchmarks/alpha3-edit-buffering.md).

1. Establish alpha.2 baselines for ordinary creation, write-only creation and
   existing-file edit/save. Include numeric, text, date/style and formula
   workloads at representative sizes. Separate loading, mutation and saving
   where useful; compare equivalent supported behavior and modes.
2. Profile Python-to-Rust conversion, XML encoding, ZIP compression and temporary
   I/O. Investigate worksheet rewrite costs during existing-file saves and
   per-row binding overhead before choosing changes.
3. Implement improvements supported by measurements. Preserve formula-cache
   invalidation and recalculation behavior, original assets and unknown parts,
   bounded buffers, managed allocation limits, target protection and cleanup.
   Skipping necessary validation or retaining stale formula caches is not an
   acceptable optimization.
4. Validate output values, types, styles, formulas and preservation through
   focused regression tests and public-reference readback. Record elapsed time,
   peak RSS, temporary storage and output size against alpha.2. Explain workload
   limits and tradeoffs; do not infer universal speedups from one benchmark.
5. Run required Rust and Python checks, including Rustfmt, Clippy, Ruff format
   and check, and relevant cross-platform tests. Publish and verify installation
   from the public registries.

Acceptance: usable editing/writing improvements have reproducible evidence,
required behavior remains correct, and memory and temporary-storage changes are
documented. Regressions must be investigated and resolved or explicitly justified
before publication.

## Alpha.4: Configurable ZIP compression

Implemented and verified on Linux/Windows Rust 1.88 with both backends;
publication is in progress. See [compression evidence](../benchmarks/alpha4-compression.md).

1. Compare the existing ZIP features, pure-Rust zlib-rs and native zlib with
   equivalent numeric/text creation and existing-file editing workloads.
2. Expose validated per-save compression levels in Rust and Python. Level 0
   stores rewritten parts without compression; levels 1 through 9 use Deflate.
   None retains the default level 6. Original compressed passthrough stays intact.
3. Record timing, output size, memory and correctness across backends/levels;
   select a documented default based on evidence and portability.
4. Verify Rust 1.88, alternative-backend builds and supported Python platforms.
   Publish and verify alpha.4 crates and Python packages.

Acceptance: options work across ordinary creation, write-only creation and loaded
editing; invalid levels protect targets and permit retry; package semantics and
resource guarantees remain intact. M2 closure is explicitly deferred to alpha.5.

## Alpha.5: Test consolidation and M2 closure

1. Audit current code against M2 acceptance. Distinguish implemented behavior,
   remaining gaps and historical checkpoint notes. Keep M4/M5 and Python adapter
   requirements separately tracked; do not narrow M2 merely to close it.
2. Map behavior and failure risks to existing tests. Consolidate redundant cases,
   remove tests that mirror implementation without independent assertions, and
   strengthen weak assertions. Prefer fewer tests with higher effective coverage.
   Preserve important regression cases and required unchanged upstream test
   bodies, licenses and provenance. There is no test-count or line-count target.
3. Use code and branch coverage where available to identify gaps, alongside
   assertion quality and interoperability evidence. A coverage percentage or
   passing test count alone does not establish correctness. Keep timing
   benchmarks separate from deterministic tests.
4. Implement outstanding M2 value, style, formula and resource-management
   requirements identified by the audit. Add focused tests for uncovered behavior
   rather than duplicating existing coverage.
5. Verify mixed-value semantics, both date systems, formula/cache behavior,
   repeated and high-cardinality strings, large-string disk strategies, managed
   budgets, failure paths and temporary cleanup. Measure representative release
   workloads and investigate performance regressions.
6. Update capability mappings, limitations, acceptance evidence and milestone
   status. Close M2 only when every required acceptance item has supporting
   evidence. Publish alpha.5 and verify public installation.

Acceptance: test consolidation retains effective behavior coverage; M2 has no
unresolved acceptance blockers; correctness, resource behavior and performance
evidence support closure. Complete Python compatibility and later milestones
remain separately tracked.

## Publication and checkpoints

- Keep Rust changes in `crabxl/crabxl`; keep Python bindings in
  `crabxl/crabxl-python`, pinned to a verified canonical core revision.
- Rust core retains MSRV 1.88. Python wheels use latest stable Rust and support
  CPython 3.11 through 3.15, allowing the 3.15 release candidate until stable is
  available.
- Publish `crabxl-core`, `crabxl-xlsx` and `crabxl` through the Rust release
  workflow. Publish Python through OIDC with five platform/architecture build
  jobs: Linux x86_64/ARM64, Windows x86_64 and macOS x86_64/ARM64. Each produces
  and tests five Python wheels; also build and test the source distribution.
- Verify public registry versions, release tags, artifact identities and clean
  installation. Exercise representative read/write/edit operations using the
  published packages.
- Commit coherent, reviewable checkpoints and immediately push each commit.
  Update this plan with actual evidence and remaining work as delivery proceeds.
  A documentation-only update does not initiate a release.
