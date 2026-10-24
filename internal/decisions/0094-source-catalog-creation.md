# Source catalog creation

Loaded workbooks without a stylesheet can initialize the canonical style bank
when a style operation first needs it. Initialization respects the configured
style and retained-memory allowances and assigns the bank only after success.

Saving a changed style bank or theme creates the missing package part and adds
its workbook relationship and content-type declaration. Part names and
relationship identifiers avoid existing archive and graph identities. Existing
opaque parts and unchanged worksheets remain preserved. The save planner charges
the identifiers alongside hyperlink plans and checks the combined archive entry
count before opening the output archive.

The format layer uses the existing stylesheet and theme codecs; it does not
retain a second catalog. Signed packages and missing workbook relationship roots
remain explicit unsupported dependencies. Structural merge preparation adopts
source hyperlink metadata before applying the canonical affected-link guard.

This is an A11 implementation checkpoint, not release acceptance. Compilation
passes on Rust 1.88; additional behavioral tests, interoperability and performance
acceptance are deferred until the complete native and Python A11 scope is ready.
