//! Conditional formatting (mirrors openpyxl `formatting/rule.py` and `formatting/formatting.py`).
use crate::{CellRange, Color};
use std::mem::size_of;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                #[doc = concat!("The `", $text, "` value.")]
                $variant,
            )+
        }
        impl $name {
            /// Canonical OOXML spelling.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text,)+ }
            }
            /// Parse the canonical OOXML spelling.
            pub fn parse(value: &str) -> Option<Self> {
                match value { $($text => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

string_enum! {
    /// Conditional formatting rule type (`cfRule@type`).
    ConditionalRuleType {
        Expression => "expression",
        CellIs => "cellIs",
        ColorScale => "colorScale",
        DataBar => "dataBar",
        IconSet => "iconSet",
        Top10 => "top10",
        UniqueValues => "uniqueValues",
        DuplicateValues => "duplicateValues",
        ContainsText => "containsText",
        NotContainsText => "notContainsText",
        BeginsWith => "beginsWith",
        EndsWith => "endsWith",
        ContainsBlanks => "containsBlanks",
        NotContainsBlanks => "notContainsBlanks",
        ContainsErrors => "containsErrors",
        NotContainsErrors => "notContainsErrors",
        TimePeriod => "timePeriod",
        AboveAverage => "aboveAverage",
    }
}
string_enum! {
    /// Comparison operator of a rule (`cfRule@operator`).
    ConditionalOperator {
        LessThan => "lessThan",
        LessThanOrEqual => "lessThanOrEqual",
        Equal => "equal",
        NotEqual => "notEqual",
        GreaterThanOrEqual => "greaterThanOrEqual",
        GreaterThan => "greaterThan",
        Between => "between",
        NotBetween => "notBetween",
        ContainsText => "containsText",
        NotContains => "notContains",
        BeginsWith => "beginsWith",
        EndsWith => "endsWith",
    }
}
string_enum! {
    /// Time period of a `timePeriod` rule.
    ConditionalTimePeriod {
        Today => "today",
        Yesterday => "yesterday",
        Tomorrow => "tomorrow",
        Last7Days => "last7Days",
        ThisMonth => "thisMonth",
        LastMonth => "lastMonth",
        NextMonth => "nextMonth",
        ThisWeek => "thisWeek",
        LastWeek => "lastWeek",
        NextWeek => "nextWeek",
    }
}
string_enum! {
    /// Value-object type (`cfvo@type`).
    CfValueType {
        Num => "num",
        Percent => "percent",
        Max => "max",
        Min => "min",
        Formula => "formula",
        Percentile => "percentile",
    }
}
string_enum! {
    /// Icon set name.
    IconSetType {
        Arrows3 => "3Arrows",
        ArrowsGray3 => "3ArrowsGray",
        Flags3 => "3Flags",
        TrafficLights3 => "3TrafficLights1",
        TrafficLights3Rimmed => "3TrafficLights2",
        Signs3 => "3Signs",
        Symbols3 => "3Symbols",
        Symbols3Circled => "3Symbols2",
        Arrows4 => "4Arrows",
        ArrowsGray4 => "4ArrowsGray",
        RedToBlack4 => "4RedToBlack",
        Rating4 => "4Rating",
        TrafficLights4 => "4TrafficLights",
        Arrows5 => "5Arrows",
        ArrowsGray5 => "5ArrowsGray",
        Rating5 => "5Rating",
        Quarters5 => "5Quarters",
    }
}

/// Conditional format value object (`cfvo`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CfValueObject {
    /// Threshold type.
    pub value_type: Option<CfValueType>,
    /// Threshold value or formula text.
    pub value: Option<Box<str>>,
    /// Greater-than-or-equal flag.
    pub gte: Option<bool>,
}
impl CfValueObject {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.value.as_ref().map_or(0, |v| v.len())
    }
}

/// Color scale (`colorScale`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorScale {
    /// Value objects, two or three.
    pub cfvos: Vec<CfValueObject>,
    /// Colors matching the value objects.
    pub colors: Vec<Color>,
}
/// Data bar (`dataBar`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataBar {
    /// Minimum bar length in percent of the cell width.
    pub min_length: Option<u32>,
    /// Maximum bar length in percent of the cell width.
    pub max_length: Option<u32>,
    /// Whether the cell value is shown.
    pub show_value: Option<bool>,
    /// Minimum and maximum value objects.
    pub cfvos: Vec<CfValueObject>,
    /// Bar color.
    pub color: Option<Color>,
}
/// Icon set (`iconSet`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IconSet {
    /// Icon set name.
    pub icon_set: Option<IconSetType>,
    /// Whether the cell value is shown.
    pub show_value: Option<bool>,
    /// Whether thresholds are percentages.
    pub percent: Option<bool>,
    /// Whether icon order is reversed.
    pub reverse: Option<bool>,
    /// Threshold value objects.
    pub cfvos: Vec<CfValueObject>,
}

