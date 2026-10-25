//! `<autoFilter>` and `<sortState>` codecs (worksheet and table scope).
use crate::encode::write_attribute;
use crate::metadata::{attributes, boolean};
use crate::printing::namespace;
use crate::xml::{Scope, XmlStream};
use crabxl_core::features::{
    AutoFilter, ColorFilter, CustomFilter, CustomFilterOperator, CustomFilters, DateGroupItem,
    DateTimeGrouping, DynamicFilter, DynamicFilterType, FilterCalendarType, FilterColumn,
    FilterIconSet, Filters, IconFilter, SortBy, SortCondition, SortMethod, SortState, Top10,
};
use crabxl_core::{Error, Result};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

pub(crate) fn unknown_attribute() -> Error {
    super::unsupported("Unsupported attribute in table or filter element")
}
pub(crate) fn unknown_element() -> Error {
    super::unsupported("Unsupported element in table or filter")
}
pub(crate) fn uint(value: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| super::invalid("Invalid unsigned integer attribute"))
}
pub(crate) fn float(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| super::invalid("Invalid numeric attribute"))
}
pub(crate) fn opt_bool(value: &str) -> Result<Option<bool>> {
    boolean(value).map(Some)
}
fn enumerated<T>(value: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T> {
    parse(value).ok_or_else(|| super::invalid("Invalid enumerated attribute"))
}

/// Visit each child element start of the element at `depth`; `f` must consume
/// the child through its matching end. Text other than whitespace is rejected.
pub(crate) fn children<B: BufRead>(
    xml: &mut XmlStream<B>,
    depth: usize,
    mut f: impl FnMut(&mut XmlStream<B>, &[u8], &BytesStart<'static>, usize) -> Result<()>,
) -> Result<()> {
    loop {
        let frame = xml.next()?;
        let frame_depth = frame.depth;
        let scope = frame.scope;
        match frame.event {
            Event::Start(e) => {
                if scope != Scope::Spreadsheet {
                    return Err(unknown_element());
                }
                let owned = e.into_owned();
                let name = owned.local_name();
                f(xml, name.as_ref().as_bytes(), &owned, frame_depth)?;
            }
            Event::End(_) if frame_depth + 1 == depth => return Ok(()),
            Event::Text(t) if t.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => return Err(super::invalid("Incomplete XML element")),
            _ => return Err(super::invalid("Unexpected content in element")),
        }
    }
}
/// Consume an element that must have no children.
pub(crate) fn leaf<B: BufRead>(xml: &mut XmlStream<B>, depth: usize) -> Result<()> {
    children(xml, depth, |_, _, _, _| Err(unknown_element()))
}

pub(crate) fn read_auto_filter<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<AutoFilter> {
    let mut filter = AutoFilter::default();
    attributes(start, |name, value| match name {
        b"ref" => {
            filter.ref_range = Some(value.into());
            Ok(())
        }
        _ => Err(unknown_attribute()),
    })?;
    children(xml, depth, |xml, name, e, d| match name {
        b"filterColumn" => {
            filter.filter_column.push(read_filter_column(xml, e, d)?);
            Ok(())
        }
        b"sortState" if filter.sort_state.is_none() => {
            filter.sort_state = Some(read_sort_state(xml, e, d)?);
            Ok(())
        }
        _ => Err(unknown_element()),
    })?;
    Ok(filter)
}

fn read_filter_column<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<FilterColumn> {
    let mut column = FilterColumn::default();
    let mut has_id = false;
    attributes(start, |name, value| {
        match name {
            b"colId" => {
                column.col_id = uint(value)?;
                has_id = true;
            }
            b"hiddenButton" => column.hidden_button = opt_bool(value)?,
            b"showButton" => column.show_button = opt_bool(value)?,
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !has_id {
        return Err(super::invalid("Missing XML attribute colId"));
    }
    children(xml, depth, |xml, name, e, d| {
        match name {
            b"filters" if column.filters.is_none() => {
                column.filters = Some(read_filters(xml, e, d)?)
            }
            b"top10" if column.top10.is_none() => column.top10 = Some(read_top10(xml, e, d)?),
            b"customFilters" if column.custom_filters.is_none() => {
                column.custom_filters = Some(read_custom_filters(xml, e, d)?)
            }
            b"dynamicFilter" if column.dynamic_filter.is_none() => {
                column.dynamic_filter = Some(read_dynamic(xml, e, d)?)
            }
            b"colorFilter" if column.color_filter.is_none() => {
                column.color_filter = Some(read_color(xml, e, d)?)
            }
            b"iconFilter" if column.icon_filter.is_none() => {
                column.icon_filter = Some(read_icon(xml, e, d)?)
            }
            _ => return Err(unknown_element()),
        }
        Ok(())
    })?;
    Ok(column)
}

fn read_filters<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<Filters> {
    let mut filters = Filters::default();
    attributes(start, |name, value| {
        match name {
            b"blank" => filters.blank = opt_bool(value)?,
            b"calendarType" => {
                filters.calendar_type = Some(enumerated(value, FilterCalendarType::from_xml)?)
            }
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    children(xml, depth, |xml, name, e, d| match name {
        b"filter" => {
            let mut val = None;
            attributes(e, |n, v| match n {
                b"val" => {
                    val = Some(v.into());
                    Ok(())
                }
                _ => Err(unknown_attribute()),
            })?;
            filters
                .filter
                .push(val.ok_or_else(|| super::invalid("Missing XML attribute val"))?);
            leaf(xml, d)
        }
        b"dateGroupItem" => {
            let mut item = DateGroupItem::default();
            let (mut year, mut grouping) = (false, false);
            attributes(e, |n, v| {
                match n {
                    b"year" => {
                        item.year = uint(v)?;
                        year = true;
                    }
                    b"month" => item.month = Some(uint(v)?),
                    b"day" => item.day = Some(uint(v)?),
                    b"hour" => item.hour = Some(uint(v)?),
                    b"minute" => item.minute = Some(uint(v)?),
                    b"second" => item.second = Some(uint(v)?),
                    b"dateTimeGrouping" => {
                        item.date_time_grouping = enumerated(v, DateTimeGrouping::from_xml)?;
                        grouping = true;
                    }
                    _ => return Err(unknown_attribute()),
                }
                Ok(())
            })?;
            if !year || !grouping {
                return Err(super::invalid("Missing dateGroupItem attribute"));
            }
            filters.date_group_item.push(item);
            leaf(xml, d)
        }
        _ => Err(unknown_element()),
    })?;
    Ok(filters)
}

fn read_top10<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<Top10> {
    let mut top = Top10::default();
    let mut has_val = false;
    attributes(start, |name, value| {
        match name {
            b"top" => top.top = opt_bool(value)?,
            b"percent" => top.percent = opt_bool(value)?,
            b"val" => {
                top.val = float(value)?;
                has_val = true;
            }
            b"filterVal" => top.filter_val = Some(float(value)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !has_val {
        return Err(super::invalid("Missing XML attribute val"));
    }
    leaf(xml, depth)?;
    Ok(top)
}

fn read_custom_filters<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<CustomFilters> {
    let mut filters = CustomFilters::default();
    attributes(start, |name, value| match name {
        b"and" => {
            filters.and = opt_bool(value)?;
            Ok(())
        }
        _ => Err(unknown_attribute()),
    })?;
    children(xml, depth, |xml, name, e, d| match name {
        b"customFilter" => {
            let mut item = CustomFilter::default();
            attributes(e, |n, v| {
                match n {
                    b"operator" => {
                        item.operator = Some(enumerated(v, CustomFilterOperator::from_xml)?)
                    }
                    b"val" => item.val = Some(v.into()),
                    _ => return Err(unknown_attribute()),
                }
                Ok(())
            })?;
            filters.custom_filter.push(item);
            leaf(xml, d)
        }
        _ => Err(unknown_element()),
    })?;
    Ok(filters)
}

fn read_dynamic<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<DynamicFilter> {
    let mut filter = DynamicFilter::default();
    let mut has_type = false;
    attributes(start, |name, value| {
        match name {
            b"type" => {
                filter.filter_type = enumerated(value, DynamicFilterType::from_xml)?;
                has_type = true;
            }
            b"val" => filter.val = Some(float(value)?),
            b"valIso" => filter.val_iso = Some(value.into()),
            b"maxVal" => filter.max_val = Some(float(value)?),
            b"maxValIso" => filter.max_val_iso = Some(value.into()),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !has_type {
        return Err(super::invalid("Missing XML attribute type"));
    }
    leaf(xml, depth)?;
    Ok(filter)
}

fn read_color<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<ColorFilter> {
    let mut filter = ColorFilter::default();
    attributes(start, |name, value| {
        match name {
            b"dxfId" => filter.dxf_id = Some(uint(value)?),
            b"cellColor" => filter.cell_color = opt_bool(value)?,
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    leaf(xml, depth)?;
    Ok(filter)
}

fn read_icon<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<IconFilter> {
    let mut filter = IconFilter::default();
    let mut has_set = false;
    attributes(start, |name, value| {
        match name {
            b"iconSet" => {
                filter.icon_set = enumerated(value, FilterIconSet::from_xml)?;
                has_set = true;
            }
            b"iconId" => filter.icon_id = Some(uint(value)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !has_set {
        return Err(super::invalid("Missing XML attribute iconSet"));
    }
    leaf(xml, depth)?;
    Ok(filter)
}

pub(crate) fn read_sort_state<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<SortState> {
    let mut sort = SortState::default();
    let mut has_ref = false;
    attributes(start, |name, value| {
        match name {
            b"columnSort" => sort.column_sort = opt_bool(value)?,
            b"caseSensitive" => sort.case_sensitive = opt_bool(value)?,
            b"sortMethod" => sort.sort_method = Some(enumerated(value, SortMethod::from_xml)?),
            b"ref" => {
                sort.ref_range = value.into();
                has_ref = true;
            }
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !has_ref {
        return Err(super::invalid("Missing XML attribute ref"));
    }
    children(xml, depth, |xml, name, e, d| match name {
        b"sortCondition" => {
            let mut cond = SortCondition::default();
            let mut has_ref = false;
            attributes(e, |n, v| {
                match n {
                    b"ref" => {
                        cond.ref_range = v.into();
                        has_ref = true;
                    }
                    b"descending" => cond.descending = opt_bool(v)?,
                    b"sortBy" => cond.sort_by = Some(enumerated(v, SortBy::from_xml)?),
                    b"customList" => cond.custom_list = Some(v.into()),
                    b"dxfId" => cond.dxf_id = Some(uint(v)?),
                    b"iconSet" => cond.icon_set = Some(enumerated(v, FilterIconSet::from_xml)?),
                    b"iconId" => cond.icon_id = Some(uint(v)?),
                    _ => return Err(unknown_attribute()),
                }
                Ok(())
            })?;
            if !has_ref {
                return Err(super::invalid("Missing XML attribute ref"));
            }
            sort.sort_condition.push(cond);
            leaf(xml, d)
        }
        _ => Err(unknown_element()),
    })?;
    Ok(sort)
}

// ---------------------------------------------------------------- writers

pub(crate) fn put_bool(out: &mut impl Write, name: &str, value: Option<bool>) -> io::Result<()> {
    if let Some(value) = value {
        write_attribute(out, name, if value { "1" } else { "0" })?;
    }
    Ok(())
}
pub(crate) fn put_num(out: &mut impl Write, name: &str, value: Option<u32>) -> io::Result<()> {
    if let Some(value) = value {
        write!(out, " {name}=\"{value}\"")?;
    }
    Ok(())
}
pub(crate) fn put_str(out: &mut impl Write, name: &str, value: Option<&str>) -> io::Result<()> {
    if let Some(value) = value {
        write_attribute(out, name, value)?;
    }
    Ok(())
}
fn put_float(out: &mut impl Write, name: &str, value: Option<f64>) -> io::Result<()> {
    if let Some(value) = value {
        if !value.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Non-finite filter value",
            ));
        }
        write!(out, " {name}=\"{value}\"")?;
    }
    Ok(())
}
fn open(out: &mut impl Write, name: &str, uri: Option<&str>) -> io::Result<()> {
    write!(out, "<{name}")?;
    namespace(out, uri)
}

pub(crate) fn write_auto_filter(
    out: &mut impl Write,
    filter: &AutoFilter,
    uri: Option<&str>,
) -> io::Result<()> {
    open(out, "autoFilter", uri)?;
    put_str(out, "ref", filter.ref_range.as_deref())?;
    if filter.filter_column.is_empty() && filter.sort_state.is_none() {
        return out.write_all(b"/>");
    }
    out.write_all(b">")?;
    for column in &filter.filter_column {
        write_filter_column(out, column, uri)?;
    }
    if let Some(sort) = &filter.sort_state {
        write_sort_state(out, sort, uri)?;
    }
    out.write_all(b"</autoFilter>")
}

fn write_filter_column(
    out: &mut impl Write,
    column: &FilterColumn,
    uri: Option<&str>,
) -> io::Result<()> {
    open(out, "filterColumn", uri)?;
    put_num(out, "colId", Some(column.col_id))?;
    put_bool(out, "hiddenButton", column.hidden_button)?;
    put_bool(out, "showButton", column.show_button)?;
    out.write_all(b">")?;
    if let Some(filters) = &column.filters {
        open(out, "filters", uri)?;
        put_bool(out, "blank", filters.blank)?;
        put_str(
            out,
            "calendarType",
            filters.calendar_type.map(FilterCalendarType::as_xml),
        )?;
        out.write_all(b">")?;
        for value in &filters.filter {
            open(out, "filter", uri)?;
            write_attribute(out, "val", value)?;
            out.write_all(b"/>")?;
        }
        for item in &filters.date_group_item {
            open(out, "dateGroupItem", uri)?;
            put_num(out, "year", Some(item.year))?;
            put_num(out, "month", item.month)?;
            put_num(out, "day", item.day)?;
            put_num(out, "hour", item.hour)?;
            put_num(out, "minute", item.minute)?;
            put_num(out, "second", item.second)?;
            put_str(
                out,
                "dateTimeGrouping",
                Some(item.date_time_grouping.as_xml()),
            )?;
            out.write_all(b"/>")?;
        }
        out.write_all(b"</filters>")?;
    }
    if let Some(top) = &column.top10 {
        open(out, "top10", uri)?;
        put_bool(out, "top", top.top)?;
        put_bool(out, "percent", top.percent)?;
        put_float(out, "val", Some(top.val))?;
        put_float(out, "filterVal", top.filter_val)?;
        out.write_all(b"/>")?;
    }
    if let Some(custom) = &column.custom_filters {
        open(out, "customFilters", uri)?;
        put_bool(out, "and", custom.and)?;
        out.write_all(b">")?;
        for item in &custom.custom_filter {
            open(out, "customFilter", uri)?;
            put_str(
                out,
                "operator",
                item.operator.map(CustomFilterOperator::as_xml),
            )?;
            put_str(out, "val", item.val.as_deref())?;
            out.write_all(b"/>")?;
        }
        out.write_all(b"</customFilters>")?;
    }
    if let Some(dynamic) = &column.dynamic_filter {
        open(out, "dynamicFilter", uri)?;
        put_str(out, "type", Some(dynamic.filter_type.as_xml()))?;
        put_float(out, "val", dynamic.val)?;
        put_str(out, "valIso", dynamic.val_iso.as_deref())?;
        put_float(out, "maxVal", dynamic.max_val)?;
        put_str(out, "maxValIso", dynamic.max_val_iso.as_deref())?;
        out.write_all(b"/>")?;
    }
    if let Some(color) = &column.color_filter {
        open(out, "colorFilter", uri)?;
        put_num(out, "dxfId", color.dxf_id)?;
        put_bool(out, "cellColor", color.cell_color)?;
        out.write_all(b"/>")?;
    }
    if let Some(icon) = &column.icon_filter {
        open(out, "iconFilter", uri)?;
        put_str(out, "iconSet", Some(icon.icon_set.as_xml()))?;
        put_num(out, "iconId", icon.icon_id)?;
        out.write_all(b"/>")?;
    }
    out.write_all(b"</filterColumn>")
}

pub(crate) fn write_sort_state(
    out: &mut impl Write,
    sort: &SortState,
    uri: Option<&str>,
) -> io::Result<()> {
    open(out, "sortState", uri)?;
    put_bool(out, "columnSort", sort.column_sort)?;
    put_bool(out, "caseSensitive", sort.case_sensitive)?;
    put_str(out, "sortMethod", sort.sort_method.map(SortMethod::as_xml))?;
    write_attribute(out, "ref", &sort.ref_range)?;
    if sort.sort_condition.is_empty() {
        return out.write_all(b"/>");
    }
    out.write_all(b">")?;
    for cond in &sort.sort_condition {
        open(out, "sortCondition", uri)?;
        put_bool(out, "descending", cond.descending)?;
        put_str(out, "sortBy", cond.sort_by.map(SortBy::as_xml))?;
        write_attribute(out, "ref", &cond.ref_range)?;
        put_str(out, "customList", cond.custom_list.as_deref())?;
        put_num(out, "dxfId", cond.dxf_id)?;
        put_str(out, "iconSet", cond.icon_set.map(FilterIconSet::as_xml))?;
        put_num(out, "iconId", cond.icon_id)?;
        out.write_all(b"/>")?;
    }
    out.write_all(b"</sortState>")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crabxl_core::{ErrorKind, ResourceLimits};
    use std::io::Cursor;

    const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

    fn stream(source: &str) -> XmlStream<Cursor<&[u8]>> {
        XmlStream::new(
            Cursor::new(source.as_bytes()),
            "sheet.xml".into(),
            source.len() as u64,
            ResourceLimits::default(),
        )
    }
    fn parse_filter(source: &str) -> Result<AutoFilter> {
        let mut xml = stream(source);
        let (start, depth) = {
            let f = xml.next()?;
            let Event::Start(e) = f.event else {
                panic!("start")
            };
            (e.into_owned(), f.depth)
        };
        read_auto_filter(&mut xml, &start, depth)
    }
    fn parse_sort(source: &str) -> Result<SortState> {
        let mut xml = stream(source);
        let (start, depth) = {
            let f = xml.next()?;
            let Event::Start(e) = f.event else {
                panic!("start")
            };
            (e.into_owned(), f.depth)
        };
        read_sort_state(&mut xml, &start, depth)
    }

    const FULL: &str = r#"<autoFilter xmlns="NS" ref="A1:J20"><filterColumn colId="0" hiddenButton="1" showButton="0"><filters blank="1" calendarType="japan"><filter val="a&amp;b"/><filter val="x"/><dateGroupItem year="2020" month="2" day="3" hour="4" minute="5" second="6" dateTimeGrouping="second"/><dateGroupItem year="2021" dateTimeGrouping="year"/></filters></filterColumn><filterColumn colId="1"><top10 top="1" percent="0" val="10" filterVal="2.5"/></filterColumn><filterColumn colId="2"><customFilters and="1"><customFilter operator="greaterThan" val="5"/><customFilter val="*x*"/></customFilters></filterColumn><filterColumn colId="3"><dynamicFilter type="aboveAverage" val="1.5" valIso="2020-01-01T00:00:00" maxVal="3" maxValIso="2021-01-01T00:00:00"/></filterColumn><filterColumn colId="4"><colorFilter dxfId="3" cellColor="0"/></filterColumn><filterColumn colId="5"><iconFilter iconSet="3TrafficLights2" iconId="1"/></filterColumn><sortState columnSort="1" caseSensitive="1" sortMethod="stroke" ref="A2:J20"><sortCondition descending="1" sortBy="cellColor" ref="A2:A20" customList="a,b" dxfId="1" iconSet="4Rating" iconId="2"/><sortCondition ref="B2:B20"/></sortState></autoFilter>"#;

    fn write_f(f: &AutoFilter, uri: Option<&str>) -> String {
        let mut out = Vec::new();
        write_auto_filter(&mut out, f, uri).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn full_roundtrip() {
        let src = FULL.replace("NS", NS);
        let f = parse_filter(&src).unwrap();
        assert_eq!(f.filter_column.len(), 6);
        let c0 = &f.filter_column[0];
        assert_eq!(c0.hidden_button, Some(true));
        let fl = c0.filters.as_ref().unwrap();
        assert_eq!(&*fl.filter[0], "a&b");
        assert_eq!(fl.calendar_type, Some(FilterCalendarType::Japan));
        assert_eq!(fl.date_group_item[0].second, Some(6));
        assert_eq!(f.filter_column[1].top10.unwrap().filter_val, Some(2.5));
        assert_eq!(f.sort_state.as_ref().unwrap().sort_condition.len(), 2);
        assert!(f.heap_bytes() > 0);
        let text = write_f(&f, Some(NS));
        assert!(text.starts_with("<autoFilter xmlns=\""));
        assert_eq!(parse_filter(&text).unwrap(), f);
        let plain = write_f(&f, None);
        assert!(!plain.contains("xmlns"));
        let wrapped = format!("<root xmlns=\"{NS}\">{plain}</root>");
        let mut xml = stream(&wrapped);
        xml.next().unwrap();
        let (start, depth) = {
            let fr = xml.next().unwrap();
            let Event::Start(e) = fr.event else { panic!() };
            (e.into_owned(), fr.depth)
        };
        assert_eq!(read_auto_filter(&mut xml, &start, depth).unwrap(), f);
    }

    #[test]
    fn empty_and_openpyxl() {
        let f = parse_filter(&format!("<autoFilter xmlns=\"{NS}\" ref=\"A1:C3\"/>")).unwrap();
        assert_eq!(f.ref_range.as_deref(), Some("A1:C3"));
        assert_eq!(write_f(&f, None), "<autoFilter ref=\"A1:C3\"/>");
        let g = parse_filter(&format!("<autoFilter xmlns=\"{NS}\"/>")).unwrap();
        assert_eq!(g, AutoFilter::default());
    }

    #[test]
    fn strict_namespace() {
        let strict = "http://purl.oclc.org/ooxml/spreadsheetml/main";
        let a = parse_filter(&FULL.replace("NS", strict)).unwrap();
        assert_eq!(a, parse_filter(&FULL.replace("NS", NS)).unwrap());
    }

    #[test]
    fn sort_state_roundtrip() {
        let src = format!(
            "<sortState xmlns=\"{NS}\" ref=\"A1:B9\"><sortCondition descending=\"1\" ref=\"A1:A9\"/></sortState>"
        );
        let s = parse_sort(&src).unwrap();
        let mut out = Vec::new();
        write_sort_state(&mut out, &s, Some(NS)).unwrap();
        assert_eq!(parse_sort(std::str::from_utf8(&out).unwrap()).unwrap(), s);
        assert!(s.heap_bytes() > 0);
    }

    #[test]
    fn every_enum_roundtrips() {
        for v in [
            "null",
            "aboveAverage",
            "belowAverage",
            "tomorrow",
            "today",
            "yesterday",
            "nextWeek",
            "thisWeek",
            "lastWeek",
            "nextMonth",
            "thisMonth",
            "lastMonth",
            "nextQuarter",
            "thisQuarter",
            "lastQuarter",
            "nextYear",
            "thisYear",
            "lastYear",
            "yearToDate",
            "Q1",
            "Q4",
            "M1",
            "M12",
        ] {
            assert_eq!(DynamicFilterType::from_xml(v).unwrap().as_xml(), v);
        }
        for v in [
            "3Arrows",
            "3ArrowsGray",
            "3Flags",
            "3TrafficLights1",
            "3TrafficLights2",
            "3Signs",
            "3Symbols",
            "3Symbols2",
            "4Arrows",
            "4ArrowsGray",
            "4RedToBlack",
            "4Rating",
            "4TrafficLights",
            "5Arrows",
            "5ArrowsGray",
            "5Rating",
            "5Quarters",
        ] {
            assert_eq!(FilterIconSet::from_xml(v).unwrap().as_xml(), v);
        }
        for v in [
            "gregorian",
            "gregorianUs",
            "gregorianMeFrench",
            "gregorianArabic",
            "hijri",
            "hebrew",
            "taiwan",
            "japan",
            "thai",
            "korea",
            "saka",
            "gregorianXlitEnglish",
            "gregorianXlitFrench",
        ] {
            assert_eq!(FilterCalendarType::from_xml(v).unwrap().as_xml(), v);
        }
        for v in [
            "equal",
            "lessThan",
            "lessThanOrEqual",
            "notEqual",
            "greaterThanOrEqual",
            "greaterThan",
        ] {
            assert_eq!(CustomFilterOperator::from_xml(v).unwrap().as_xml(), v);
        }
        for v in ["year", "month", "day", "hour", "minute", "second"] {
            assert_eq!(DateTimeGrouping::from_xml(v).unwrap().as_xml(), v);
        }
        for v in ["value", "cellColor", "fontColor", "icon"] {
            assert_eq!(SortBy::from_xml(v).unwrap().as_xml(), v);
        }
    }

    #[test]
    fn rejects_unknown_and_invalid() {
        let un = |inner: &str, attr: &str| {
            let src =
                format!("<autoFilter xmlns=\"{NS}\" ref=\"A1:B2\"{attr}>{inner}</autoFilter>");
            assert_eq!(
                parse_filter(&src).unwrap_err().kind(),
                ErrorKind::Unsupported,
                "{src}"
            );
        };
        un("<extLst/>", "");
        un("", " foo=\"1\"");
        un("<filterColumn colId=\"0\" zzz=\"1\"/>", "");
        un("<filterColumn colId=\"0\"><weird/></filterColumn>", "");
        un(
            "<filterColumn colId=\"0\"><top10 val=\"1\" q=\"1\"/></filterColumn>",
            "",
        );
        un("<sortState ref=\"A1\" ext=\"1\"/>", "");
        let bad = |inner: &str| {
            let src = format!("<autoFilter xmlns=\"{NS}\">{inner}</autoFilter>");
            assert!(parse_filter(&src).is_err(), "{src}");
        };
        bad("<filterColumn/>");
        bad("<filterColumn colId=\"x\"/>");
        bad("<filterColumn colId=\"0\"><top10/></filterColumn>");
        bad("<filterColumn colId=\"0\"><top10 val=\"NaN\"/></filterColumn>");
        bad("<filterColumn colId=\"0\"><dynamicFilter type=\"zz\"/></filterColumn>");
        bad("<filterColumn colId=\"0\"><filters><filter/></filters></filterColumn>");
        bad("<sortState/>");
        bad("<sortState ref=\"A1\"><sortCondition/></sortState>");
        bad("stray text");
    }

    #[test]
    fn writer_rejects_non_finite() {
        let mut f = AutoFilter::default();
        f.filter_column.push(FilterColumn {
            top10: Some(Top10 {
                val: f64::NAN,
                ..Top10::default()
            }),
            ..FilterColumn::default()
        });
        assert!(write_auto_filter(&mut Vec::new(), &f, None).is_err());
    }
}
