# ADR 0070: Source sheet display order

## Decision

Retain the original source catalog/part identities and maintain a separately
bounded original-index permutation for current display order. Cache only the
original sheet declaration start tags, preserving relationship IDs, sheet IDs,
namespace bindings and unknown attributes. No worksheet XML, cell models or
binary assets are copied to reorder sheets. Repeated operations reuse this
declaration cache and replace one prospectively bounded index vector.

The canonical bank and preserving metadata overlay commit together after
package/graph and joint allowance checks. Preserve the signed active display
index across reorder, matching the pinned reference's observable behavior.
Explicit stable-ID active selection resolves the new display position. Name and
visibility patches remain keyed by original source identity; save applies them
once while replaying declarations in display order. Formula caches/chains,
worksheet parts and unrelated opaque content remain unchanged.

Reject affected workbook alternatives, signed packages, nested/foreign sheet
catalog content and local defined-name graphs before changing order. Local name
owner-index remapping remains tracked under M5 and deferred M4 acceptance;
preserving its old index would silently move the name to a different sheet.
Global name expressions are unaffected and remain preserved. Source metadata is
checked against existing XML/metadata limits; declaration and operation scratch
storage are charged inside the existing managed allowance, not a process RSS cap.

This coordinator and checked catalog replay are original CrabXL implementation.
The pinned umya 3.1.0 workbook collection/identity mutation was inspected as the
primary feature source; it does not supply this source-preserving transaction.
No second workbook model or wholesale upstream engine is introduced.

## Verification

Extend existing loaded metadata workflows across strict/prefixed namespaces,
custom parts, opaque extensions, repeated moves/saves, current-title and
visibility patches, active selection and stable model handles. Verify unchanged
part bytes and explicit atomic local-name/invalid-position/signature/budget
failures. Run the workspace, strict Clippy and Rust 1.88 loaded checks. Measure
release lazy reorder/rename/select/save/full streaming readback independently
from parser and Python performance. Remaining M4 creation/copy/removal and
row/column edits are separate gates.
