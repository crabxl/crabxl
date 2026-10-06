# ADR 0081: Reuse the first cell block during descending insertion

Status: implemented; A8 integration and release acceptance remain open.

Inserting a new minimum coordinate previously created a one-cell vector and tree
entry each time. Descending population therefore defeated canonical packed cell
storage despite using the same editable Worksheet model.

Reuse a partially filled first block, insert at its beginning and rekey it to the
new minimum. Shifts remain bounded by 128 cells. Full blocks retain the existing
new-block path; ordering, uniqueness, maximum-coordinate caching, references,
styles and resource checks keep their contracts. No second model, payload index,
public API change or unsafe code is introduced.

The existing independently checked sparse-cell workflow now compares 653
descending cells, multiple blocks and an incomplete final row with ascending
population before running its interior insertion/removal cases. Rust 1.88 and
loaded-model tests and strict Clippy pass without new test functions.

[Paired measurements](../../benchmarks/alpha8-descending-insertion.md) retain
the exact patch and comparison worker. One million descending numeric cells
reduce median peak RSS from 116,828 to 37,076 KiB; complete creation/save time
changes from 1.155196 to 1.146507 seconds. Smaller descending creation is slower,
and ordinary read/write timing is mixed. Accept the large allocation reduction,
not a claim of general speed superiority. Capacity-aware model accounting and
arbitrary insertion distributions remain separate work.
