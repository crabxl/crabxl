# ADR 0067: Compact failure context in successful Result paths

## Context

The canonical Error directly contains kind, message, part, cell and source
fields. Its size determines the size of many successful Result return values
in hot address/value/XML/model paths, although failure context is rarely needed.
Error simplification must retain matchable categories, source chains and location.

## Decision

Keep the public Error and Result API. Place private detail fields behind one
owned Box and allocate them only when constructing an error. Mark constructors
cold. Successful parsing does not allocate an error, and its Result variants
no longer reserve the full context inline. Do not box ordinary values or rows.

Keep kind/part/cell getters, Display spelling, Debug field names/order and the
standard error source chain unchanged. Attaching context mutates the owned
details. Send/Sync follow the existing source bounds. No unsafe code or
dependency is added. The model and explicit resource checks are unchanged;
allocator/error overhead is outside the managed payload allowance as before.

This is a tradeoff: each actual failure now allocates an extra context box.
Do not present reduced successful stack/result size as lower universal RSS or
assume faster parsing without release measurements.

## Verification

Extend the existing malformed-input workflow to verify part/cell/source context
for a wrapped numeric parse error. Run workspace checks, strict Clippy and Rust
1.88 loaded/streaming tests, covering error and resource behavior. Measure
identical prior/current release workers for numeric streaming/retained models,
repeated/unique RAM/disk text models, and mixed styled/scalar streams. Verify
every value, output cleanup, timing, RSS and temporary storage; keep builds,
tests and profiling outside timing.

## Boundaries

This is original canonical error integration. It changes neither supported
format families nor preservation/editing capabilities and does not establish
calamine speed acceptance on its own. Python exposure requires a new verified
published core pin and separate end-to-end conversion measurements.
