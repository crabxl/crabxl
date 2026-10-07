# Alpha 10 appearance component acceptance

Status: local native, pinned Python and performance acceptance passed; publication requested.

## Behavior

The canonical `StyleComponent` API replaces a shared font, pattern/gradient fill,
complete border, alignment or protection without copying unrelated component
payloads or cell values. Derived formats retain number-format/base identities and
unrelated flags. Appearance changes preserve temporal encoding preferences;
explicit General assignments retain A9 semantics. Registry capacity, interned
payloads and format indices remain charged to aggregate allowances.

Python exposes `crabxl.styles` values, familiar aliases, immutable cell proxies,
copy/reassign behavior and one reusable checked native snapshot per caller-owned
appearance value. Nested colors, sides and gradient stops invalidate snapshots
when changed. Editable/loaded cells, read-only component getters and write-only
component directives use the same Rust model and codecs. Detached cell aliases
retain their appearance independently of replacement coordinates.

## Verification

Rust 1.88 full workspace tests, current stable strict Clippy and warning-free
documentation passed. Existing registry tests cover all five component changes,
ledger/capacity equality, deduplication, invalid IDs/components, tight budgets and
retaining a one-MiB unrelated font payload without copying it. Existing loaded
and writer tests cover XML output and repeat saves. Python's existing 547 cases
include both public engines, all appearance families, gradients, diagonal borders,
copy/proxy semantics, source edits, read-only getters, write-only directives,
automatic date formats and repeated saves. Ruff remains required.

The complete operation benchmark uses 100,000 cells, one warm-up and three
alternating fresh-process samples in owned/load-edit-save/write-only modes. It
validates every cell and all component families after timing and RSS collection,
records CPU/output/checksum/temp usage and verifies zero remaining spools.
No builds, tests or profiling overlap timing. Initial dictionary conversion
failed the required openpyxl speed target; native snapshot reuse and Python
assignment simplification resolve that measured bottleneck. Raw initial and
intermediate results remain available; the pinned final receipt follows.


Final measurements pin core `626d3e24f6744237a975cf4b377242656cd58ed6` and
record native SHA-256 `21b2bde9fa5bab13a4f05c5faee8dd60e451b467aadc8e7387f70284e98b8842`.
[Raw samples](https://github.com/crabxl/crabxl-python/blob/main/benchmarks/results/alpha10-full-components-python.json)
include the script digest and all assertions. The package-version promotion
does not change those verified component implementations.

| Mode | CrabXL seconds | openpyxl seconds | CrabXL RSS KiB | openpyxl RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| New editable | 1.4959 | 2.3859 | 25,088 | 80,360 |
| Load/edit/save | 1.7602 | 2.7885 | 25,496 | 84,496 |
| Write-only | 1.5072 | 2.5632 | 21,436 | 32,900 |

All three modes pass the required openpyxl speed gate. CrabXL sampled temporary
spools peaked at 3,768,002 bytes in new/editable and write-only modes and zero
in loaded mode; openpyxl peaked at about 4,278,299 bytes. Every worker verified
zero remaining spools. Sampling is not an exact disk-high-water guarantee.

## Remaining scope

A10 does not close M5. Named styles, theme resolution, row/column appearance and
advanced feature graphs follow the release plan. Affected unmodeled graphs,
signed/data-only sources, unknown style sections and missing source stylesheets
retain explicit errors before mutation. Opaque preservation is not typed graph
editing. Native calamine/rust_xlsxwriter comparisons and broad performance/RAM
acceptance remain scheduled; the public workload is not a universal speed claim.
