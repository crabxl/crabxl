# ADR 0052: Explicit owned formula annotation references

## Decision

`CellMetadataReadPolicy::RetainFormulaReferences` retains source cm/vm literals
in canonical `FormulaMetadata.annotations`, separately from formula text, kind,
range and optional typed cache. Reference strings preserve absence, empty and
opaque/non-numeric spelling without inventing metadata graph semantics. Ordinary
Compatible and Reject policies retain their existing behavior.

Annotation capture happens only after cell projection. The selected literals
and formula payload obey cell limits, and actual owned wrapper/payload bytes
flow through existing row, batch, materialization and joint Auto accounting.
Scalar Cell/CellValue sizes do not grow. FormulaMetadata adds one optional boxed
pointer (eight bytes on supported 64-bit hosts); annotated formulas additionally
own the reference wrapper and literal payloads. There is no worksheet-wide map.

The explicit retention policy rejects data-only projection and annotated scalar
cells, since those output models cannot expose formula references. Callers can
use Compatible visible-value projection instead. Typed metadata part graph
interpretation/create/edit remains M6 work. New-file and assigned-value output
reject annotated formula models with Unsupported before row emission rather than
dropping references or creating dangling indices. Original unrelated parts and
cells continue to use the existing preservation path.

## Evidence

Two tests verify streaming, owned materialization and repeated-access Auto
outputs, cache distinctions, literal references, owner destruction, projection,
limits and contextual errors. A rejected writer row leaves the writer reusable.
Rust 1.88 workspace tests and latest-stable Clippy pass.

[Native evidence](../../benchmarks/alpha5-formula-annotations.md) compares unchanged
ordinary projection with exact prior core ca40f67 and separately measures
explicit retention. This does not claim complete dynamic metadata graph support
or M2 closure.
