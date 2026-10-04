// SPDX-License-Identifier: MIT
// Selected style component layout adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. Unified typed models replace upstream wrappers.
use crate::{Color, Error, ErrorKind, Result};
macro_rules! token_enum {
    ($(#[$meta:meta])* $name:ident {$( $(#[$vmeta:meta])* $variant:ident => $token:literal ),+ $(,)?}) => {
        $(#[$meta])* #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {$( $(#[$vmeta])* $variant ),+}
        impl $name {
            /// Format token for this variant.
            pub const fn as_str(self) -> &'static str {match self {$(Self::$variant=>$token),+}}
            /// Parse a supported format token without a fallback default.
            pub fn parse(value: &str) -> Result<Self> {match value { $($token=>Ok(Self::$variant)),+, _=>Err(Error::new(ErrorKind::InvalidData,concat!("Invalid ",stringify!($name)," token")))}}
        }
    }
}
token_enum! {/// Cell border line style.
    BorderLine {
        /// Explicit no line.
        None=>"none", /// Thin line.
        Thin=>"thin", /// Medium line.
        Medium=>"medium", /// Thick line.
        Thick=>"thick", /// Dashed line.
        Dashed=>"dashed", /// Dotted line.
        Dotted=>"dotted", /// Double line.
        Double=>"double", /// Hair line.
        Hair=>"hair", /// Dash-dot line.
        DashDot=>"dashDot", /// Dash-dot-dot line.
        DashDotDot=>"dashDotDot", /// Medium dashed line.
        MediumDashed=>"mediumDashed", /// Medium dash-dot line.
        MediumDashDot=>"mediumDashDot", /// Medium dash-dot-dot line.
        MediumDashDotDot=>"mediumDashDotDot", /// Slanted dash-dot line.
        SlantDashDot=>"slantDashDot"
    }
}
/// One optional border edge; absence and an empty edge remain distinct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Hash)]
pub struct BorderSide {
    /// Optional line override.
    pub line: Option<BorderLine>,
    /// Optional color identity/tint.
    pub color: Option<Color>,
}
/// Complete border components, including internal and directional edges.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Border {
    /// Left, right, top, bottom, diagonal, vertical, horizontal, start and end.
    pub sides: [Option<BorderSide>; 9],
    /// Draw the ascending diagonal.
    pub diagonal_up: Option<bool>,
    /// Draw the descending diagonal.
    pub diagonal_down: Option<bool>,
    /// Optional outline setting.
    pub outline: Option<bool>,
}
impl Default for Border {
    fn default() -> Self {
        Self {
            sides: [
                Some(BorderSide {
                    line: None,
                    color: None,
                }),
                Some(BorderSide {
                    line: None,
                    color: None,
                }),
                Some(BorderSide {
                    line: None,
                    color: None,
                }),
                Some(BorderSide {
                    line: None,
                    color: None,
                }),
                Some(BorderSide {
                    line: None,
                    color: None,
                }),
                None,
                None,
                None,
                None,
            ],
            diagonal_up: None,
            diagonal_down: None,
            outline: None,
        }
    }
}
token_enum! {/// Horizontal cell alignment.
    HorizontalAlignment {
        /// General alignment.
        General=>"general", /// Left aligned.
        Left=>"left", /// Center aligned.
        Center=>"center", /// Right aligned.
        Right=>"right", /// Fill available width.
        Fill=>"fill", /// Justify text.
        Justify=>"justify", /// Center across selection.
        CenterContinuous=>"centerContinuous", /// Distributed text.
        Distributed=>"distributed"
    }
}
token_enum! {/// Vertical cell alignment.
    VerticalAlignment {
        /// Bottom aligned.
        Bottom=>"bottom", /// Center aligned.
        Center=>"center", /// Top aligned.
        Top=>"top", /// Justify text.
        Justify=>"justify", /// Distributed text.
        Distributed=>"distributed"
    }
}
/// Cell alignment overrides with absent values retained.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Alignment {
    /// Horizontal positioning.
    pub horizontal: Option<HorizontalAlignment>,
    /// Vertical positioning.
    pub vertical: Option<VerticalAlignment>,
    /// Text rotation, 0..=180 or 255 for stacked text.
    pub rotation: Option<u8>,
    /// Wrap text, independent of shrink-to-fit.
    pub wrap_text: Option<bool>,
    /// Shrink to fit, independent of wrapping.
    pub shrink_to_fit: Option<bool>,
    /// Leading indentation, up to 255.
    pub indent: Option<f64>,
    /// Relative indentation in the range -255..=255.
    pub relative_indent: Option<f64>,
    /// Justify last line.
    pub justify_last_line: Option<bool>,
    /// Reading-order code.
    pub reading_order: Option<f64>,
    /// Optional merge-cell alignment metadata.
    pub merge_cell: Option<bool>,
}
/// Cell protection overrides, separate from sheet protection activation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Protection {
    /// Lock the cell when sheet protection is enabled.
    pub locked: Option<bool>,
    /// Hide the cell formula when sheet protection is enabled.
    pub hidden: Option<bool>,
}
token_enum! {/// Pattern fill style.
    FillPattern {
        /// No pattern.
        None=>"none", /// Solid foreground.
        Solid=>"solid", /// Medium gray.
        MediumGray=>"mediumGray", /// Dark gray.
        DarkGray=>"darkGray", /// Light gray.
        LightGray=>"lightGray", /// Dark horizontal.
        DarkHorizontal=>"darkHorizontal", /// Dark vertical.
        DarkVertical=>"darkVertical", /// Dark downward diagonal.
        DarkDown=>"darkDown", /// Dark upward diagonal.
        DarkUp=>"darkUp", /// Dark grid.
        DarkGrid=>"darkGrid", /// Dark trellis.
        DarkTrellis=>"darkTrellis", /// Light horizontal.
        LightHorizontal=>"lightHorizontal", /// Light vertical.
        LightVertical=>"lightVertical", /// Light downward diagonal.
        LightDown=>"lightDown", /// Light upward diagonal.
        LightUp=>"lightUp", /// Light grid.
        LightGrid=>"lightGrid", /// Light trellis.
        LightTrellis=>"lightTrellis", /// One-eighth gray.
        Gray125=>"gray125", /// One-sixteenth gray.
        Gray0625=>"gray0625"
    }
}
/// Optional fill pattern and colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Hash)]
pub struct PatternFill {
    /// Pattern token, distinct from missing.
    pub pattern: Option<FillPattern>,
    /// Foreground reference/tint.
    pub foreground: Option<Color>,
    /// Background reference/tint.
    pub background: Option<Color>,
}
token_enum! {/// Gradient geometry.
    GradientKind {
        /// Linear gradient.
        Linear=>"linear", /// Path gradient.
        Path=>"path"
    }
}
/// Color at a gradient position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Fraction in 0..=1.
    pub position: f64,
    /// Color reference/tint.
    pub color: Color,
}
/// Full gradient fill rather than only upstream's degree/stops subset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GradientFill {
    /// Geometry token.
    pub kind: Option<GradientKind>,
    /// Rotation in degrees.
    pub degree: Option<f64>,
    /// Path edge fractions, left/right/top/bottom.
    pub edges: [Option<f64>; 4],
    /// Ordered stops; payload/capacity counts towards catalog budgets.
    pub stops: Vec<GradientStop>,
}
/// Shared fill model used by both imported catalogs and new styles.
#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Fill {
    /// Pattern/color fill.
    Pattern(PatternFill),
    /// Linear/path gradient.
    Gradient(GradientFill),
}
impl Default for Fill {
    fn default() -> Self {
        Self::Pattern(PatternFill {
            pattern: Some(FillPattern::None),
            ..Default::default()
        })
    }
}
impl Fill {
    /// Construct a solid foreground fill without installing a background override.
    pub fn solid(color: Color) -> Self {
        Self::Pattern(PatternFill {
            pattern: Some(FillPattern::Solid),
            foreground: Some(color),
            background: None,
        })
    }

