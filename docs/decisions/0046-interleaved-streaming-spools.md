# ADR 0046: Independent append-only worksheet spools

WorkbookWriter retains the sequential API and adds stable creation IDs for
independently appendable worksheets. Selecting a sheet swaps owned spool state;
each sheet retains its last row, footer and bounded buffer. All sheets share the
canonical style/date/formula encoder and one registration bank. Finish closes
remaining spools and packages creation order, regardless of append/close order.

Paused vector capacity, names, paths, footers and buffers count against metadata
allowances. Row writes reserve paused footer bytes within aggregate temporary
limits. Abort and Drop clean active, paused and completed files. Invalid names,
duplicate names and sheet-count failures preserve the previously active sheet.
No spreadsheet cells or whole worksheet XML are kept resident.

Worksheet dimension inspection reads bounded header events and returns the
declared range or None. It is metadata, not validation of actual cell extent or
unread CRC. Consumers can stream to calculate actual dimensions independently.

These core capabilities enable Python optimized modes without duplicating XLSX
codecs. Complete Python styles and advanced workbook features remain separate
staged capabilities. No additional upstream implementation was copied.
