# ADR 0007: Owned workbook bank and aggregate allowances

Status: accepted for explicit owned scalar/formula models; loaded feature graphs remain staged.

## Decision

Core owns an I/O-free Workbook containing the existing sparse Worksheet models. Display order, active selection and calendar epoch belong to the workbook. SheetId is a private owner/monotonic-serial pair: rename/reorder preserve identity, deletion invalidates it, and another workbook rejects it. The IDs are runtime handles, not OOXML relationship IDs. A checked atomic owner counter supplies uniqueness without global model or resource caches.

WorkbookLimits constrain aggregate managed bytes, cell cardinality and sheet count, with additional per-sheet limits. Conservative accounting charges existing worksheet cells/payloads plus 256 bytes per allocated sheet-bank slot, including retained unused capacity. Removed models transfer to the caller and leave the bank's accounting. These estimates are not allocator measurements or a process RSS ceiling.

A WorksheetEditor mutation facade borrows the workbook exclusively and caps its selected sheet to remaining aggregate capacity. It exposes immutable borrowing and explicit checked mutations, but no unrestricted mutable replacement. Dropping restores the sheet's personal ceiling, so later operations can reuse space freed elsewhere. Structural operations include the existing transient work allowance. Callers can retain one facade while filling a sheet to avoid repeatedly scanning the sheet bank per cell.

Copies are explicit independent owned copies, preserving cell values, styles, normal formulas and logical append extent. Aggregate/per-sheet capacity is checked before cloning payloads. This does not copy drawings or any original package feature graph. Rename, reorder, remove and active selection operate on stable IDs. The low-level format-neutral bank requires nonempty unique names; the XLSX codec enforces XLSX-specific name constraints.

WorkbookWriter borrows bank models through the existing cell encoder; it does not clone cells or create a second workbook representation. The writer uses the bank's order, active index and epoch. Style IDs still require registration in the writer's shared style catalog; a complete owned workbook feature/style catalog remains planned. Invalid active indexes fail before ZIP output.

## Binding and existing-file boundaries

The Python compatibility adapter now reads and writes active-sheet metadata through existing compatible calls. Its owned sheets still use the earlier native sheet handles with per-model allowances; migration to the aggregate bank remains required. Do not claim that max_memory_bytes currently caps all Python sheets together.

WorkbookEditor remains the lazy original-package preservation path, using the same core values. Structural changes to existing feature/reference graphs are not implemented by exporting a newly created bank: doing that would lose unsupported original parts. Existing-file structural editing needs coordinated part/relationship changes and typed feature handling before it can be advertised.

## Verification

Tests cover independent copies, logical extent, owner/removed-handle rejection, rename/reorder, epoch/active output, global bytes/cell limits, transient-operation failure, failed mutation atomicity, and reuse of freed capacity. Python parity covers active selection and readback. Release evidence includes explicit model-copy costs and default writer regression rather than claiming copying is constant memory.
