# Borrowed hyperlink output identities

The XLSX facade exposes a borrowed `HyperlinkOutput` projection and an owned
package identity visitor. Loaded workbooks visit adopted declarations using the
same bounded relationship planner as source saves. Untouched worksheets are not
materialized, and planned identifiers do not overwrite canonical source hints.

Visitors follow workbook/declaration output order and include actual compact
coverage independently of a point's serialized reference. Consumers can retain
only identities for existing caller-held objects rather than clone every URL or
build a persistent per-cell table. Loaded visitors accept the consumer's managed
workspace charge so output selection shares the source/model allowance.

Point and declaration-anchor coverage lookup avoids scanning the range index.
No broad performance improvement is claimed without pre-release measurements.

Rust 1.88 library compilation and formatting pass. Python must apply planned
identities to its public views only after a successful save; that integration and
full A11 acceptance remain pending at this native checkpoint.
