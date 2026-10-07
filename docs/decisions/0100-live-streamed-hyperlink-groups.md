# Live streamed hyperlink groups

Sequential output now offers writer-local mutable hyperlink group identities.
Rows retain fixed-width owner/group events in per-sheet temporary files; group
payloads live in an append-only value file with a fixed-width random-access index.
An update replaces the indexed payload without retaining a workbook-sized RAM
map. Multiple owners can share a group, including owners in closed worksheets.
Appended cell values remain snapshots; later metadata updates do not refill them.

Packaging evaluates current group payloads in sheet creation and append order.
It streams declarations into the worksheet footer and relationships into their
own package part, using the shared hyperlink encoder. A single bounded decoded
payload cache avoids repeated group decoding for adjacent aliases. The optional
finish identity visitor reports final public IDs in actual output order; callers
apply them only after the whole save succeeds. The visitor declares its retained
workspace so requests/results share the writer metadata allowance. A bounded
current-group read supports transactional adapter rollback without a resident map.

Buffer/path/payload accounting participates in writer metadata budgets. Generated
parts enforce byte limits, temporary storage counts index/payload/event files,
and abort attempts cleanup of all owned files. An I/O failure poisons the writer.
Payload updates retain old versions on disk until cleanup; heavy mutation has an
explicit temporary-space cost. This is a deliberate first implementation,
without a measured speed or RSS claim.

Snapshot and live declarations cannot be mixed on one sheet. Live groups with
prebuilt model/printing footers are explicitly rejected; current Python write-only
sheets use ordinary sequential footers. Such native combinations remain tracked
capability work rather than silently discarding either feature.

Rust 1.88 library compilation and formatting are the implementation checks.
Python live object integration and consolidated A11 pre-release acceptance remain
required; no new tests or benchmarks run at this checkpoint.
