# ADR 0010: Owned rich text with explicit projections

Status: accepted for the M2 rich-text checkpoint; not a completed style catalog.

## Decision

Core owns rich runs, optional run font overrides, color identities/tints and separate pronunciation annotations. Optional properties distinguish inherited values from explicit false. Rich text contains no duplicate flattened string. Borrowed text iteration and checked linear projection are available. CellValue retains its compact tagged representation by boxing rich values.

ReadOptions.rich_text defaults to false, matching the reference default display-text projection. This path skips formatting models and uses compact shared-string slots. Explicit true retains runs and metadata. First typed access upgrades a previously prepared plain SST by dropping and rebuilding it; subsequent plain access can project typed entries. This may incur a second scan and is visible in SharedStringStats.rich_text_preserved.

Shared-string Memory/Disk/Auto placement, complete fixed-width disk index, byte-bounded cache, temporary quotas and RAII cleanup apply equally to typed rich data. Disk rich payloads use streamed internal XML fragments, not a second workbook implementation. The currently requested result is owned and survives workbook/cache removal. These remain component allowances rather than a process RSS cap.

## Observable semantics

Default inline projection preserves text and OOXML-looking markers. Typed inline reads remove x005F_ protection within individual runs (or a lone text element). Public openpyxl 3.1.5 SST behavior removes x005F_ protection markers after flattening in default mode, but separately within each run in typed mode. Consequently markers crossing run boundaries can produce different display strings between modes. Retain raw SST text until projection; do not globally decode escape-looking sequences.

Rich text creation writes inline runs, preserving explicit false font properties, color references, meaningful whitespace, empty styled runs and phonetic metadata. Ordinary editing may replace existing rich literals while retaining unrelated original package parts. Formula caches cannot contain rich text.

## Boundaries

Font/theme/indexed-color catalog resolution remains part of M2 style loading and M5 theme support. Color references are retained rather than eagerly rendered. New workbook phonetic font references must identify a registered font. Existing-package assignment containing new phonetic settings is rejected until imported font catalogs can validate references. Original unchanged package parts can still preserve those settings.

Unknown rich extensions can be projected to display text; typed access rejects them explicitly. Preservation does not count as editable support. Python rich_text=True still requires compatible Python rich classes and conversion; default text projection can use the canonical core. This checkpoint does not complete M2 or all openpyxl rich API compatibility.

## Evidence

Generated integration fixtures cover inline/shared text, policy placement, metadata upgrades, protection boundaries, malformed properties, illegal characters, ownership, repeated editing and atomic writer failure. benchmarks/rich_text_checkpoint.py separately verifies every rich run and specified font/color override against public openpyxl 3.1.5, and compares flat projections with calamine only on shared capabilities. Historical shared-string measurements remain separate.
