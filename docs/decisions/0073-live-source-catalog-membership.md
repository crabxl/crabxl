# ADR 0073: Live source catalog membership

Status: implemented for worksheet creation; copy and removal remain pending.

The source-backed coordinator registers new worksheets in its existing canonical
bank. It retains bounded original sheet declarations and stable source identities,
plus compact records for newly allocated package identities. Original cell bodies
remain lazy. Names, order and visibility come from the bank at save time.

Creation validates source policies and metadata allowances before registering a
model. Package sheet IDs, relationship IDs and part names avoid existing and
pending identities. A conservative charge covers sparse B-tree allocation; the
old declaration cache remains charged until the membership transaction commits.

Repeat saves emit synchronized workbook declarations, relationships and content
types. Newly created bodies use the shared borrowed cell encoder and imported
style catalog, with byte-bounded row/output buffers. Empty creation preserves
original cell XML and caches. New cell edits use existing calculation invalidation
policies. No original workbook, worksheet XML or complete cell model is cloned.

This is original CrabXL package coordination. The pinned umya-spreadsheet 3.1.0
reference (`aa6a80f66ff0f6ae629b2a3439d8d1e71bdbcd5b`), particularly workbook sheet
collection and worksheet ownership behavior, informed the design; no umya code
or types are imported by this checkpoint. Earlier structural provenance remains
in ADR 0072.

Affected local defined-name ownership, signed packages and unsupported source
policies still reject before mutation. New typed-date style registration and
unmodeled feature graphs remain explicit M5/M6 dependencies. This checkpoint
neither completes staged M4 nor releases Alpha 7.

The existing loaded-bank workflow verifies lazy creation, ordinary edits and
structural edits, name/order changes, repeated readback, multiple additions,
identity collisions, invalid-name atomicity and opaque-part preservation.
`loaded_rows bank-create` measures creation, save and complete readback separately
from builds and fixture generation.
