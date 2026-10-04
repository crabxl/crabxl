# Canonical owned-bank style creation and export

Workbook now owns the same canonical style registry used by readers/writers. Source adoption retains component/format IDs; full appearance and raw format/code registration share aggregate sheet/theme/catalog limits. Consuming export transfers the existing catalog, hash indices and sheet entry allocation without payload snapshots. Pointer tests verify font-name allocation identity across import/bank/writer ownership; this is not a claim that the whole workflow performs no allocation.

## Equivalent public full-model workload

openpyxl 3.1.5 and Rust both create and retain an explicitly owned worksheet containing one floating value per distinct font/fill/alignment combination, then save it. Public reload checks every value, number format, font, fill, alignment and protection property; canonical streaming reads verify every cell outside timing. Native additionally re-registers each style to assert stable IDs; public reference shares component objects and interns formats during save. Native retains four extra date presets and emits larger style XML, so equivalent public properties do not imply identical internal work.

Five rotating serial samples plus warmup include process baseline, CPU and peak RSS on the current Linux environment. Builds and readback are excluded from timing. Run benchmarks/bank_style_checkpoint.py with release bank_style_fixture/style_registry_read examples and CARGO_TARGET_DIR. Raw samples and metadata sizes are in results/m4-bank-styles.json.

| Cells/combinations | Native seconds / KiB RSS | openpyxl seconds / KiB RSS | Native managed bank bytes | Native exact spool bytes |
| --- | --- | --- | --- | --- |
| 1,000 | 0.035261321 / 2,656 | 0.241146454 / 39,336 | 444,295 | 55,736 |
| 8,000 | 0.263649079 / 4,576 | 0.594514435 / 73,624 | 3,473,447 | 475,736 |

Native wall time and RSS meet the reference targets for this workload. These are explicit full-model results and should not be substituted for streaming measurements. Public temporary-file sampling is every 25 ms; medians 0 and 476,408 bytes can miss peaks and are not exact totals. All native-owned temporary files are verified cleaned up. Final ZIP files are excluded from spool accounting. Managed bank bytes include conservative capacities/indices/slots, not allocator overhead or a hard process RSS cap.

No earlier native API exported a canonical styled bank, so no prior equivalent baseline is fabricated. This checkpoint claims neither calamine nor rust_xlsxwriter overlap acceptance. Loaded lazy ownership, repeated non-consuming saves, original-package graphs, differential/table style schemas and additional features remain staged. Borrowed write_workbook now rejects a bank with its own catalog before any sheet spool instead of interpreting its IDs against unrelated writer registrations; use explicit consuming from_workbook for this supported mode.
