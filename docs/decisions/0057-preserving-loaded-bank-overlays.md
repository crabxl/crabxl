# ADR 0057: One source coordinator for loaded models and preserving overlays

## Decision

`LoadedWorkbook` now owns the existing `WorkbookEditor` rather than a parallel
reader/source. The editor's reader transfers source style ownership to the same
canonical bank and decodes source worksheets against that catalog. Source part
inventory, relationship/calculation catalogs, overlay nodes/payloads, SST/cache,
source handles and bank models all participate in the loaded joint allowance.
`LoadOptions.editor` supplies overlay/output policies; its resource and memory
policy fields are overridden by the coordinator's canonical enclosing settings.

`set_value`/`upsert_value` preserve the existing editor's cell/value semantics and
source graph guards. Unmaterialized sheets keep only pending overlays. Cached
models receive successful updates; models loaded later apply the same overlays
before stable-ID commit. Until finer shared immutable value ownership is added,
materialized values and preserving overlays own separate payloads and both are
charged, including incoming clone work. Resource reservations precede cloned
payload/node growth; Auto SST spilling and cache shrink can provide room.

The editor separates pure patch validation/planning from infallible logical
commit. The coordinator can reserve the planned final overlay cost, perform
atomic bank mutation, then commit that validated overlay without a second
fallible source-policy transition. Failed limits leave both logical values
unchanged; cache placement or conservative capacity reservations may change.

Save reuses the original package editor and its bounded event codecs. Source
materialization alone is not dirty. Scalar edits retain source appearances,
untouched rich/unknown content and reusable binary sources; repeated saves do
not consume source assets. Path output uses an adjacent guarded temporary ZIP
and replaces the target after success. Invalid settings/output failure preserve
the source and pending values for retry. Borrowed caller sinks may contain
partial output. `into_source` releases bank/cache resources and transfers the
original caller-owned source.

Original chartsheets/dialog sheets retain opaque stable catalog entries. Their
typed model access remains unsupported, while unrelated worksheet access and
edits stay available. Saving preserves their original parts without interpreting
or regenerating them.

## Boundaries

Typed date assignment, source style registration, loaded structural/sheet edits,
complete feature graphs and Python integration remain the next A6 checkpoints.
The existing editor's signed-package and affected formula/metadata restrictions
still apply. Edited data-only output remains explicitly unsupported. These
restrictions do not narrow the final A6 or baseline scope.

## Verification

The loaded integration suite now batches lazy overlays, cached-model updates,
missing-cell insertion, max-overlay-cell failure atomicity, materialization after
assignment, repeated save/reload, unaffected binary preservation, original source
transfer and invalid-compression target/temp protection. Loaded tests also
verify unrelated worksheet edits beside an opaque chartsheet with its XML
preserved exactly. The existing source editor
fixtures continue to verify namespace, calculation, signed/annotated, sparse
row metadata and repeated-source behavior. Joint low-memory RAM/Auto/disk cases
remain in the same suite. Native checkpoints use Rust 1.88 and current stable;
relevant default/native-zlib cases pass. Performance evidence is recorded with
checkpoint scope; no full M4 closure is claimed.

The [native coordinator measurements](../../benchmarks/alpha6-loaded-coordinator.md)
include equivalent full-model loading and complete edit/save/reload verification,
with raw wall/CPU/RSS, retained allowances, output bytes and cleanup checks.
