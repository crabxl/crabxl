# ADR 0054: Exact style integer identities

Public openpyxl 3.1.5 constructors and readers accept arbitrary decimal charset,
theme and indexed identities. The former i64-only records could not retain
these inputs. `StyleInteger` now shares the existing canonical `ExactInteger`
representation, with an allocation-free i64 fast path and rare boxed large
values. Parsing normalizes signs/leading zeros, including long spellings of
small values, without routing through floating point. Private representation
keeps equality and hashing canonical; callers cannot construct mismatched small
and large representations of the same value.

`RunFont.charset` and `ColorKind::Theme`/`Indexed` use this common type. Color,
border-edge and gradient-stop records own their identities and are Clone rather
than Copy. Serializers and validation borrow these records. On the measured
64-bit target StyleInteger is 16 bytes, Color 40 bytes and Font 120 bytes.
Ordinary cells remain unchanged; small identities allocate no decimal payload.
Existing Rust source using integer constructors adds `.into()`.

Reader catalogs, canonical registry registration/adoption, differential styles,
rich runs, recent colors, fills and borders count exact integer wrappers and
payload. Shared codecs enforce component limits; gradient decoding keeps a
linear cumulative payload ledger and reserves validation scratch alongside
actual vector capacity. Failed registration does not mutate catalog records.

Existing domain tests now cover boundaries, sign normalization, invalid syntax,
deduplication, ownership transfer, exact native read/write and large-payload
budget rejection/retry. Original pinned upstream tests are unchanged. The
[public probe](../../benchmarks/results/alpha5-style-integer-interop.json) checks
native charset/font/fill/border values through openpyxl. Its self-save observation
is deliberately separate: the reference emits scientific notation for large
integers, then cannot reload its own output. CrabXL emits exact decimal text;
it does not reproduce that defect.

[Measured regression evidence](../../benchmarks/alpha5-style-integers.md) retains
verified outputs, wall/CPU/RSS, temporary storage and managed catalog cost.
Linux Rust 1.88 and latest-stable workspace tests and strict Clippy pass. This
finishes exact style identity retention; it does not implement theme rendering,
complete style mutation or Python style proxies.
