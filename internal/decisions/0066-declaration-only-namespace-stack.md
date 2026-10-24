# ADR 0066: Namespace stack updates only at declarations

## Context

The shared XML route still advances and unwinds the namespace resolver for
every element even when no namespace binding changes. Most worksheet cell/value
elements carry coordinates and types but no declarations. The actual element
nesting and the binding-scope nesting need not be the same counter.

## Decision

Maintain a checked u16 element counter for all start/empty elements, preserving
the resolver's previous nesting boundary. Store this element level in each
declaration snapshot. Push the authoritative quick-xml NamespaceResolver only
for declaration-bearing elements, and pop it only when that snapshot's element
level ends. The resolver's private level now counts active declaration scopes.

Keep delayed popping: empty/end events resolve their names before the next event
unwinds the scope. Cache default semantic scope, resolve prefixed names through
quick-xml, and retain reserved-prefix and undeclared-prefix errors. The shared
Reader still checks end tags and the separate XmlStream depth/byte/root limits.
An ordinary ancestor cannot remove a descendant binding before its owning
declaration element ends. No new namespace grammar or worksheet buffer is added.

## Verification

Retain the existing strict/prefixed workflow covering declaration restoration,
sibling/empty/end scopes, prefix shadowing and illegal bindings. Run workspace
tests, strict Clippy and the streaming suite on Rust 1.88. Compare prior/current
serial release numeric stream/model and SST/style probes, including retained
calamine model and calamine stream references. Keep builds/tests/generation out
of timed operations and record RSS, temporary bytes and verified values.

## Boundaries

This original optimization extends the previously attributed quick-xml event
ownership adaptation. It does not weaken the shared resource/default limits,
change model ownership or establish that CrabXL is faster than calamine.