    /// Retained heap payload including gradient vector capacity.
    pub fn heap_bytes(&self) -> usize {
        match self {
            Self::Pattern(_) => 0,
            Self::Gradient(v) => v.stops.capacity() * size_of::<GradientStop>(),
        }
    }
}

token_enum! {/// Table/pivot formatting region from the pinned public schema.
    TableStyleRegion {
        /// blankRow region.
        BlankRow=>"blankRow",
        /// firstColumn region.
        FirstColumn=>"firstColumn",
        /// firstColumnStripe region.
        FirstColumnStripe=>"firstColumnStripe",
        /// firstColumnSubheading region.
        FirstColumnSubheading=>"firstColumnSubheading",
        /// firstHeaderCell region.
        FirstHeaderCell=>"firstHeaderCell",
        /// firstRowStripe region.
        FirstRowStripe=>"firstRowStripe",
        /// firstRowSubheading region.
        FirstRowSubheading=>"firstRowSubheading",
        /// firstSubtotalColumn region.
        FirstSubtotalColumn=>"firstSubtotalColumn",
        /// firstSubtotalRow region.
        FirstSubtotalRow=>"firstSubtotalRow",
        /// firstTotalCell region.
        FirstTotalCell=>"firstTotalCell",
        /// headerRow region.
        HeaderRow=>"headerRow",
        /// lastColumn region.
        LastColumn=>"lastColumn",
        /// lastHeaderCell region.
        LastHeaderCell=>"lastHeaderCell",
        /// lastTotalCell region.
        LastTotalCell=>"lastTotalCell",
        /// pageFieldLabels region.
        PageFieldLabels=>"pageFieldLabels",
        /// pageFieldValues region.
        PageFieldValues=>"pageFieldValues",
        /// secondColumnStripe region.
        SecondColumnStripe=>"secondColumnStripe",
        /// secondColumnSubheading region.
        SecondColumnSubheading=>"secondColumnSubheading",
        /// secondRowStripe region.
        SecondRowStripe=>"secondRowStripe",
        /// secondRowSubheading region.
        SecondRowSubheading=>"secondRowSubheading",
        /// secondSubtotalColumn region.
        SecondSubtotalColumn=>"secondSubtotalColumn",
        /// secondSubtotalRow region.
        SecondSubtotalRow=>"secondSubtotalRow",
        /// thirdColumnSubheading region.
        ThirdColumnSubheading=>"thirdColumnSubheading",
        /// thirdRowSubheading region.
        ThirdRowSubheading=>"thirdRowSubheading",
        /// thirdSubtotalColumn region.
        ThirdSubtotalColumn=>"thirdSubtotalColumn",
        /// thirdSubtotalRow region.
        ThirdSubtotalRow=>"thirdSubtotalRow",
        /// totalRow region.
        TotalRow=>"totalRow",
        /// wholeTable region.
        WholeTable=>"wholeTable",
    }
}
