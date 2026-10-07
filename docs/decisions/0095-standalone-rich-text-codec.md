# Standalone rich-text codec

The public facade exposes `read_rich_text` and `write_rich_text` for XML-tree
adapters and callers that already hold a rich-value fragment. Both operations use
the same format codec as worksheets and shared strings; no separate run, font or
phonetic parser is introduced.

Reading accepts a namespaced inline or shared-string root, consumes the complete
input and rejects extra roots or trailing malformed XML. XML event/depth, part
size and decoded cell limits use explicit `ResourceLimits`. Writing validates the
canonical value first and bounds the emitted part while streaming to the supplied
writer. The existing worksheet writer keeps its inherited namespace behavior;
standalone output declares the spreadsheet namespace.

Rust 1.88 library compilation and formatting pass. Python tree conversion and
complete A11 pre-release acceptance remain subsequent work; this checkpoint does
not claim additional interoperability or performance results.
