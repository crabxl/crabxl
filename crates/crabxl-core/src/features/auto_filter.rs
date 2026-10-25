//! Worksheet and table auto filters and sort state.
//!
//! Mirrors `openpyxl/worksheet/filters.py` (attributes in snake_case).

macro_rules! xml_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
        }
        impl $name {
            /// Parse the XML attribute value.
            pub fn from_xml(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
            /// The XML attribute value.
            pub fn as_xml(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }
        }
    };
}

xml_enum! {
    /// Sort key kind (`sortBy`).
    SortBy {
        /// Sort by cell value.
        Value = "value",
        /// Sort by cell color.
        CellColor = "cellColor",
        /// Sort by font color.
        FontColor = "fontColor",
        /// Sort by icon.
        Icon = "icon",
    }
}

xml_enum! {
    /// Sort method for East Asian text (`sortMethod`).
    SortMethod {
        /// Sort by stroke count.
        Stroke = "stroke",
        /// Sort by pin-yin.
        PinYin = "pinYin",
    }
}

xml_enum! {
    /// Icon set names (`iconSet`).
    FilterIconSet {
        /// `3Arrows`.
        Arrows3 = "3Arrows",
        /// `3ArrowsGray`.
        ArrowsGray3 = "3ArrowsGray",
        /// `3Flags`.
        Flags3 = "3Flags",
        /// `3TrafficLights1`.
        TrafficLights3 = "3TrafficLights1",
        /// `3TrafficLights2`.
        TrafficLights3Rimmed = "3TrafficLights2",
        /// `3Signs`.
        Signs3 = "3Signs",
        /// `3Symbols`.
        Symbols3 = "3Symbols",
        /// `3Symbols2`.
        Symbols3Uncircled = "3Symbols2",
        /// `4Arrows`.
        Arrows4 = "4Arrows",
        /// `4ArrowsGray`.
        ArrowsGray4 = "4ArrowsGray",
        /// `4RedToBlack`.
        RedToBlack4 = "4RedToBlack",
        /// `4Rating`.
        Rating4 = "4Rating",
        /// `4TrafficLights`.
        TrafficLights4 = "4TrafficLights",
        /// `5Arrows`.
        Arrows5 = "5Arrows",
        /// `5ArrowsGray`.
        ArrowsGray5 = "5ArrowsGray",
        /// `5Rating`.
        Rating5 = "5Rating",
        /// `5Quarters`.
        Quarters5 = "5Quarters",
    }
}

xml_enum! {
    /// Dynamic filter type.
    DynamicFilterType {
        /// `null`.
        Null = "null",
        /// `aboveAverage`.
        AboveAverage = "aboveAverage",
        /// `belowAverage`.
        BelowAverage = "belowAverage",
        /// `tomorrow`.
        Tomorrow = "tomorrow",
        /// `today`.
        Today = "today",
        /// `yesterday`.
        Yesterday = "yesterday",
        /// `nextWeek`.
        NextWeek = "nextWeek",
        /// `thisWeek`.
        ThisWeek = "thisWeek",
        /// `lastWeek`.
        LastWeek = "lastWeek",
        /// `nextMonth`.
        NextMonth = "nextMonth",
        /// `thisMonth`.
        ThisMonth = "thisMonth",
        /// `lastMonth`.
        LastMonth = "lastMonth",
        /// `nextQuarter`.
        NextQuarter = "nextQuarter",
        /// `thisQuarter`.
        ThisQuarter = "thisQuarter",
        /// `lastQuarter`.
        LastQuarter = "lastQuarter",
        /// `nextYear`.
        NextYear = "nextYear",
        /// `thisYear`.
        ThisYear = "thisYear",
        /// `lastYear`.
        LastYear = "lastYear",
        /// `yearToDate`.
        YearToDate = "yearToDate",
        /// `Q1`.
        Q1 = "Q1",
        /// `Q2`.
        Q2 = "Q2",
        /// `Q3`.
        Q3 = "Q3",
        /// `Q4`.
        Q4 = "Q4",
        /// `M1`.
        M1 = "M1",
        /// `M2`.
        M2 = "M2",
        /// `M3`.
        M3 = "M3",
        /// `M4`.
        M4 = "M4",
        /// `M5`.
        M5 = "M5",
        /// `M6`.
        M6 = "M6",
        /// `M7`.
        M7 = "M7",
        /// `M8`.
        M8 = "M8",
        /// `M9`.
        M9 = "M9",
        /// `M10`.
        M10 = "M10",
        /// `M11`.
        M11 = "M11",
        /// `M12`.
        M12 = "M12",
    }
}