/// One conditional formatting rule (`cfRule`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConditionalRule {
    /// Rule type.
    pub rule_type: Option<ConditionalRuleType>,
    /// Index into the workbook differential style catalog; must be valid there.
    pub dxf_id: Option<u32>,
    /// Evaluation priority.
    pub priority: Option<u32>,
    /// Stop evaluating lower-priority rules when this one matches.
    pub stop_if_true: Option<bool>,
    /// Above (true) or below average.
    pub above_average: Option<bool>,
    /// Top/bottom rank is a percentage.
    pub percent: Option<bool>,
    /// Bottom instead of top ranking.
    pub bottom: Option<bool>,
    /// Comparison operator.
    pub operator: Option<ConditionalOperator>,
    /// Text for text rules.
    pub text: Option<Box<str>>,
    /// Period for `timePeriod` rules.
    pub time_period: Option<ConditionalTimePeriod>,
    /// Rank for top/bottom rules.
    pub rank: Option<u32>,
    /// Standard deviations for average rules.
    pub std_dev: Option<i32>,
    /// Include values equal to the average.
    pub equal_average: Option<bool>,
    /// Formula texts (without leading `=`).
    pub formulas: Vec<Box<str>>,
    /// Color scale payload.
    pub color_scale: Option<ColorScale>,
    /// Data bar payload.
    pub data_bar: Option<DataBar>,
    /// Icon set payload.
    pub icon_set: Option<IconSet>,
    /// Raw `<extLst>` of the rule (for example an Excel 2010 data bar link),
    /// kept unchanged so that saves do not drop it.
    pub ext_list: Option<Box<str>>,
}
fn cfvos_bytes(v: &[CfValueObject]) -> usize {
    std::mem::size_of_val(v) + v.iter().map(CfValueObject::heap_bytes).sum::<usize>()
}
impl ConditionalRule {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.text.as_ref().map_or(0, |v| v.len())
            + self.formulas.capacity() * size_of::<Box<str>>()
            + self.formulas.iter().map(|f| f.len()).sum::<usize>()
            + self.color_scale.as_ref().map_or(0, |c| {
                cfvos_bytes(&c.cfvos) + c.colors.capacity() * size_of::<Color>()
            })
            + self.data_bar.as_ref().map_or(0, |d| cfvos_bytes(&d.cfvos))
            + self.icon_set.as_ref().map_or(0, |d| cfvos_bytes(&d.cfvos))
            + self.ext_list.as_ref().map_or(0, |raw| raw.len())
    }
}

/// One `<conditionalFormatting>` block: ranges plus rules.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConditionalFormatting {
    /// Cell ranges the rules apply to (`sqref`).
    pub ranges: Vec<CellRange>,
    /// Pivot table flag.
    pub pivot: Option<bool>,
    /// Rules in document order.
    pub rules: Vec<ConditionalRule>,
}
impl ConditionalFormatting {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.ranges.capacity() * size_of::<CellRange>()
            + self.rules.capacity() * size_of::<ConditionalRule>()
            + self
                .rules
                .iter()
                .map(ConditionalRule::heap_bytes)
                .sum::<usize>()
    }
}
