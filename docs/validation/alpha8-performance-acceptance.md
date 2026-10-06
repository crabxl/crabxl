# Alpha.8 performance and resource acceptance

Status: native alpha.8 and Python 0.1.0a8 published and verified.

## Accepted changes

| Change | Verification and measured scope |
| --- | --- |
| Conservative clean inactive Linux cgroup cache credit | ADR 0076; existing Auto discovery matrices and public-core probe; preserves dirty/writeback exclusion and caller budgets |
| Buffered complete scalar cells | ADR 0077; shared canonical validators and checked event fallback; numeric, SST, styled and Python complete-operation measurements |
| Buffered plain ASCII SST preparation | ADR 0078; direct canonical ownership, bounded XML and rich/prefixed fallback; unique/repeated RAM/disk and Python measurements |
| Byte-oriented XML replacement scanning | ADR 0079; exact uncompressed output comparison, Unicode/XML value readback, both compressors; modest large text gains and mixed numeric results |
| Default model cardinality follows byte budgets and coordinates | ADR 0080; explicit finite limits retain enforcement; more than 1,024 sheets verified and fresh Python wheel compatibility |
| Descending population fills bounded first blocks | ADR 0081; independent sparse reference map and complete write readback; one-million-cell RSS falls about 68%, without a general speed claim |
| Capacity-aware retained models and growth work | ADR 0082; one million cells under 64 MiB and row insertion under 384 MiB; preserves large reading with explicit per-cell creation overhead |

Capacity-aware model accounting (ADR 0082) has native resource/performance
evidence and validated binding integration. Tests have been
extended within existing workflows; these changes do not add timing thresholds
or redundant test functions.

## Before/after evidence

All reports retain individual serial samples and binary/fixture hashes, with
generation, compilation, profiling and output verification excluded from timing.
Read workers traverse all values and check fixture counts/checksums; text/style
workers additionally compare individual values and coordinates. Model,
row-stream, edit/save and writer operations have distinct retention/feature
boundaries. RSS, managed allowances and sampled temporary descriptors are separate.

- [Buffered scalar decoding](../../benchmarks/alpha8-buffered-scalars.md)
- [Direct plain SST ownership](../../benchmarks/alpha8-direct-sst.md)
- [Byte escaping and verified writes](../../benchmarks/alpha8-byte-escape.md)
- [Descending insertion](../../benchmarks/alpha8-descending-insertion.md)
- [Capacity-aware model allowances](../../benchmarks/alpha8-capacity-accounting.md)
- [Conservative Auto cache availability](../../benchmarks/alpha8-reclaimable-memory.md)
- [Rejected append-tail storage](../../benchmarks/alpha8-rejected-tail.md)
- [Rejected whole-text handle sharing](../../benchmarks/alpha8-rejected-text-sharing.md)

Rejected runtime prototypes are absent from the final implementation; their
reports retain source patches and explain their regressions. A memory win on
one text distribution does not justify silently worsening owned text creation.

## Remaining limits

These controlled numeric/repeated/unique-text workloads demonstrate substantial
native and conversion-inclusive Python read improvements against A7. Some
overlapping workloads reach or beat the pinned calamine reference; close medians,
mixed styled results and differing editable/source ownership prohibit a universal
speed/RAM claim. Native streaming RSS still exceeds calamine on the measured
numeric case. No Pandas/Polars integration or actual NYC workbook result is verified.

Writing and load/edit/save still incur compression, temporary worksheet I/O and
serialization work. Escape scanning has a narrow benefit; it does not establish
universal superiority over rust_xlsxwriter or umya. Structural work reservations,
per-alias shared payload charging and larger real-file workloads remain explicit
follow-up opportunities. Resource defaults remove arbitrary cardinality caps
while preserving explicit byte/count controls and legal coordinate checks.

Full M4 feature-graph interactions, all assigned M5 families and M6 graphs remain
in the authorized plan. This performance release does not close those milestones.

## Release gates

Record exact final core/binding SHAs, full local and platform checks, three-crate
publication, Python 3.11-3.15 wheels/sdist and fresh public-package verification
here before marking publication complete. Both native and Python A8 publication
gates are now complete.

Native implementation `a6e83a21bef58597b458dd548f1cc0c41a4b63f7` passes local
workspace tests, strict Clippy, warning-free documentation, Rust 1.88 core tests
and both default/native-zlib loaded/writer checks. GitHub Rust run
[37410046133](https://github.com/crabxl/crabxl/actions/runs/37410046133) passes
the configured platform/MSRV matrix. Its fresh CPython 3.12 binding wheel passes
547 compatibility cases (4.54 seconds), Ruff format/check and strict Clippy.
Two existing finite-budget fixtures were updated for block-capacity accounting;
their aggregate failure, retry, aliases and source-preservation assertions remain.

## Verified native and Python publication

Rust release run
[37410572504](https://github.com/crabxl/crabxl/actions/runs/37410572504) succeeds
at `f08b8e6d494e575ba39289dde4895b7a9b648242`, publishing all three alpha.8
crates and their exact archives in
[the tagged release](https://github.com/crabxl/crabxl/releases/tag/0.1.0-alpha.8).
Regular Rust run 37410572488 also passes its platform/MSRV matrix. A fresh
Rust 1.88 crates.io consumer resolves all three packages from registry sources,
completes ascending/descending million-cell probes under 64 MiB, and verifies
row insertion under 384 MiB. [The audit](alpha8-release.json) records source and
functional checksums; these are resource/publication checks, not timed claims.

Python `1c9256563ea222341cd594b74fdadfe3cb8143ad` pins that exact released core
and prepares `0.1.0a8`. Its final local wheel passes 547 tests (4.51 seconds),
Ruff and strict Clippy; compatibility run 37411371457 succeeds. Complete Python
numeric/unique comparisons are recorded in the binding's A8 report, with preview
wheel hashes and their identical runtime-source relationship to the released core.
Release run [37411371452](https://github.com/crabxl/crabxl-python/actions/runs/37411371452)
succeeds across all five platform jobs and OIDC publication. PyPI exposes 25 wheels
for Python 3.11-3.15 and one sdist. A fresh public PyPI CPython 3.12.14 installation
passes all 547 tests in 4.57 seconds. This verifies public-package usability
separately from the local preview performance measurements.
