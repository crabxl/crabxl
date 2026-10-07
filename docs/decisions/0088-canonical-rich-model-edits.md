# Canonical rich-value model editing

Status: usable W13 checkpoint; hyperlinks and consolidated A11 remain open.

Existing canonical `RichText`, `RichTextRun`, `RunFont` and phonetic records remain
the only engine representation. The existing namespace-aware, byte-bounded run
codec now validates supported inline and shared content before loaded-model
rewrites. No new upstream implementation is imported. Sources decoded with plain
projection still reject affected rich model rewrites rather than losing runs.
Unknown affected elements and attributes retain explicit errors.

Loaded editors receive the current canonical font count before value/row edits.
Known phonetic identities can be retained and edited; invalid identities reject
before mutation. Bare original-package editors without an imported catalog retain
their unsupported phonetic-assignment error. Source font identities and shared
run formatting remain separate from cell appearance IDs.

`ReadOptions.inline_rich_text` optionally overrides inline preservation while
`rich_text` continues to select shared-string run preservation. None keeps the
existing unified native behavior. The Python read-only adapter selects plain
inline projection and typed shared strings, matching observed openpyxl 3.1.5
behavior. Ordinary Python loading preserves both when `rich_text=True`.

Python `CellRichText`, `TextBlock` and `InlineFont` are editable views. Their weak
owner/coordinate registrations do not retain a second worksheet or a permanent
copy of native run payloads. Shared values update their live owners; field/list
failure restores UI state and already updated values. Overwritten/deleted views
detach, while row/column movement relocates held owner registrations. Nested font
and color edits use the same notification path. Write-only rows snapshot values
at append and account for nested rich payload bytes.

Native row projection emits deferred markers for live rich/structured values,
avoiding an unused full conversion before the cell view is requested. Scalar
rows keep their existing conversion. Input runs reuse immutable validated native
font components, and output shares trusted font/color materialization. Public
input validation remains canonical. Selected public list mutation is supported;
complete XML-tree utility APIs, copied-sheet alias semantics, detached range
utilities and remaining A11 structural interactions require acceptance work.
This checkpoint does not mark the complete rich-text public module verified.

Existing native streaming/loaded scenarios cover independent inline policy,
plain-mode refusal, relationship-resolved rich SST model edits, copy and repeated
output. The Python shared scenario covers ordinary/source-backed/streaming values,
disk SST, live nested/shared edits, phonetic preservation and reference reopening;
one resource scenario verifies multi-owner failure and retry. Full native tests
pass with Rust 1.88, strict Clippy passes with Rust 1.99, and the CPython 3.12
development wheel passes 552 cases with Ruff check/format.

[Rich-view measurements](../../benchmarks/alpha11-rich-views.md) record creation,
typed load/scan, structural edit, serialization and RSS separately, including
remaining Python assignment and structural-validation costs.
