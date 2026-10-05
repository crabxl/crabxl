# Alpha.5 M2 acceptance audit

## Scope and verdict

M2 covers complete assigned value/string/date/formula read semantics, bounded
large shared strings and the shared read-side style catalogs. Its acceptance
requires streaming and explicit materialization, ownership, failure handling,
resource accounting and representative measurements. Read/create/edit/preserve
remain distinct. This audit applies the existing roadmap and completion plan;
it does not equate M2 closure with the full openpyxl baseline or completed M4–M7.

The remaining exact style-integer gap found in this audit is implemented in ADR
0054. All required M2 behavioral gates have supporting evidence below. Historical
checkpoint notes are superseded only where the linked later implementation
and checks support that conclusion. Linux/Windows/macOS Rust 1.88 default and native-zlib checks, latest-stable
quality checks and workspace tests pass in CI run 37293401537. Local MSRV,
Clippy and warning-free docs also pass. The final release gates additionally require
Rust MSRV/platform checks, Python adapter checks and public package verification.

The module inventory is deliberately broader than a milestone's behavioral
acceptance. Its original automatic `milestone` classification mixes dependency
foundations with complete modules: for example `formula.tokenizer` is classified
M2, while full tokenizer public behavior has always been explicitly assigned
M5.8 in the completion plan; dataframe adapters are M7. Every original baseline
entry, class/member and capability status is retained. `behavioral_milestones`
clarifies those existing cross-milestone dependencies; planned whole-module
entries are not marked verified by this audit. Future work cannot drop them.

## Behavioral acceptance matrix

Tests below are in `crates/crabxl-xlsx/tests` unless stated otherwise. They inspect
values, source identities, styles and typed errors, rather than only valid ZIPs.
Streaming/materialized/adaptive paths share the canonical decoder; mode-specific
restrictions and ownership are independently asserted.

| Required behavior | Deterministic evidence | Measurement / policy evidence |
| --- | --- | --- |
| Exact integer/float distinctions, booleans, errors, overflow/non-finite policy | streaming scalar cases, `overflow_numeric_lexemes_and_cached_results_retain_infinities`; writer `compatible_nonfinite_numbers_emit_blank_values_without_losing_formulas` | `benchmarks/m2-scalars.md`, `m2-nonfinite.md`; exact model and compatible blank-output policy |
| Inline/shared whitespace, entities, Unicode, literal/protected escapes, absent/empty text | `shared_text_modes_preserve_ids_entities_whitespace_and_owned_lifetimes`; rich text and literal escape codec cases in streaming/writer | `benchmarks/m2-shared-strings.md`, `m2-rich-text.md`; original licensed upstream assertions remain unchanged in the adapter |
| Owned rich runs, font overrides, empty runs and phonetics; explicit plain projection | `shared_rich_metadata_upgrade_disk_cache_and_protected_run_boundaries`; writer `typed_rich_runs_fonts_colors_phonetics_and_empty_style_round_trip` | ADR 0010, shared core run/font types; typed rich extensions reject unsupported selected content |
| Relationship-resolved SST IDs, actual counts, RAM/Auto/disk, bounded data/index/cache | `adaptive_shared_text_spills_payload_and_index_without_losing_ids`, `shared_string_limits_invalid_ids_and_deferred_rich_entries`, zero-cache/reconfiguration cases | `benchmarks/m2-shared-strings.md`, `alpha5-concurrent-auto.md`; declared counts never size an unbounded allocation |
| SST CRC/XML failures, cancellation/close, low disk budget, owned-row lifetime and cleanup | `shared_string_crc_and_xml_failures_leave_no_prepared_table`, `rich_metadata_budget_spill_cache_bypass_and_failure_cleanup`, aggregate failed spill case | Corresponding SST raw measurements and Python actual spill-handle/cleanup assertions |
| Complete shared style components, source IDs, number formats, colors, palettes, named/differential/table definitions | `imported_style_components_keep_ids_optional_overrides_palettes_and_staged_sections`, differential/table definition cases; writer complete/source-catalog export cases | `benchmarks/m2-styles-dates.md`, `m2-style-registry.md`, `m2-style-extras.md`; canonical adoption/export retains IDs |
| Exact charset/theme/indexed integers, lexical RGB identity, component budgets and registration retry | Existing core `public_style_domains_and_literal_color_casing_are_retained` and writer domain test; streaming style limit test now includes six large-payload placements | ADR 0054, `benchmarks/alpha5-style-integers.md`, independent public-reference probe |
| Theme public baseline and requested owned palette/font catalog | writer opaque/default theme cases; `theme.rs` default/namespace/lifetime/budget cases | ADR 0051, `benchmarks/alpha5-theme-catalog.md`; typed color transforms/rendering are separate unsupported extensions, original bytes remain readable |
| Both date systems, serial zero/early 1900/leap anomaly, negative values, time/duration and millisecond conversion | `styled_dates_epochs_duration_cached_formulas_and_general_values_stream`, materialized date policy; core date cases; writer epoch/temporal cases | `benchmarks/m2-styles-dates.md`, calendar precision and ISO evidence; literal and numeric precision policies stay distinct |
| ISO date/date-time/time/duration and typed formula caches | `iso_cells_dates_clocks_durations_and_formula_caches_share_read_modes`; writer ISO creation/payload/retry cases | `benchmarks/m2-iso-dates.md`; timezone-free reference prefix compatibility is explicit |
| Normal/shared formulas, sparse IDs, follower expansion, first-template compatibility and strict geometry | shared formula/template/header/id cases in streaming; canonical translation tests | `benchmarks/m2-formulas.md`, `m2-formula-headers.md`, shared ID/optional formula evidence; no formula calculation |
| Array/data-table/visible dynamic formula metadata independent of expression/cache | `array_table_source_properties_caches_and_empty_bodies_remain_distinct`, literal reference/header cases; writer structured records and attribute-policy cases | ADRs 0031–0037/0041/0052; full metadata graph interpretation/create/edit remains explicitly M6.4 |
| Absent/empty/false/zero/error/date caches, data-only projection without formula reconstruction | `cache_only_projection_ignores_formula_semantics_without_ignoring_xml_or_values`, normal/structured cache cases | `benchmarks/m2-formula-cache.md`, `alpha5-formula-annotations.md`; no fabricated results |
| Explicit owned cm/vm formula references, incompatible modes and projection | `formula_annotation_references_are_owned_bounded_and_separate_from_values`, annotation mode/projection test | ADR 0052; graph-writing requests reject Unsupported before output |
| Byte/count/event/depth/metadata limits and typed part/cell errors | streaming limit/malformed coordinate/style/formula cases, writer atomic preflight tests, core registry failure/retry | ResourceLimits and component/aggregate diagnostics; low-level component settings remain independent |
| Joint catalog/SST/cache/formula/row budgets, Auto sampling/fallback/lending | aggregate cases in streaming and policy-options cases | `benchmarks/m2-aggregate-read.md`, `m2-policy-options.md`; managed accounting is not a process RSS cap |
| Host/container availability, caller override, useful RAM/disk/concurrent strategies | adaptive availability and deterministic allowance/probe tests | ADRs 0044/0050/0053; mounted Linux constraints, Windows/macOS native host probes, conservative fallback and explicit caller override |
| Styled read/create/value edit round-trips and source protection | writer source style/temporal/owned-bank cases; editor rich/shared/structured/date/cache cases | Existing interop and A3/A4 edit/compression evidence; complete structural/catalog/feature edits remain M4/M5 |

