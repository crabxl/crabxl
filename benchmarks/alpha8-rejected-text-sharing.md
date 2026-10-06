# Rejected A8 whole-text-handle sharing

The prototype replaced `CellValue::Text(Box<CellText>)` with a compact CellText
whose immutable storage used Arc ownership. SST entries reused that same handle,
removing per-cell wrapper allocations without changing the 16-byte CellValue.
Clone-on-consuming text preserved independent mutable owned output; workspace,
Rust 1.88 and both compression-backend tests passed.

Against core `b5990868623e31b8cc3e47f4ba3bf0f906e54865`, one million repeated
shared-text cells decreased median RSS from 68,828 to 37,120 KiB and elapsed
time from 0.372846 to 0.334681 seconds. Unique RAM SST models improved too.
However, ordinary owned Unicode text creation increased RSS from 209,156 to
225,264 KiB and elapsed time from 1.314267 to 1.379920 seconds. Styled and unique
disk cases also regressed. The representation changed the public Rust enum
payload, while the supported constructors and Python value behavior stayed the
same. The owned-text regression makes this unsuitable as a general optimization.

The runtime and test-constructor changes are reverted. The current canonical
Text variant remains boxed, and the accepted buffered SST changes remain.
[Model samples](results/alpha8-text-sharing-models.json) and
[owned creation samples](results/alpha8-text-sharing-owned-writes.json) retain
the exact rejected patch, binaries, checks and resource/temporary observations.
Do not cite the prototype's improvements as released behavior.

The text-creation benchmark now accepts an explicit baseline revision instead
of assuming one fixed commit. Future model allocation/accounting changes remain
separate measured work; this record does not complete their acceptance.
