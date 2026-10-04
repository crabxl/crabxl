# ADR 0030: Canonical owned-bank style registration and export

## Decision

Workbook owns an optional canonical StyleRegistry initialized by explicit source import or lazy appearance registration. Source IDs/components remain stable; repeated registration shares canonical records. Imported cells must resolve against source tables before adoption. Replacement of an initialized catalog is rejected because remapping source IDs requires a separate explicit operation.

Styles, theme, sheet slots and retained worksheet models share the bank's managed allowance. Borrowed sheet editing subtracts style/theme storage from its remaining allowance. Registration uses the remaining aggregate budget while preserving the caller's independent component cap, allowing subsequently released sheet storage to become available. Failed operations preserve logical cells/catalogs; capacity reserved before an unsuccessful registration may remain charged, as in existing registry behavior.

Workbook::into_parts consumes the bank and transfers its existing sheet entry allocation, style registry with indices, epoch, active index and immutable theme ownership. WorkbookWriter::from_workbook uses those owned components without cloning payloads or rebuilding the registry. It revalidates export compatibility and applies writer byte/record limits before adding automatic date formats. Consuming export is distinct from repeated non-consuming saves; the older borrowed write_workbook explicitly rejects a bank with its own catalog rather than interpreting its IDs against unrelated caller registrations.

This API creates a new package. Lazy loaded ownership, repeated-save snapshots, original feature graphs and complete metadata schemas remain required. No milestone completion or Python surface support is claimed.

## Verification

Tests cover pointer-preserving import and export, shared identities, invalid existing cell IDs, failed import/registration, aggregate sheet/theme rejection, ownership iteration, exported values/styles/epoch/active order and early borrowed-export rejection. Representative explicit full-model creation/readback measurements are in benchmarks/m4-bank-styles.md; streaming modes are recorded separately rather than used as the full-model comparator.
