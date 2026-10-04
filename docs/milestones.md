# Milestone acceptance evidence

## M0: Complete architecture and baseline audit index

- Pinned openpyxl 3.1.5, calamine, and rust_xlsxwriter references and license/provenance records: ports.json and THIRD_PARTY_NOTICES.md.
- Public metadata catalog: 190 modules, 567 classes with inherited public members, public functions/exports, and 45 pinned release RST documents. No implementation source or bytecode was inspected to generate this catalog.
- Reproduce with `python tools/catalog_openpyxl_baseline.py --reference-checkout /workspace/openpyxl` using openpyxl 3.1.5; map and validate coverage with `python tools/map_baseline_inventory.py`.
- Every cataloged module and document has a separate staged inventory entry. Read/create/edit/preserve remain separately planned; narrow implemented checkpoints do not mark whole baseline modules verified. This completes the architecture audit index, not all semantic specifications or support.
- One runtime-independent core provides values, addresses, errors, resource limits, memory policies, and shared StyleId identity. Complete style/formula models belong to later codecs.
- ADRs 0001-0003 cover ZIP ownership, adaptive allocations, and writer port restrictions. The writer design records the upstream silent late-row write risk; no writer implementation is claimed.
- MR and work-item reviews are linked in the roadmap. Additional Rust sources remain eligible for concrete M6 gaps.

## M1: Complete raw numeric streaming acceptance

- Relationship-based package/sheet discovery; strict/transitional namespaces; sparse rows; early projection; reusable row buffers; owned bounded batches; explicit materialization and adaptive access policies.
- Integration coverage includes malformed coordinates/XML, limits, CRC after full consumption, reopening, owned output lifetimes, source transfer with into_inner, and source release on archive failure. Dropping an unfinished row stream releases the borrow, not the workbook source; it does not drain unread data or promise CRC validation.
- Recorded five-run 10k/100k/1m-row, ten-column benchmark counts/checksums, CPU/wall time, and native peak RSS are in benchmarks. Fixed-setting numeric streaming RSS remains about 1.5 MiB across these workloads; this is measured evidence, not a universal RSS guarantee.
- Materialized/Auto repeated-access evidence demonstrates the performance-memory tradeoff. Explicit budgets constrain managed retained data; dependency allocations, caller output, and allocator overhead are not a hard process RSS cap.
- Acceptance is raw finite f64/empty cells. Integer precision, strings, boolean/error/date/formula/style semantics are M2. Unsupported selected content returns errors rather than claiming support. Style index zero is not interpreted.
- Closure changes add style identity and lifecycle verification without changing the numeric parser hot path; existing release benchmark evidence remains applicable.

## M2: In progress

All value semantics, shared/inline/rich strings, styles/date systems, formula caches, and disk-backed high-cardinality strings remain required. Individual checkpoints update the verified feature matrix. M2 completion requires mixed-value correctness, large-string resource/cleanup evidence, and representative release benchmarks; M1 numeric results do not establish it.
