# Source hyperlink display values

`LoadOptions.bind_hyperlink_values` enables normal editable-model initialization
of empty hyperlink cells. Rust defaults to preserving physical source values;
the ordinary Python adapter will enable the compatibility policy. Read-only
streams retain their existing behavior.

Initialization occurs in the unpublished incoming worksheet before its stable-ID
commit. Existing values, queued explicit value overrides and merged non-anchor
cells are retained. Non-overlapping rectangle declarations can produce physical
display cells in this explicit materialized mode, with one immutable shared UTF-8
payload per declaration rather than a copied URL in every cell. Hyperlink metadata
itself remains compact. Managed byte/cell limits, initializer scratch and metadata
headroom are checked; a failed incoming model is discarded.

Initial display text follows the common 32,767-character clipping rule, including
owned and detached assignments. Source saves materialize unrequested sheets only
when a metadata scan finds declarations, then queue initialized values through the
existing guarded model-save coordinator. Source/footer graph dependencies remain
explicit staged errors. Value changes retain the original initialized display
when a link target changes later.

Overlapping source declarations now use ordered replay into the private incoming
model. Later declarations still replace canonical metadata, while only the first
nonempty initializer fills an empty value. Destination-free declarations extend
logical cell presence without constructing every covered empty coordinate. Queued
value overrides and merged non-anchor cells retain the same exclusions.

The shared declaration codec supplies validated source-order events without
retaining a second link collection. A bounded relationship map resolves original
identities, including links discarded by final coverage normalization. Scan
buffers, relationship storage and current declaration scratch reduce the model's
available allowance. Failure discards the unpublished model. This replay requires
an additional worksheet decompression pass only for overlapping declarations;
ordinary non-overlapping and read-only loads retain their existing paths. Avoiding
that exceptional replay pass remains a performance opportunity.

Python enables the compatibility policy for ordinary editable loads. Normal
post-save public identity synchronization is implemented separately in ADR 0098.
Rust 1.88 library compilation and formatting pass. Consolidated resource/failure,
ordering and compatibility acceptance remains deferred until the entire A11
native and Python feature implementation is complete.
