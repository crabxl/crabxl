# Third-party provenance

Selected algorithms and parsing flows were adapted from the pinned calamine
source listed in [ports.json](ports.json). Original notices are retained in the
adapted source files, and the MIT license is distributed under licenses/.

calamine and rust_xlsxwriter are not runtime dependencies or wholesale vendored
libraries. Selected rust_xlsxwriter scalar/formula/style XML layouts, date conversion, sequential spooling and packaging flows are now adapted into the XLSX writer. Its MIT license is distributed under licenses/ and notices are retained in adapted source files. Full upstream model/style/feature imports remain deferred.

Selected openpyxl 3.1.5 worksheet test methods are adapted only for the optional Python adapter. Exact test bodies/parameters, revision, source hash, fixture changes, expected failure and MIT notice are recorded in [python-tests.json](python-tests.json); `tools/verify_python_test_provenance.py` verifies them from the pinned test source without reading implementation.

Additional original translator tests are traced in [python-formula-tests.json](python-formula-tests.json); verify with `--record third_party/python-formula-tests.json`. Tokenizer-dependent cases remain unported.

Additional original workbook tests are traced in [python-workbook-tests.json](python-workbook-tests.json), including the duplicate source function selection matching Python execution. Verify with `--record third_party/python-workbook-tests.json`. Full workbook/style/view/mode coverage remains staged.
