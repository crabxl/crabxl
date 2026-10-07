# ADR 0092: independent hyperlink declaration references

Status: implemented native point foundation; Python alias integration follows.

A hyperlink's canonical owner coordinate and serialized declaration reference can
differ. Optional owned reference text participates in point byte accounting and
shared validation. Default records retain the allocation-free owner-coordinate
spelling. Finite point references are checked before value filling or metadata
commit; range references remain explicitly unsupported pending compact geometry.

Owned and preserving writers share reference emission. Captured duplicate point
declarations replace prior metadata under the same bounded reservation, matching
public openpyxl last-declaration behavior. This accommodates files written from
shared public objects without dense cell expansion or another decoder.

Merge/move/copy/shift guards inspect affected declaration coordinates as well as
owner cells. A cached count bypasses declaration scans when no independent refs
exist, preserving ordinary sparse intersection lookup. Structural transformations
of affected hyperlink metadata remain explicit staged dependencies.

The existing native lifecycle scenario additionally writes two owner records with
one declaration coordinate, verifies last-target readback, and checks invalid range
rejection retains values and affected row shifts fail. Rust 1.88 workspace tests
and Rust 1.99 strict Clippy verify the foundation; the cached-count refinement is
also covered by its targeted lifecycle rerun. This does not release A11.
