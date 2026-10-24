# ADR 0062: Cache semantic default namespace scope

## Context

The four-engine numeric baseline leaves a substantial streaming read gap to
calamine. Separate CPU sampling identified the shared XML event route as the
largest area to investigate. Ordinary cell/value elements repeat default
namespace resolution and attribute inspection even when no declaration changes.

## Decision

Use quick-xml's Reader and NamespaceResolver together. Adapt the delayed-pop
ownership flow from the pinned 0.42.0 NsReader rather than implementing a second
namespace grammar. Cache the semantic default namespace classification while
keeping prefixed elements and relationship attributes on the authoritative
resolver. Record the original symbols, revision and MIT notice in third_party.

An element without a namespace declaration advances resolver depth using a
borrowed declaration-free tag, avoiding attribute decoding for namespace work.
An element with a declaration pushes its actual start event and saves the
previous default classification. Empty and end events retain their scope until
the following event, when the resolver and cache unwind together.

The raw attribute scan is only a necessary-condition filter: every namespace
binding key contains lowercase `x`. A positive result still parses attributes
and checks actual binding keys. XML/value codecs continue validating their own
attributes. Reserved namespace bindings, unknown prefixes, qualified attributes,
end tags and all existing input/event/depth limits retain their existing checks.

The cache stores ancestor declaration snapshots, not XML or decoded cells.
Allocation is fallible. No public model/API, resource defaults, or supported
format family changes as a consequence of this optimization.

## Verification

Extend the existing strict/prefixed namespace integration workflow with default
namespace changes/restoration, sibling scopes, prefix shadowing, undeclared
prefixes, foreign elements and reserved-prefix failures. Run workspace tests and
strict lint/MSRV checks. Compare preserved prior/current release binaries on
numeric streams/models/edit-save and the existing value-verifying SST and
styled/date/formula-cache probes; keep generation, builds and profiling outside
timing. Record RSS, output verification and sampled temporary storage alongside
wall/CPU measurements. Timing results do not replace the compatibility checks.

## Consequences

One shared XML route serves worksheets and other package parts, so improvements
benefit their codecs without separate implementations. Prefix-heavy documents
still need individual resolution and may gain less. This checkpoint does not
close the full calamine gap or the remaining editable-model/M6 work.
