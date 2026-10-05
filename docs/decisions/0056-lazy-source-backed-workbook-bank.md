# ADR 0056: Lazy source-backed canonical workbook loading

## Decision

`LoadedWorkbook` owns the seekable source, a canonical `Workbook`, stable source
sheet handles, and one resolved joint memory allowance. It creates named empty
bank placeholders without worksheet decoding; `sheet(id)` decodes the selected
full sheet into a temporary `Worksheet` and atomically replaces its placeholder.
Repeated access borrows that committed model. Loaded source IDs and epoch remain
stable. The current API deliberately exposes read-only models until preserving
mutation coordination is implemented; this checkpoint does not complete A6/M4.

The source `StyleCatalog` moves into the bank's existing `StyleRegistry`, without
cloning its component payloads. The source decoder retains its bounded derived
date-kind table and borrows the bank catalog when streaming original cells.
Registration indices remain bank-owned. This internal transfer cannot be used by
ordinary reader callers to silently substitute unrelated source identities.
Normal `WorkbookReader` APIs retain their existing ownership and behavior.

## Joint resources

The joint retained cap is the smaller of the resolved operation allowance and
configured workbook ceiling. Working reserve stays separate. Account source
names/catalogs, date classifications, style registry and indices, sheet slots,
source handle/name storage, committed models, incoming sparse nodes/payloads,
current decoded row capacity, SST RAM/index data and disk caches. Source metadata
and style preparation receive prospective bounds before loading their payloads.
ZIP/dependency/allocator overhead, caller-retained outputs and OS cache remain
additional; this is not a whole-process RSS cap.

The decoder reserves existing bank/handle storage before preparing an SST, so
forced RAM cannot consume the same allowance as existing models. Subsequent row
commits adjust the shared read pool before additional retained growth. Auto can
spill a previously RAM-resident table to disk and disk caches can shrink. Do not
freeze the incoming sheet ceiling to a pre-spill/cache snapshot. Forced Memory
remains strict and can reject a later sheet even when its own per-sheet limit
would fit. `Workbook::set_memory_allowance` updates the bank's remaining source-
reserved ceiling atomically without changing per-sheet/cell limits.

Source parse/limit errors discard the incoming model and retain the placeholder
and previously committed sheets. Bounded prepared SST/cache changes may remain
for retry; owned files close on coordinator destruction or source transfer.
Coordinate projection is incompatible with a full-sheet model. Typed chartsheet/
dialog models are still unsupported. Original package editing, new/source sheet
mixing, property graphs, style mutation, file-like Python APIs and model save
coordination follow separately. Read-only models do not imply edit/preserve
acceptance for advanced graphs.

## Verification

Two generated integration cases batch stable lazy source handles, original style
pointer identity, dates, repeated access/source transfer, aggregate cell rejection
and retry, and RAM/Auto/disk SST policies under one 4 MiB operation allowance.
The 3,000 unique 128-byte entries shared by two sheets force Auto spilling; forced
RAM rejects the second model without changing the first. Real disk lookup/output,
temporary-byte counters and Linux open temporary handles are verified; Drop
cleanup is checked separately. Existing reader/editor tests remain unchanged.

The `loaded_rows` example and `alpha6_loaded_bank.py` compare this canonical
ownership path with the previous standalone model algorithm on the same current
parser. Measurements include full native process RSS, time and verified integer
checksums. They do not compare Python calls or streaming mode and make no
optimization or full milestone-completion claim.