xml_enum! {
    /// Custom filter comparison operator.
    CustomFilterOperator {
        /// `equal`.
        Equal = "equal",
        /// `lessThan`.
        LessThan = "lessThan",
        /// `lessThanOrEqual`.
        LessThanOrEqual = "lessThanOrEqual",
        /// `notEqual`.
        NotEqual = "notEqual",
        /// `greaterThanOrEqual`.
        GreaterThanOrEqual = "greaterThanOrEqual",
        /// `greaterThan`.
        GreaterThan = "greaterThan",
    }
}

xml_enum! {
    /// Date grouping granularity.
    DateTimeGrouping {
        /// Year.
        Year = "year",
        /// Month.
        Month = "month",
        /// Day.
        Day = "day",
        /// Hour.
        Hour = "hour",
        /// Minute.
        Minute = "minute",
        /// Second.
        Second = "second",
    }
}

xml_enum! {
    /// Calendar used by date group items.
    FilterCalendarType {
        /// `gregorian`.
        Gregorian = "gregorian",
        /// `gregorianUs`.
        GregorianUs = "gregorianUs",
        /// `gregorianMeFrench`.
        GregorianMeFrench = "gregorianMeFrench",
        /// `gregorianArabic`.
        GregorianArabic = "gregorianArabic",
        /// `hijri`.
        Hijri = "hijri",
        /// `hebrew`.
        Hebrew = "hebrew",
        /// `taiwan`.
        Taiwan = "taiwan",
        /// `japan`.
        Japan = "japan",
        /// `thai`.
        Thai = "thai",
        /// `korea`.
        Korea = "korea",
        /// `saka`.
        Saka = "saka",
        /// `gregorianXlitEnglish`.
        GregorianXlitEnglish = "gregorianXlitEnglish",
        /// `gregorianXlitFrench`.
        GregorianXlitFrench = "gregorianXlitFrench",
    }
}

fn text(value: &str) -> usize {
    value.len()
}
fn opt_text(value: &Option<Box<str>>) -> usize {
    value.as_deref().map_or(0, str::len)
}

/// One sort condition (`<sortCondition>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortCondition {
    /// Range the condition sorts (`ref`).
    pub ref_range: Box<str>,
    /// Descending order (`descending`).
    pub descending: Option<bool>,
    /// Sort key kind (`sortBy`).
    pub sort_by: Option<SortBy>,
    /// Custom list (`customList`).
    pub custom_list: Option<Box<str>>,
    /// Differential format id (`dxfId`).
    pub dxf_id: Option<u32>,
    /// Icon set (`iconSet`).
    pub icon_set: Option<FilterIconSet>,
    /// Icon id (`iconId`).
    pub icon_id: Option<u32>,
}
impl SortCondition {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        text(&self.ref_range) + opt_text(&self.custom_list)
    }
}

/// Sort state (`<sortState>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortState {
    /// Sort by columns instead of rows (`columnSort`).
    pub column_sort: Option<bool>,
    /// Case-sensitive sort (`caseSensitive`).
    pub case_sensitive: Option<bool>,
    /// Sort method (`sortMethod`).
    pub sort_method: Option<SortMethod>,
    /// Sorted range (`ref`).
    pub ref_range: Box<str>,
    /// Sort conditions.
    pub sort_condition: Vec<SortCondition>,
}
impl SortState {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        text(&self.ref_range)
            + self.sort_condition.capacity() * size_of::<SortCondition>()
            + self
                .sort_condition
                .iter()
                .map(SortCondition::heap_bytes)
                .sum::<usize>()
    }
}

/// Icon filter (`<iconFilter>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconFilter {
    /// Icon set (`iconSet`).
    pub icon_set: FilterIconSet,
    /// Icon id (`iconId`).
    pub icon_id: Option<u32>,
}
impl Default for IconFilter {
    fn default() -> Self {
        Self {
            icon_set: FilterIconSet::Arrows3,
            icon_id: None,
        }
    }
}

/// Color filter (`<colorFilter>`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColorFilter {
    /// Differential format id (`dxfId`).
    pub dxf_id: Option<u32>,
    /// Filter by cell color instead of font color (`cellColor`).
    pub cell_color: Option<bool>,
}

