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

Formula attribute write policies are original shared-model/codec integration changes to the previously attributed formula layouts. Public constructor/save/reload evidence and synthetic fixtures verify omission versus source retention without additional upstream implementation inspection; see ADR 0024.

Owned bank theme integration and immutable shared ownership are original Rust model/resource work, with synthetic lifecycle/limit fixtures and native snapshot verification; existing opaque theme serialization provenance remains unchanged. See ADR 0026 and benchmarks/m4-bank-themes.md.

Canonical style catalog adoption, sparse ID reservation, raw format registration and consuming reader transfer are original ownership/resource integration changes to previously attributed shared models and codecs. Public model calls and generated OOXML verify behavior without additional upstream implementation inspection; see ADR 0027 and benchmarks/m2-style-import.md.

Source catalog export, source-derived automatic date identities, sorted sparse number-format insertion and aggregate raw-registration wrappers are original integration changes to the attributed core/style/writer models. Public save/reload uses generated fixtures and no additional reference implementation inspection; see ADR 0028 and benchmarks/m2-style-export.md.

Finite style domain corrections, public color identity selection and gradient preparation scratch accounting are original shared-model/codec changes. Evidence uses synthetic XML and public openpyxl constructor/save/reload calls, with no additional implementation inspection; see ADR 0029 and benchmarks/probe_finite_style_domains.py. Existing model/codec attribution remains unchanged.

Canonical owned-bank styles, aggregate registration wrappers and consuming bank/registry transfer are original ownership/resource integration changes. Shared style-combination benchmark fixtures are original generated data. Public full-model save/reload checks use openpyxl 3.1.5 without implementation inspection; existing attributed models/codecs remain canonical. See ADR 0030 and benchmarks/m4-bank-styles.md.
