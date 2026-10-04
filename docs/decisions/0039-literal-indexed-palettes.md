# ADR 0039: Literal indexed palette colors

Indexed palette entries use the existing canonical ArgbLiteral, shared with font/fill/border colors and rich text. Numeric channels alone cannot retain the public reference's case-sensitive RGB property. Parsing accepts six or eight hexadecimal digits, prefixes six-digit RGB with zero alpha and retains letter case. Numeric callers use ArgbLiteral::from_channels() or From<u32> for uppercase spelling. Serialization uses the same Display implementation; no second case mask, heap string or palette color engine is introduced.

Source catalog adoption transfers the existing vector without copying. Reader and registry budgets charge its actual capacity using size_of::<ArgbLiteral>(), including the case mask and padding. Each slot increases from four to eight bytes on the tested platform; a 64-entry palette costs an additional 256 managed bytes. This is a correctness tradeoff, not a memory optimization. Invalid/missing RGB and non-RGB palette records continue returning typed errors; broader color descriptor and source-extension behavior remains staged.

Tests cover mixed-case/equal-channel identities, six-digit alpha normalization, writer readback, consuming vector ownership, budget rejection and parser failure cleanup. The public probe uses RgbColor and ColorList objects and saved XML, without inspecting reference implementation. Every palette spelling and the worksheet scalar survive source-to-new-package transfer. Performance evidence is in benchmarks/m2-palette-literals.md.

This checkpoint does not complete M2 styles, named-style mutation, loaded package graphs or full Python style APIs.