## Test quality and coverage

Python temporal replacement tests consolidate forty parametrized invocations
into two batched invocations while retaining all forty semantic combinations,
independent-cell diagnostics, live values, reference values, formats and two
subsequent saves. Shared SST generation removes duplicated setup without removing
ownership/cleanup assertions. Original pinned upstream test files/bodies/licenses
are unchanged. See the adapter's `docs/validation/alpha5-test-consolidation.md`.
The final resource extension adds three batched behavioral tests rather than
one test per option: validation/modes, actual read/edit enforcement, and actual
RAM/disk SST/cleanup. CPython 3.12 has 544 passing tests versus alpha.4's 579;
the difference is test consolidation, not removed behavioral combinations.

Rust domain and budget assertions extend existing tests. The audit checks six
large style payload placements (font, pattern fill, gradient, border, recent
color, differential font), error kind/part and absence of a partially cached
catalog. Registry tests independently check normalization, deduplication,
owned adoption, nested payload accounting and failure/retry. Allocation-failure
branches requiring process-wide exhaustion are not forced to inflate coverage.

Stable Rust 1.99 with cargo-llvm-cov 0.9.1 reports line/region/function coverage.
Standalone tests/examples and the generated default theme are excluded; embedded
`#[cfg(test)]` modules in source are included. Branch instrumentation was not
collected on this stable run: zero branch counters mean unavailable data, not
0% behavioral branch coverage. Source and generic instantiation reports are
reviewed alongside public probes and failure assertions. Coverage is not an
M2 completion percentage. Measured coverage is 87.58% lines (12,908/14,738), 84.73% regions and
79.31% functions. The [compact report](alpha5-coverage.json) contains file counts, exclusions,
reproduction and these limitations; timing benchmarks are separate.

## Explicit remaining scope

- M4: complete loaded-bank aggregate/style integration, workbook/structural edits
  and affected package-graph synchronization. Existing value editing/preservation
  does not close those requirements.
- M5: full style mutation/proxies, theme rendering/color transforms, worksheet
  rules/tables, names, properties and the complete formula tokenizer/utilities.
- M6: drawing/chart/pivot/external/rich-value graphs, interpreted cm/vm metadata
  and dynamic spill graph creation/editing. Literal references are not graph support.
- M7: complete reference API inventory, additional portability/performance tuning,
  private Windows job/macOS process-limit discovery and full cross-library overlap
  comparisons. Windows/macOS NativeHost diagnostics are host availability only;
  constrained callers must supply explicit budgets or available_bytes. Linux
  hidden ancestor constraints also cannot be discovered universally.
- Python: complete rich/style/formula object proxies and other openpyxl public
  surfaces remain staged. Core M2 closure does not promise these adapter APIs.

All of these retain their original roadmap commitments. No universal performance
superiority or hard whole-process RSS limit is claimed. The historical calamine
speed gap and missing direct rust_xlsxwriter comparison remain desired M7 work.