/// Dynamic filter (`<dynamicFilter>`).
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicFilter {
    /// Filter type (`type`).
    pub filter_type: DynamicFilterType,
    /// Value (`val`).
    pub val: Option<f64>,
    /// ISO date-time value text (`valIso`).
    pub val_iso: Option<Box<str>>,
    /// Maximum value (`maxVal`).
    pub max_val: Option<f64>,
    /// ISO date-time maximum text (`maxValIso`).
    pub max_val_iso: Option<Box<str>>,
}
impl Default for DynamicFilter {
    fn default() -> Self {
        Self {
            filter_type: DynamicFilterType::Null,
            val: None,
            val_iso: None,
            max_val: None,
            max_val_iso: None,
        }
    }
}
impl DynamicFilter {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        opt_text(&self.val_iso) + opt_text(&self.max_val_iso)
    }
}

/// One custom filter criterion (`<customFilter>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomFilter {
    /// Comparison operator (`operator`; absent means equal).
    pub operator: Option<CustomFilterOperator>,
    /// Comparison value (`val`).
    pub val: Option<Box<str>>,
}
impl CustomFilter {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        opt_text(&self.val)
    }
}

/// Custom filters (`<customFilters>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomFilters {
    /// Combine criteria with AND (`and`).
    pub and: Option<bool>,
    /// Criteria (one or two).
    pub custom_filter: Vec<CustomFilter>,
}
impl CustomFilters {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.custom_filter.capacity() * size_of::<CustomFilter>()
            + self
                .custom_filter
                .iter()
                .map(CustomFilter::heap_bytes)
                .sum::<usize>()
    }
}

/// Top-N filter (`<top10>`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Top10 {
    /// Top rather than bottom (`top`).
    pub top: Option<bool>,
    /// Value is a percentage (`percent`).
    pub percent: Option<bool>,
    /// Item count or percentage (`val`).
    pub val: f64,
    /// Cutoff value (`filterVal`).
    pub filter_val: Option<f64>,
}

/// Date group item (`<dateGroupItem>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateGroupItem {
    /// Year.
    pub year: u32,
    /// Month 1-12.
    pub month: Option<u32>,
    /// Day 1-31.
    pub day: Option<u32>,
    /// Hour 0-23.
    pub hour: Option<u32>,
    /// Minute 0-59.
    pub minute: Option<u32>,
    /// Second 0-59.
    pub second: Option<u32>,
    /// Grouping granularity (`dateTimeGrouping`).
    pub date_time_grouping: DateTimeGrouping,
}
impl Default for DateGroupItem {
    fn default() -> Self {
        Self {
            year: 0,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
            date_time_grouping: DateTimeGrouping::Year,
        }
    }
}

/// Value filters (`<filters>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    /// Include blank cells (`blank`).
    pub blank: Option<bool>,
    /// Calendar type (`calendarType`).
    pub calendar_type: Option<FilterCalendarType>,
    /// Selected values.
    pub filter: Vec<Box<str>>,
    /// Date group items.
    pub date_group_item: Vec<DateGroupItem>,
}
impl Filters {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.filter.capacity() * size_of::<Box<str>>()
            + self.filter.iter().map(|v| v.len()).sum::<usize>()
            + self.date_group_item.capacity() * size_of::<DateGroupItem>()
    }
}

/// Filter on one column (`<filterColumn>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilterColumn {
    /// Zero-based column id (`colId`).
    pub col_id: u32,
    /// Hide the drop-down button (`hiddenButton`).
    pub hidden_button: Option<bool>,
    /// Show the drop-down button (`showButton`).
    pub show_button: Option<bool>,
    /// Value filters.
    pub filters: Option<Filters>,
    /// Top-N filter.
    pub top10: Option<Top10>,
    /// Custom filters.
    pub custom_filters: Option<CustomFilters>,
    /// Dynamic filter.
    pub dynamic_filter: Option<DynamicFilter>,
    /// Color filter.
    pub color_filter: Option<ColorFilter>,
    /// Icon filter.
    pub icon_filter: Option<IconFilter>,
}
impl FilterColumn {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.filters.as_ref().map_or(0, Filters::heap_bytes)
            + self
                .custom_filters
                .as_ref()
                .map_or(0, CustomFilters::heap_bytes)
            + self
                .dynamic_filter
                .as_ref()
                .map_or(0, DynamicFilter::heap_bytes)
    }
}

/// Auto filter (`<autoFilter>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutoFilter {
    /// Filtered range (`ref`).
    pub ref_range: Option<Box<str>>,
    /// Per-column filters.
    pub filter_column: Vec<FilterColumn>,
    /// Sort state.
    pub sort_state: Option<SortState>,
}
impl AutoFilter {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        opt_text(&self.ref_range)
            + self.filter_column.capacity() * size_of::<FilterColumn>()
            + self
                .filter_column
                .iter()
                .map(FilterColumn::heap_bytes)
                .sum::<usize>()
            + self.sort_state.as_ref().map_or(0, SortState::heap_bytes)
    }
}
