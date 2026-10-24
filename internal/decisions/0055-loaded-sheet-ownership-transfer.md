# ADR 0055: Atomic decoded worksheet transfer into the canonical bank

## Decision

`Workbook::adopt_sheet` transfers an already decoded `Worksheet` into the existing
canonical bank. `replace_sheet` commits a separately decoded model at an existing
stable `SheetId` and returns the former model. Both use existing naming, style,
per-sheet and aggregate byte/cell checks; cell payloads are not cloned. Incoming
replacement names must match the registered model. Removed/foreign identities
remain invalid. The returned former model belongs to the caller and leaves the
bank's retained accounting.

`remaining_bytes` reports unused managed bank space for a format coordinator to
bound temporary decoding while existing models remain resident. It does not
include ZIP/parser working storage or original-package catalogs, strings, caches
and overlays; those still require coordinated reservation. Receiving a decoded
model does not infer package ownership or silently remap imported style IDs.
Initialized catalogs validate incoming cell IDs before commit; coordinators must
retain matching source identities when catalog adoption is deferred.

Failed validation leaves logical bank models and handles unchanged. Incoming
caller-owned models are consumed and dropped on failure. As with existing sheet
creation, a successful capacity reservation before a later allocation/aggregate
failure may remain charged. No source archive is consumed by these APIs.

## Verification

The existing workbook integration suite covers pointer-preserving text transfer,
stable placeholder replacement, incoming per-sheet cardinality rejection,
aggregate accounting, name/duplicate validation, source append extent, removed
identity rejection and invalid imported style references. Format loading and
Python integration follow as separate A6 checkpoints. This foundation alone does
not complete M4 or make loaded structural editing available.
