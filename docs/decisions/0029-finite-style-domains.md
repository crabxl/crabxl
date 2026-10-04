# ADR 0029: Finite style domains and gradient preparation budgets

## Decision

Shared core validation accepts any finite font size and gradient edge value, including values outside the former 0..409 and 0..1 ranges. Empty number-format strings are valid custom formats. These domains match observed openpyxl 3.1.5 public constructor/save/reload behavior. Non-finite style values remain rejected; tint, gradient stop position, font family and alignment constraints retain their separate validation.

Color decoding follows public constructor identity priority: indexed, theme, auto, then RGB. Known attributes are all decoded for XML/entity validity, but scalar validation applies to the selected identity only. Unknown attributes still fail explicitly. Raw unspecified colors remain distinct from a language adapter's default-property projection; this change does not implement theme palette resolution or a complete Python style interface.

Gradient decoding reserves allowance for the duplicate-position validator's temporary u64 array as well as retained stop capacity. It retains stop order and rejects insufficient preparation allowance before allocating the next stop. The allowance covers managed component storage and validation scratch, not total process RSS or XML event buffers.

## Verification

Core registration checks finite negative/large sizes, out-of-range finite gradient edges, empty format strings and non-finite rejection. Reader tests verify selected color priority, invalid entities in ignored attributes and unknown attributes. A tight two-stop budget test distinguishes retained storage from the larger preparation requirement. Public constructor/save/reload fixtures are checked again through the canonical reader; see benchmarks/probe_finite_style_domains.py and benchmarks/m2-finite-style-domains.md.

Loaded catalog editing, differential/table/extension styles and remaining style semantics stay in the completion plan. No milestone completion is claimed.
