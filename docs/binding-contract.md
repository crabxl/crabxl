# Binding and performance contract

## Canonical implementation

Rust core is the only canonical implementation. Models, validation, algorithms and feature semantics belong in the shared Rust implementation; adapters translate language-specific calls, objects, ownership and errors. Do not implement a second spreadsheet engine in a binding or silently fall back to its reference package.

For languages with a dominant reference package, match that package's public API and mental model. Capabilities already present in the reference package use its compatibility interface rather than a newly invented CrabXL API. Additional core capabilities receive idiomatic language extensions. A reference package's missing capabilities never restrict Rust core, and Rust API structure never dictates binding API structure.

Compatibility surfaces and CrabXL extensions must be clearly distinguishable and independently evolvable. Keep their documentation, tests and compatibility/version policies separate. Extensions must not change reference-compatible defaults or introduce ambiguous competing calls for existing reference capabilities. Choose concrete extension namespaces per language when implementing its adapter.

## Binding priority

| Tier | Language | Reference interface |
|---|---|---|
| 1 | Python | openpyxl |
| 2 | JavaScript / TypeScript on Node | ExcelJS-compatible API |
| 2 | JavaScript / TypeScript through WASM | The same ExcelJS-compatible API |
| 3 | C# / .NET | ClosedXML |
| 3 | Java / Kotlin | Apache POI |
| 4 | PHP | PhpSpreadsheet |
| 4 | Go | Excelize |
| 5 | Ruby | RubyXL and caxlsx |
| 5 | Swift | A separately designed idiomatic interface |
| 5 | Dart | excel |

The Node and WASM adapters share one ExcelJS-compatible public API and mental model. Platform adapters handle host I/O, ownership and runtime constraints; core behavior remains canonical in Rust. Exercise shared compatibility tests on both targets and document host-specific capabilities without inventing a second spreadsheet API.

Python implementation is authorized now. Other adapters are planned in this order; this contract does not claim they exist. Pin each reference version, inventory its public behavior, and define overlapping or mode-specific surfaces before implementing its adapter. Ruby's read/edit and creation references require an explicit compatibility map rather than an invented merged API.

## Performance targets

Here, greater speed means lower elapsed time for the same completed operation, and lower RAM means lower measured peak RSS under comparable conditions.

| Reference | Speed target | RAM target | Scope |
|---|---|---|---|
| openpyxl | Required: CrabXL faster | Desired: CrabXL lower | Equivalent supported behavior and modes |
| calamine | Desired: CrabXL faster | Desired: CrabXL lower | Overlapping read capabilities only |
| rust_xlsxwriter | Desired: CrabXL faster | Desired: CrabXL lower | Overlapping write capabilities only |

These are acceptance goals, not assertions about current results. Functional correctness and full planned feature coverage remain required. Record a failed required speed target as unresolved; do not hide it behind a faster unrelated workload or remove the feature.

Use pinned versions, identical inputs and verified outputs, equivalent semantics/modes, release builds, warmups and alternating repeated runs. Record wall time, CPU time, peak RSS, temporary storage, output size and relevant feature/checksum assertions. Include representative numeric, repeated/high-cardinality text, styled, sparse, multi-sheet and edit workloads as capabilities become available. Separate streaming, materialized and preserving-edit operations; report unequal capabilities explicitly rather than presenting them as equivalent measurements.

Compare Python migration workloads through the Python adapter using compatible calls and include conversion/runtime costs. Report native Rust comparisons separately. Include runtime baselines and avoid subtracting them from headline RSS results. Memory policies remain configurable: bounded streaming, explicit budgets and measured adaptive Auto behavior. Extra memory is useful only where measurements establish a speed benefit; lower RSS must not be claimed by concealing temporary-disk or I/O costs.
