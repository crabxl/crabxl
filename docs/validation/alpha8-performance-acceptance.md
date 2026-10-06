# Alpha.8 performance and resource acceptance

Status: implementation acceptance in progress; alpha.8 is not yet published.

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
evidence; binding integration remains pending. Tests have been
extended within existing workflows; these changes do not add timing thresholds
or redundant test functions.

## Before/after evidence

All reports retain individual serial samples and binary/fixture hashes, with
generation, compilation, profiling and output verification excluded from timing.
Complete value and coordinate verification runs inside the read workers. Model,
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
here before marking publication complete. Existing published version remains A7.
