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

Overlapping source declarations need separate original-order display binding:
later links replace metadata, but an earlier link can already have filled a value.
The canonical collection records overlap history. Empty-value binding in such a
source returns explicit Unsupported until ordered initialization is integrated;
already populated values and the Rust preserve-values mode remain usable. This
case remains an A11 gate, not a completed capability.

Rust 1.88 library compilation and formatting pass. Python policy integration,
ordered overlap binding, resource/failure assertions and measurements belong to
the remaining implementation and consolidated pre-release acceptance stages.
