# Binding and performance contract

## Canonical implementation

Rust core is the only canonical implementation. Models, validation, algorithms and feature semantics belong in the shared Rust implementation; adapters translate language-specific calls, objects, ownership and errors. Do not implement a second spreadsheet engine in a binding or silently fall back to a reference package.

## Native API first

Each binding's primary API is a native CrabXL API designed for its language. It shares the core's semantics, not its structure: the same values, formulas, styles, references, errors and guarantees (lossless saves, atomic path saves, reference updates on row and column edits, exact numbers), expressed in that language's idioms. A reference package is not the design reference, foundation or constraint of the native API.

## Compatibility interfaces

Where a language has a dominant reference package, a binding may also offer a compatibility interface for it, under its own namespace (for Python, `crabxl.openpyxl`). A compatibility interface adapts to CrabXL: it selects the core options that reproduce the reference package's behavior (for example cells-only row edits, value-based date kinds or `DataLossPolicy::Allow`). Reference behavior never becomes a core default, and core options are named by what they do, not after a reference package.

Native and compatibility interfaces are independently maintained and versioned in their documentation, tests and compatibility policies. A compatibility interface must not change native defaults, and the native API must not add calls whose only purpose is compatibility.

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

Compare Python workloads through both the native API and the compatibility interface, and include conversion and runtime costs. Report native Rust comparisons separately. Include runtime baselines and avoid subtracting them from headline RSS results. Memory policies remain configurable: bounded streaming, explicit budgets and measured adaptive Auto behavior. Extra memory is useful only where measurements establish a speed benefit; lower RSS must not be claimed by concealing temporary-disk or I/O costs.

Loading optimization now explicitly targets calamine for both streaming and
non-streaming reads. Attempt lower elapsed time and lower peak RSS for complete
retained models, including all sheets and their source catalogs. A smaller model
does not satisfy the speed target. Track native parsing, model retention and
Python value conversion separately, and evaluate bounded column/batch delivery
for potential Pandas/Polars integration. Integration is a future capability,
not a claim of adoption or an existing adapter.
