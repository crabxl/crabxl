# ADR 0093: disk-backed hyperlink metadata for sequential output

Status: native snapshot API implemented; Python write-only live integration remains open.

## One writer owner and shared codecs

WorkbookWriter::write_row_with_hyperlinks accepts one bounded sparse row and its
point metadata. The canonical hyperlink validators and record/relationship codecs
serve owned exports, preserving edits and this stream; no second XML encoder exists.
Independent references remain supported. The API snapshots its borrowed inputs.
Prebuilt model/printing footers reject additional links explicitly until schema
composition supports that combination.

Each active/paused sheet lazily owns two buffered NamedTempFile spools: declaration
records and external relationships. Row batches encode into one bounded scratch
buffer, without retaining a worksheet model or a growing collection. Fixed spool
buffers and paths participate in active/paused catalog allowances. Scratch/new
buffer reservations reduce style-registration headroom before row encoding.

Row preflight reserves future footer copying and relationship closing bytes under
both aggregate temporary and per-sheet limits. Invalid rows or resource failures
do not partly commit the requested row, identities or metadata. Pending dimension-only
rows retain the existing sequential emission policy; whole-call rollback of earlier
implicit rows is a separate writer transaction dependency. After a row succeeds, an I/O failure
poisons the writer; abort retains cleanup paths for failed deletion retries.

Closing streams declarations into the worksheet in schema order and transfers the
external relationship file into its completed sheet. Location-only streams omit
relationship parts. Packaging uses final creation/display indexes rather than close
order. Finished declaration files are explicitly retired; abort attempts cleanup of
worksheet and metadata files, including paused sheets. Repeated abort is harmless.
Logical temporary accounting includes buffered bytes and the copy overlap at close.

## Evidence and remaining gates

One integration scenario verifies interleaved creation/close order, 3,000 declarations
under a 128 KiB metadata allowance, invalid-row and temporary-budget retry, location-only
output and complete save/abort cleanup. Existing sink-failure and cleanup-retry
scenarios also include hyperlink metadata files without changing their assertions. Rust 1.88 workspace tests and Rust 1.99 strict
Clippy pass. [Release measurements](../../benchmarks/alpha11-stream-hyperlinks.md)
include time, process high-water RSS, logical temporary space and independent complete
openpyxl value/target/identity readback for six generated outputs.

Python WriteOnlyCell has additional live-reference behavior after append, including
reused/shared objects and generated IDs after save. A snapshot adapter must not claim
that behavior: its native mutable metadata/group coordinator remains a binding gate.
Finite ranges, source empty-anchor integration and complete W13/A11 acceptance also
remain open. No A11 version bump or publication is made by this checkpoint.
