# Optional Python compatibility adapter

The public interface targets openpyxl-compatible migration, not a second Python API. Supported code can replace `import openpyxl` with `import openrsxl as openpyxl`. Full compatibility remains a staged requirement. This package never calls openpyxl at runtime.

```sh
python -m pip install maturin "pytest>=8,<9" "openpyxl==3.1.5" Pillow
maturin build --release --locked --manifest-path bindings/python/Cargo.toml --out bindings/python/dist
python -m pip install bindings/python/dist/*.whl
python -m pytest bindings/python/tests -q
```

Builds require Rust 1.88 and Python 3.10 or newer. CPython 3.12 is locally verified; CI also checks 3.10. The adapter is excluded from the core Cargo workspace, has its own lockfile and can be relocated without making Rust crates depend on Python.

Verified calls include Workbook, active selection and load/save readback, create_sheet/remove/index/move_sheet/copy_worksheet, sheetnames/indexing, Worksheet/Cell indexing, one-based cell(), value/data_type/coordinate, append with lists/dictionaries/generators, iter_rows/iter_cols/values, finite insert/delete/move, model titles and path save. Cached Cell views follow moves and detach on deletion/overwrite. Loaded load_workbook and scalar/formula cell assignment use the original-package editor, including sparse missing-cell insertion and repeatable saves preserving original assets.

Default `max_memory_bytes=None` uses Rust Auto availability policy. For a newly created Workbook, an explicit integer caps the Rust bank's aggregate managed model/work allowance across registered sheets, including allocated bank slots. Auto selects this same aggregate allowance. It is not a whole-process RSS cap: Python objects, caller values, dependency allocations and temporary output are additional. Standalone Worksheet objects and removed sheets retained by callers are outside the bank. Loaded workbooks still use per-model/overlay operation allowances; their aggregate bank migration remains required. Models scale with loaded cells; lazy loaded-cell assignments keep only overlays. Accessing original values/dimensions/iteration materializes that selected sheet and may reject unsupported style/date/shared-string content. Python output and source catalogs are additional costs.

New saves use bounded sequential XML spools plus an adjacent output ZIP; loaded saves need only the output ZIP. Path targets are replaced after successful ZIP completion, and failed writes clean owned temporary files. Loaded `close()` releases reader/editor file handles and future source access raises ValueError. New-model close is harmless. Long native loading/saving/structural operations release the GIL.

Still required: full style/date/shared/rich-string reading, date-only and sub-millisecond datetime compatibility, non-finite numbers, complete formula tokenizer, loaded append/structural/sheet mutation, read-only/write-only binding modes, file-like I/O, workbook views/properties and all advanced M5/M6 features. Unsupported arguments/properties fail explicitly. Macro input currently requires keep_vba=True; macro removal is staged. Saving a data-only loaded workbook is not implemented. These limits are not a reduced final feature scope.

Tests contain 37 selected original openpyxl 3.1.5 worksheet methods, nine translator methods and 17 workbook test bodies, with unchanged assertions and adapted imports/fixtures. All selected cases pass; complete tokenizer-dependent cases remain staged. Shared public-API tests run against both engines, and separate failure/preservation tests cover adapter ownership and limits. [Test provenance](../../third_party/python-tests.json), [license](../../third_party/licenses/openpyxl-MIT.txt), [ADR](../../docs/decisions/0006-python-compatibility-adapter.md), [direct API benchmark](../../benchmarks/python-adapter.md).

Verify original test bodies with `python tools/verify_python_test_provenance.py --reference-checkout /path/to/pinned/openpyxl` from the repository root. The full upstream suite is not claimed to run unchanged.

`move_range(..., translate=True)` and `openrsxl.formula.translate.Translator` now use bounded Rust A1 translation. Absolute axes, quoted sheet names/text and structured references retain context. Formula tools use baseline reference grammar separately from physical worksheet limits. `Translator.MAX_FORMULA_BYTES` configures its output allowance. Full tokenizer APIs and dynamic spill translation are not implemented.

Owned worksheet copies are independent scalar/formula models using the canonical Rust bank; unsupported feature graphs cannot be set and are not silently omitted. Removed worksheets remain usable through retained Python references. Public sheet insertion/movement and active-index behavior follow Python compatibility rules while stable Rust IDs remain internal. [Workbook test provenance](../../third_party/python-workbook-tests.json) records the additional original assertions.
