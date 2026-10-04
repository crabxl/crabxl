# Third-party provenance

Selected algorithms and parsing flows were adapted from the pinned calamine
source listed in [ports.json](ports.json). Original notices are retained in the
adapted source files, and the MIT license is distributed under licenses/.

calamine and rust_xlsxwriter are not runtime dependencies or wholesale vendored
libraries. Selected rust_xlsxwriter scalar/formula/style XML layouts, date conversion, sequential spooling and packaging flows are now adapted into the XLSX writer. Its MIT license is distributed under licenses/ and notices are retained in adapted source files. Full upstream model/style/feature imports remain deferred.

Selected openpyxl 3.1.5 tests, their provenance records, verifier and MIT notice belong to the standalone [Python adapter repository](https://github.com/crabxl/crabxl-python/tree/main/third_party). Run its verifier inside that repository; no Python adapter implementation is included in the Rust workspace. Selected test assertions remain unchanged; the complete upstream suite is not claimed.

Selected umya-spreadsheet run/font/color layouts are adapted into shared typed rich text and bounded XLSX codecs. Its pinned source, substantial refactoring, independently implemented phonetics, integration evidence and MIT notice are recorded in ports.json and licenses/umya-spreadsheet-MIT.txt. It is not a dependency or a second workbook engine.

Composable managed ReadOptions and projected shared-template tail retention are original integration changes to the existing attributed readers. Verification uses synthetic OOXML and public openpyxl 3.1.5 calls, without further upstream implementation inspection; see ADR 0022 and benchmarks/m2-policy-options.md.

Cache-only formula projection is an original change to the attributed scalar reader, verified through synthetic fixtures and public openpyxl 3.1.5 observations; see ADR 0023 and benchmarks/m2-formula-cache.md. No additional upstream implementation was inspected.
