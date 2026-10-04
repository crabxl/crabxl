# ADR 0037: Known formula replacement accepts literal flags

Replacement analysis now distinguishes known record validation from strict Boolean interpretation. It accepts raw flag strings for known array/data-table records, so a loaded public property can be edited after a compatible read. Both paths use the same canonical formula header decoder and literal flag model; no editor-only flag parser is introduced.

KnownRecords replacement keeps unknown encoding types and unknown attributes unsupported, and preserves the existing shared-group replacement rejection. Compatible visible reads still discard unused hints; ValidateGroups still rejects non-Boolean flags. These are explicit different guarantees, not an automatic relaxation of graph editing. XML/entity and byte checks apply throughout.

Tests cover repeated replacement of opaque/escaped data-table flags and array hints, correct new literal flag readback, unrelated scalar sheets and cells, and continued rejection of unknown records/attributes/shared groups. Existing global calculation-cache invalidation remains intentional; untouched scalar sheet assertions do not imply preservation of stale formula caches.

Release measurements replace one opaque-flag table formula in 5,000/50,000-cell sources and verify all remaining public flags after both engines save. Native uses lazy original-package editing; openpyxl uses a full owned model. Output/atomic ZIP staging is excluded from worksheet temporary sampling and disclosed separately. See benchmarks/m4-literal-flag-edit.md. This is a narrow M4 checkpoint, not structural feature-graph completion.
