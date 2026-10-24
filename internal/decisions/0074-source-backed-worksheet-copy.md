# ADR 0074: Source-backed worksheet copy

Status: implemented for the staged supported subset; removal is added in ADR 0075.

A requested copy uses `Workbook::copy_sheet` to duplicate the canonical cells
under aggregate/per-sheet allowances. This is the user's requested result, not a
rollback clone or a second engine. Its new source-backed identity uses the live
catalog transaction introduced in ADR 0073. Current overlays/model edits are
included; later edits to the copy do not change its source.

A compact immutable original-source index identifies the XML template. Each save
streams that template, replaces sheetData with the borrowed copy model, updates
dimensions, and preserves supported worksheet properties and printing metadata.
Copying a copy keeps that original template identity. No worksheet XML is cached
and unrelated sheets remain lazy.

Pinned openpyxl 3.1.5 public probes confirm that views and header/footer state are
not copied, while sheet properties, format and printing settings are copied.
The serializer omits those excluded elements. Native owned visibility remains a
core policy; the Python adapter must set copied worksheets visible, matching the
public reference. No implicit cross-sheet formula/name rewriting is promised.

Affected feature graphs, rich/shared formula structures, worksheet relationship
identities, VBA code names and unmodeled property descendants reject before
registration. Typed-date values already classified by the imported style catalog
retain their source style IDs; new registration remains M5.

Coordination and codecs are original CrabXL code. The umya-spreadsheet 3.1.0
workbook/worksheet copy model at commit
`aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b` remains the primary inspected reference;
no implementation is imported here. Source graph interactions stay tracked for
M5/M6 instead of silently losing content.

The existing loaded workflows cover source edits, copy independence, copying a
copy, source printing/header/view behavior, repeat readback, aggregate failure
atomicity and affected-graph rejection. This checkpoint does not close A7.

A7 integration/publication follow-up: the matching Python operations and staged
release acceptance are verified in
[the A7 audit](../validation/alpha7-m4-stage-acceptance.md). Remaining affected
feature-graph transformations still belong to the tracked M5/M6 dependencies.
