# ADR 0091: preserving point hyperlink edits and source relationships

Status: implemented native checkpoint; complete W13/A11 acceptance remains open.

## Ownership and preservation

LoadedWorkbook mutates the canonical worksheet collection. A sparse editor ledger
marks affected source parts; it does not duplicate hyperlink models. Save prepares
bounded identity plans before writing output. Unchanged external targets reuse valid
source relationship IDs. Changed targets receive collision-free IDs, while unrelated
relationships, opaque assets and unknown relationship attributes remain preserved.
Existing unused relationships remain intact because unknown consumers can share IDs.
Missing relationship parts and their content-type default are created when needed.
Repeat saves start from the original source, avoiding accumulated additions.

Hyperlink XML scheduling shares the bounded worksheet rewriter. Borrowed canonical
metadata and prepared IDs are emitted in worksheet schema order. A nonempty cell's
value is unchanged when its target changes; empty anchors receive the target or
location through one core policy. Clearing metadata retains the physical value.
Pure hyperlink edits preserve formula caches. Supported source copies regenerate
identities independently, and clearing all copied links removes the template body.
Physical removal retires point metadata; affected unsupported structural operations
still reject explicitly. Graph preflight runs before typed materialization, retaining
lazy failure behavior for unsupported source graphs.

## Resource and verification boundaries

Identity vectors, text, encoded relationship capacities and save lookup entries share
source/model allowances. Relationship decoding remains bounded and common to readers.
Two end-to-end scenarios exercise shared IDs, collisions, missing graphs, copies,
removal, repeated saves, cache preservation and unchanged opaque payloads. Rust 1.88
workspace tests and Rust 1.99 strict Clippy pass. Independent openpyxl readback checks
all six generated benchmark outputs; [measurements](../../benchmarks/alpha11-source-hyperlinks.md)
record the complete native edit phase and process high-water RSS.

Range declarations, mutable declaration references and aliases, Python live objects,
full strict-schema write acceptance and internal package graph editing remain tracked.
This checkpoint does not release A11 or claim complete hyperlink compatibility.
