# Shared rectangle operations

Finite cell ranges expose bounding union, intersection, containment, independent
edge adjustment and rectangle difference through the canonical core geometry.
All operations avoid cell enumeration. Adjustments use checked signed arithmetic
and validate the complete result before returning it; failure cannot partially
move a caller's range.

The four-piece difference operation also supports compact hyperlink coverage.
Public binding range utilities should delegate geometric decisions to these
methods and retain only public coordinate/title views. Live merged-declaration
mutation remains a separate owner-aware operation with appearance and source
graph dependencies; standalone geometry does not imply that integration is done.

This is an A11 implementation checkpoint. Rust 1.88 library compilation and
formatting pass; additional range utility acceptance is deferred to pre-release.
