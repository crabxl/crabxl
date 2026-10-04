# ADR 0014: Structured formula records and sparse shared templates

## Decision

The canonical Formula owns an expression, an optional typed cached value and optional FormulaMetadata. Literal construction removes one leading equals sign; XML source construction retains the body verbatim, including empty bodies and an additional equals sign. Array and data-table types retain source ranges, calculation hints and data-table inputs. Flags distinguish literal booleans from source XML spelling so adapters can match the reference public object properties without implementing another formula engine.

Each streaming worksheet owns a sparse shared-template table keyed by u32 group ID. Translation uses the actual master cell, not the declared range origin. IDs never determine allocation size. Compatible mode keeps the first definition, accepts followers outside declared ranges and retains an empty placeholder for a missing master. ValidateGroups is an explicit extension rejecting those anomalies. Projected cells still scan first shared definitions; cache-only compatible reads retain no templates. Template count and conservative managed-storage budgets are enforced before growth. Exposed accounting includes estimated hash bucket/control storage and owned expressions, not allocator overhead or an exact RSS ceiling. Current estimates follow the Rust 1.88 standard HashMap capacity layout; aggregate adaptive accounting remains staged.

Resolved shared expressions are written as normal formulas, matching reference normalization; array/table records serialize typed metadata. The original-package editor preserves untouched source groups independently. Direct original-cell replacement supports normal/array/data-table records, including changed range spelling and repeat saves, without moving other cells or fabricating reference updates. Shared-group replacement remains guarded until dependency-aware normalization exists. cm/vm references still require typed metadata support. A requested shared metadata projection retains group identity but does not yet expose a complete editable group graph. Calculation and synthetic cached values are not introduced.

Empty string caches retain their source identity in the Rust formula object. Cache-only compatible projection returns Empty for an empty t=str value, corresponding to Python None. Missing caches remain absent. This distinction is verified through newly generated OOXML and public openpyxl 3.1.5 APIs.

## Evidence and remaining scope

[Public observations](../research/formula-public-probe.json), [read interoperation](../../benchmarks/results/m2-formula-public-interop.json), [creation interoperation](../../benchmarks/results/m2-structured-formula-creation.json) and [release measurements](../../benchmarks/m2-formulas.md) accompany streaming, projection, budget, strict-policy and writer round-trip tests. The selected calamine layout, dense-ID allocation and range-origin assumptions are recorded in third_party/ports.json; implementations are integrated into shared models and bounded I/O rather than wrapped wholesale.

Nonfinite compatibility is completed by ADR 0015. cm/vm metadata graphs and dynamic-array extension semantics, full expression tokenization, shared-group editing/creation policies and complete catalog/budget integration remain M2/M4/M6 work. This checkpoint does not complete M2.
