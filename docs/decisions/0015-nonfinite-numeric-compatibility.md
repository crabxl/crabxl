# ADR 0015: Nonfinite numeric compatibility

## Decision

CellValue::Number can retain IEEE-754 nonfinite values. Numeric XML lexemes still pass the decimal/integer/scientific grammar path: overflow expressions such as 1e999 become infinity, while textual NaN/inf tokens remain invalid. Numeric styled dates use the existing Compatible date-error policy; RetainSerial still requires finite serials. No invalid datetime object is fabricated.

NonFiniteWritePolicy::Blank is the default for sequential writers and original-package replacement overlays, matching pinned public openpyxl save behavior. A nonfinite number or numeric formula cache emits a present empty v element; the formula expression remains present. A reopened literal/cache-only value is Empty, corresponding to Python None. The in-memory value remains nonfinite until serialization. This is deliberate compatible conversion, not exact nonfinite round-trip storage.

NonFiniteWritePolicy::Reject is an explicit extension. Sequential row validation rejects before spooling; editor overlay validation rejects before mutation. Both report cell context and retain prior committed state. The shared serializer uses one blank conversion for writer/editor, rather than binding-specific normalization. Finite integer/float types and row/batch/overlay accounting remain unchanged. Nonfinite styles or dates are still rejected by their own typed validators.

## Evidence

Public assignment/save/load observations are reproducible with benchmarks/probe_nonfinite.py and recorded in docs/research/nonfinite-public-probe.json. All 16 generated formula cases now pass, including the two formerly deferred overflow cases. Independent native output readback is under benchmarks/results/m2-nonfinite-creation.json. Streaming, sequential writer and original-editor tests cover infinity lexemes, malformed numeric input, cache preservation, blanks, strict atomic rejection and cleanup.

The before/after shared-formula reader uses an exact preserved a710b6d native binary (SHA-256 61f1ab3829230845852ed55adb217799a5da3682911935b9bfb263107c280a6d), warmup plus five rotating serial samples against the current finite workload. Raw regression measurements are under benchmarks/results/m2-nonfinite-formula-regression.json. Native compatibility creation versus public sequential creation is documented in benchmarks/m2-nonfinite.md; builds and other heavy checks are excluded during samples.

M2 remains incomplete: complete theme/catalog integration, aggregate adaptive budgets and dynamic formula metadata are still required. This decision does not imply formula calculation or full metadata graph editing.
