//! Data validation (mirrors openpyxl `worksheet/datavalidation.py`).
use crate::CellRange;
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
    /// Validation type.
    ValidationType {
        None => "none",
        Whole => "whole",
        Decimal => "decimal",
        List => "list",
        Date => "date",
        Time => "time",
        TextLength => "textLength",
        Custom => "custom",
    }
}
string_enum! {
    /// Validation comparison operator.
    ValidationOperator {
        Between => "between",
        NotBetween => "notBetween",
        Equal => "equal",
        NotEqual => "notEqual",
        LessThan => "lessThan",
        LessThanOrEqual => "lessThanOrEqual",
        GreaterThan => "greaterThan",
        GreaterThanOrEqual => "greaterThanOrEqual",
    }
}
string_enum! {
    /// Error alert style.
    ValidationErrorStyle {
        Stop => "stop",
        Warning => "warning",
        Information => "information",
    }
}
string_enum! {
    /// Input method editor mode.
    ImeMode {
        NoControl => "noControl",
        Off => "off",
        On => "on",
        Disabled => "disabled",
        Hiragana => "hiragana",
        FullKatakana => "fullKatakana",
        HalfKatakana => "halfKatakana",
        FullAlpha => "fullAlpha",
        HalfAlpha => "halfAlpha",
        FullHangul => "fullHangul",
        HalfHangul => "halfHangul",
    }
}

/// One data validation rule (`dataValidation`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataValidation {
    /// Cell ranges the rule applies to (`sqref`).
    pub ranges: Vec<CellRange>,
    /// Validation type.
    pub validation_type: Option<ValidationType>,
    /// Comparison operator.
    pub operator: Option<ValidationOperator>,
    /// Error alert style.
    pub error_style: Option<ValidationErrorStyle>,
    /// IME mode.
    pub ime_mode: Option<ImeMode>,
    /// Blank cells are valid.
    pub allow_blank: Option<bool>,
    /// Hide the in-cell dropdown for lists (attribute semantics are inverted by the spec).
    pub show_drop_down: Option<bool>,
    /// Show the input prompt.
    pub show_input_message: Option<bool>,
    /// Show the error alert.
    pub show_error_message: Option<bool>,
    /// Error alert title.
    pub error_title: Option<Box<str>>,
    /// Error alert message.
    pub error: Option<Box<str>>,
    /// Input prompt title.
    pub prompt_title: Option<Box<str>>,
    /// Input prompt message.
    pub prompt: Option<Box<str>>,
    /// First formula or list text.
    pub formula1: Option<Box<str>>,
    /// Second formula.
    pub formula2: Option<Box<str>>,
}
impl DataValidation {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        let t = |v: &Option<Box<str>>| v.as_ref().map_or(0, |s| s.len());
        self.ranges.capacity() * size_of::<CellRange>()
            + t(&self.error_title)
            + t(&self.error)
            + t(&self.prompt_title)
            + t(&self.prompt)
            + t(&self.formula1)
            + t(&self.formula2)
    }
}

/// The `<dataValidations>` container.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataValidations {
    /// Disable all prompts.
    pub disable_prompts: Option<bool>,
    /// Prompt window x position.
    pub x_window: Option<u32>,
    /// Prompt window y position.
    pub y_window: Option<u32>,
    /// Validation rules.
    pub validations: Vec<DataValidation>,
}
impl DataValidations {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.validations.capacity() * size_of::<DataValidation>()
            + self
                .validations
                .iter()
                .map(DataValidation::heap_bytes)
                .sum::<usize>()
    }
}
