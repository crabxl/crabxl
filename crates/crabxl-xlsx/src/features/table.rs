//! Table part (`xl/tables/tableN.xml`) codec.
use super::auto_filter::{
    children, leaf, opt_bool, put_bool, put_num, put_str, read_auto_filter, read_sort_state, uint,
    unknown_attribute, unknown_element, write_auto_filter, write_sort_state,
};
use crate::encode::{write_attribute, write_text};
use crate::metadata::attributes;
use crate::printing::namespace;
use crate::xml::{MAIN_URI, Scope, XmlStream, append_xml_text, check_declaration};
use crabxl_core::Result;
use crabxl_core::features::{
    Table, TableColumn, TableFormula, TableStyleInfo, TableType, TotalsRowFunction, XmlColumnProps,
};
use quick_xml::events::{BytesStart, Event};
use std::io::{self, BufRead, Write};

fn enumerated<T>(value: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T> {
    parse(value).ok_or_else(|| super::invalid("Invalid enumerated attribute"))
}

/// Read a whole table part, starting before the XML declaration.
pub(crate) fn read_part<B: BufRead>(xml: &mut XmlStream<B>) -> Result<Table> {
    let (start, depth) = loop {
        let frame = xml.next()?;
        let depth = frame.depth;
        let scope = frame.scope;
        check_declaration(&frame.event)?;
        match frame.event {
            Event::Start(e) => {
                if scope != Scope::Spreadsheet || e.local_name().as_ref().as_bytes() != b"table" {
                    return Err(super::invalid("Table part root must be <table>"));
                }
                break (e.into_owned(), depth);
            }
            Event::Eof => return Err(super::invalid("Empty table part")),
            _ => {}
        }
    };
    let table = read_table(xml, &start, depth)?;
    loop {
        if matches!(xml.next()?.event, Event::Eof) {
            return Ok(table);
        }
    }
}

fn read_table<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<Table> {
    let mut t = Table::default();
    let (mut has_id, mut has_display, mut has_ref) = (false, false, false);
    attributes(start, |name, v| {
        match name {
            b"id" => {
                t.id = uint(v)?;
                has_id = true;
            }
            b"name" => t.name = Some(v.into()),
            b"displayName" => {
                t.display_name = v.into();
                has_display = true;
            }
            b"comment" => t.comment = Some(v.into()),
            b"ref" => {
                t.ref_range = v.into();
                has_ref = true;
            }
            b"tableType" => t.table_type = Some(enumerated(v, TableType::from_xml)?),
            b"headerRowCount" => t.header_row_count = Some(uint(v)?),
            b"insertRow" => t.insert_row = opt_bool(v)?,
            b"insertRowShift" => t.insert_row_shift = opt_bool(v)?,
            b"totalsRowCount" => t.totals_row_count = Some(uint(v)?),
            b"totalsRowShown" => t.totals_row_shown = opt_bool(v)?,
            b"published" => t.published = opt_bool(v)?,
            b"headerRowDxfId" => t.header_row_dxf_id = Some(uint(v)?),
            b"dataDxfId" => t.data_dxf_id = Some(uint(v)?),
            b"totalsRowDxfId" => t.totals_row_dxf_id = Some(uint(v)?),
            b"headerRowBorderDxfId" => t.header_row_border_dxf_id = Some(uint(v)?),
            b"tableBorderDxfId" => t.table_border_dxf_id = Some(uint(v)?),
            b"totalsRowBorderDxfId" => t.totals_row_border_dxf_id = Some(uint(v)?),
            b"headerRowCellStyle" => t.header_row_cell_style = Some(v.into()),
            b"dataCellStyle" => t.data_cell_style = Some(v.into()),
            b"totalsRowCellStyle" => t.totals_row_cell_style = Some(v.into()),
            b"connectionId" => t.connection_id = Some(uint(v)?),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !(has_id && has_display && has_ref) {
        return Err(super::invalid("Table is missing id, displayName or ref"));
    }
    let mut columns_seen = false;
    children(xml, depth, |xml, name, e, d| {
        match name {
            b"autoFilter" if t.auto_filter.is_none() => {
                t.auto_filter = Some(read_auto_filter(xml, e, d)?)
            }
            b"sortState" if t.sort_state.is_none() => {
                t.sort_state = Some(read_sort_state(xml, e, d)?)
            }
            b"tableColumns" if !columns_seen => {
                columns_seen = true;
                read_columns(xml, e, d, &mut t.table_columns)?
            }
            b"tableStyleInfo" if t.table_style_info.is_none() => {
                t.table_style_info = Some(read_style_info(xml, e, d)?)
            }
            _ => return Err(unknown_element()),
        }
        Ok(())
    })?;
    if !columns_seen {
        return Err(super::invalid("Table is missing tableColumns"));
    }
    Ok(t)
}

fn read_columns<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
    columns: &mut Vec<TableColumn>,
) -> Result<()> {
    attributes(start, |name, _| match name {
        b"count" => Ok(()),
        _ => Err(unknown_attribute()),
    })?;
    children(xml, depth, |xml, name, e, d| match name {
        b"tableColumn" => {
            columns.push(read_column(xml, e, d)?);
            Ok(())
        }
        _ => Err(unknown_element()),
    })
}

fn read_column<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<TableColumn> {
    let mut c = TableColumn::default();
    let (mut has_id, mut has_name) = (false, false);
    attributes(start, |name, v| {
        match name {
            b"id" => {
                c.id = uint(v)?;
                has_id = true;
            }
            b"uniqueName" => c.unique_name = Some(v.into()),
            b"name" => {
                c.name = v.into();
                has_name = true;
            }
            b"totalsRowFunction" => {
                c.totals_row_function = Some(enumerated(v, TotalsRowFunction::from_xml)?)
            }
            b"totalsRowLabel" => c.totals_row_label = Some(v.into()),
            b"queryTableFieldId" => c.query_table_field_id = Some(uint(v)?),
            b"headerRowDxfId" => c.header_row_dxf_id = Some(uint(v)?),
            b"dataDxfId" => c.data_dxf_id = Some(uint(v)?),
            b"totalsRowDxfId" => c.totals_row_dxf_id = Some(uint(v)?),
            b"headerRowCellStyle" => c.header_row_cell_style = Some(v.into()),
            b"dataCellStyle" => c.data_cell_style = Some(v.into()),
            b"totalsRowCellStyle" => c.totals_row_cell_style = Some(v.into()),
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !(has_id && has_name) {
        return Err(super::invalid("Table column is missing id or name"));
    }
    children(xml, depth, |xml, name, e, d| {
        match name {
            b"calculatedColumnFormula" if c.calculated_column_formula.is_none() => {
                c.calculated_column_formula = Some(read_formula(xml, e, d)?)
            }
            b"totalsRowFormula" if c.totals_row_formula.is_none() => {
                c.totals_row_formula = Some(read_formula(xml, e, d)?)
            }
            b"xmlColumnPr" if c.xml_column_pr.is_none() => {
                c.xml_column_pr = Some(read_xml_pr(xml, e, d)?)
            }
            _ => return Err(unknown_element()),
        }
        Ok(())
    })?;
    Ok(c)
}

fn read_formula<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<TableFormula> {
    let mut f = TableFormula::default();
    attributes(start, |name, v| match name {
        b"array" => {
            f.array = opt_bool(v)?;
            Ok(())
        }
        _ => Err(unknown_attribute()),
    })?;
    let mut text = String::new();
    loop {
        let frame = xml.next()?;
        match &frame.event {
            Event::End(_) if frame.depth + 1 == depth => break,
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                append_xml_text(&mut text, &frame.event, 1 << 20)?
            }
            Event::Comment(_) | Event::PI(_) => {}
            Event::Start(_) => return Err(unknown_element()),
            _ => return Err(super::invalid("Unexpected content in formula")),
        }
    }
    f.text = text.into();
    Ok(f)
}

fn read_xml_pr<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<XmlColumnProps> {
    let mut p = XmlColumnProps::default();
    let (mut a, mut b, mut c) = (false, false, false);
    attributes(start, |name, v| {
        match name {
            b"mapId" => {
                p.map_id = uint(v)?;
                a = true;
            }
            b"xpath" => {
                p.xpath = v.into();
                b = true;
            }
            b"denormalized" => p.denormalized = opt_bool(v)?,
            b"xmlDataType" => {
                p.xml_data_type = v.into();
                c = true;
            }
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    if !(a && b && c) {
        return Err(super::invalid("xmlColumnPr is missing attributes"));
    }
    leaf(xml, depth)?;
    Ok(p)
}

fn read_style_info<B: BufRead>(
    xml: &mut XmlStream<B>,
    start: &BytesStart<'_>,
    depth: usize,
) -> Result<TableStyleInfo> {
    let mut s = TableStyleInfo::default();
    attributes(start, |name, v| {
        match name {
            b"name" => s.name = Some(v.into()),
            b"showFirstColumn" => s.show_first_column = opt_bool(v)?,
            b"showLastColumn" => s.show_last_column = opt_bool(v)?,
            b"showRowStripes" => s.show_row_stripes = opt_bool(v)?,
            b"showColumnStripes" => s.show_column_stripes = opt_bool(v)?,
            _ => return Err(unknown_attribute()),
        }
        Ok(())
    })?;
    leaf(xml, depth)?;
    Ok(s)
}

fn write_formula(out: &mut impl Write, name: &str, f: &TableFormula) -> io::Result<()> {
    write!(out, "<{name}")?;
    put_bool(out, "array", f.array)?;
    out.write_all(b">")?;
    write_text(out, &f.text)?;
    write!(out, "</{name}>")
}

/// Write a whole table part. `id` is the workbook-unique table id.
pub(crate) fn write_part(out: &mut impl Write, t: &Table, id: u32) -> io::Result<()> {
    out.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>")?;
    out.write_all(b"<table")?;
    namespace(out, Some(MAIN_URI))?;
    put_num(out, "id", Some(id))?;
    put_str(out, "name", t.name.as_deref())?;
    write_attribute(out, "displayName", &t.display_name)?;
    put_str(out, "comment", t.comment.as_deref())?;
    write_attribute(out, "ref", &t.ref_range)?;
    put_str(out, "tableType", t.table_type.map(TableType::as_xml))?;
    put_num(out, "headerRowCount", t.header_row_count)?;
    put_bool(out, "insertRow", t.insert_row)?;
    put_bool(out, "insertRowShift", t.insert_row_shift)?;
    put_num(out, "totalsRowCount", t.totals_row_count)?;
    put_bool(out, "totalsRowShown", t.totals_row_shown)?;
    put_bool(out, "published", t.published)?;
    put_num(out, "headerRowDxfId", t.header_row_dxf_id)?;
    put_num(out, "dataDxfId", t.data_dxf_id)?;
    put_num(out, "totalsRowDxfId", t.totals_row_dxf_id)?;
    put_num(out, "headerRowBorderDxfId", t.header_row_border_dxf_id)?;
    put_num(out, "tableBorderDxfId", t.table_border_dxf_id)?;
    put_num(out, "totalsRowBorderDxfId", t.totals_row_border_dxf_id)?;
    put_str(
        out,
        "headerRowCellStyle",
        t.header_row_cell_style.as_deref(),
    )?;
    put_str(out, "dataCellStyle", t.data_cell_style.as_deref())?;
    put_str(
        out,
        "totalsRowCellStyle",
        t.totals_row_cell_style.as_deref(),
    )?;
    put_num(out, "connectionId", t.connection_id)?;
    out.write_all(b">")?;
    if let Some(filter) = &t.auto_filter {
        write_auto_filter(out, filter, None)?;
    }
    if let Some(sort) = &t.sort_state {
        write_sort_state(out, sort, None)?;
    }
    write!(out, "<tableColumns count=\"{}\">", t.table_columns.len())?;
    for c in &t.table_columns {
        out.write_all(b"<tableColumn")?;
        put_num(out, "id", Some(c.id))?;
        put_str(out, "uniqueName", c.unique_name.as_deref())?;
        write_attribute(out, "name", &c.name)?;
        put_str(
            out,
            "totalsRowFunction",
            c.totals_row_function.map(TotalsRowFunction::as_xml),
        )?;
        put_str(out, "totalsRowLabel", c.totals_row_label.as_deref())?;
        put_num(out, "queryTableFieldId", c.query_table_field_id)?;
        put_num(out, "headerRowDxfId", c.header_row_dxf_id)?;
        put_num(out, "dataDxfId", c.data_dxf_id)?;
        put_num(out, "totalsRowDxfId", c.totals_row_dxf_id)?;
        put_str(
            out,
            "headerRowCellStyle",
            c.header_row_cell_style.as_deref(),
        )?;
        put_str(out, "dataCellStyle", c.data_cell_style.as_deref())?;
        put_str(
            out,
            "totalsRowCellStyle",
            c.totals_row_cell_style.as_deref(),
        )?;
        if c.calculated_column_formula.is_none()
            && c.totals_row_formula.is_none()
            && c.xml_column_pr.is_none()
        {
            out.write_all(b"/>")?;
            continue;
        }
        out.write_all(b">")?;
        if let Some(f) = &c.calculated_column_formula {
            write_formula(out, "calculatedColumnFormula", f)?;
        }
        if let Some(f) = &c.totals_row_formula {
            write_formula(out, "totalsRowFormula", f)?;
        }
        if let Some(p) = &c.xml_column_pr {
            out.write_all(b"<xmlColumnPr")?;
            put_num(out, "mapId", Some(p.map_id))?;
            write_attribute(out, "xpath", &p.xpath)?;
            put_bool(out, "denormalized", p.denormalized)?;
            write_attribute(out, "xmlDataType", &p.xml_data_type)?;
            out.write_all(b"/>")?;
        }
        out.write_all(b"</tableColumn>")?;
    }
    out.write_all(b"</tableColumns>")?;
    if let Some(s) = &t.table_style_info {
        out.write_all(b"<tableStyleInfo")?;
        put_str(out, "name", s.name.as_deref())?;
        put_bool(out, "showFirstColumn", s.show_first_column)?;
        put_bool(out, "showLastColumn", s.show_last_column)?;
        put_bool(out, "showRowStripes", s.show_row_stripes)?;
        put_bool(out, "showColumnStripes", s.show_column_stripes)?;
        out.write_all(b"/>")?;
    }
    out.write_all(b"</table>")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crabxl_core::{ErrorKind, ResourceLimits};
    use std::io::Cursor;

    fn parse(source: &str) -> Result<Table> {
        let mut xml = XmlStream::new(
            Cursor::new(source.as_bytes()),
            "xl/tables/table1.xml".into(),
            source.len() as u64,
            ResourceLimits::default(),
        );
        read_part(&mut xml)
    }
    fn roundtrip(source: &str) -> Table {
        let table = parse(source).unwrap();
        let mut out = Vec::new();
        write_part(&mut out, &table, table.id).unwrap();
        let text = String::from_utf8(out).unwrap();
        let again = parse(&text).unwrap();
        assert_eq!(table, again, "{text}");
        let mut second = Vec::new();
        write_part(&mut second, &again, again.id).unwrap();
        assert_eq!(text.as_bytes(), second.as_slice());
        table
    }

    const OPENPYXL: &str = r#"<table id="1" name="T1" displayName="T1" ref="A1:C3" headerRowCount="1" xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><autoFilter ref="A1:C3"><filterColumn colId="0" hiddenButton="0" showButton="1"><filters blank="1"><filter val="1"/><filter val="4"/></filters></filterColumn><sortState ref="A1:C3"><sortCondition descending="1" ref="A2:A3"/></sortState></autoFilter><tableColumns count="3"><tableColumn id="1" name="A"/><tableColumn id="2" name="B" totalsRowFunction="sum"/><tableColumn id="3" name="C"><calculatedColumnFormula array="0">A1+B1&lt;2</calculatedColumnFormula></tableColumn></tableColumns><tableStyleInfo name="TableStyleMedium9" showFirstColumn="0" showRowStripes="1"/></table>"#;

    #[test]
    fn openpyxl_sample() {
        let t = roundtrip(OPENPYXL);
        assert_eq!(&*t.display_name, "T1");
        assert_eq!(t.table_columns.len(), 3);
        assert_eq!(
            t.table_columns[1].totals_row_function,
            Some(TotalsRowFunction::Sum)
        );
        let f = t.table_columns[2]
            .calculated_column_formula
            .as_ref()
            .unwrap();
        assert_eq!(&*f.text, "A1+B1<2");
        assert_eq!(f.array, Some(false));
        let af = t.auto_filter.as_ref().unwrap();
        assert_eq!(
            af.filter_column[0].filters.as_ref().unwrap().filter.len(),
            2
        );
        assert_eq!(
            af.sort_state.as_ref().unwrap().sort_condition[0].descending,
            Some(true)
        );
        assert!(t.heap_bytes() > 0);
    }

    #[test]
    fn every_attribute() {
        let src = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="7" name="N &amp; M" displayName="Disp" comment="c&quot;x" ref="A1:B5" tableType="queryTable" headerRowCount="1" insertRow="1" insertRowShift="0" totalsRowCount="1" totalsRowShown="1" published="true" headerRowDxfId="1" dataDxfId="2" totalsRowDxfId="3" headerRowBorderDxfId="4" tableBorderDxfId="5" totalsRowBorderDxfId="6" headerRowCellStyle="h" dataCellStyle="d" totalsRowCellStyle="t" connectionId="9">
<sortState ref="A2:B5" columnSort="1" caseSensitive="0" sortMethod="pinYin"><sortCondition ref="A2:A5" sortBy="icon" customList="a,b" dxfId="2" iconSet="5Quarters" iconId="3"/></sortState>
<tableColumns count="2">
<tableColumn id="1" uniqueName="u" name="a &lt; b" totalsRowFunction="custom" totalsRowLabel="L" queryTableFieldId="4" headerRowDxfId="1" dataDxfId="2" totalsRowDxfId="3" headerRowCellStyle="x" dataCellStyle="y" totalsRowCellStyle="z"><calculatedColumnFormula array="1">SUM(A:A)</calculatedColumnFormula><totalsRowFormula>1+1</totalsRowFormula><xmlColumnPr mapId="1" xpath="/a/b" denormalized="1" xmlDataType="string"/></tableColumn>
<tableColumn id="2" name="b" totalsRowFunction="stdDev"/>
</tableColumns>
<tableStyleInfo name="S" showFirstColumn="1" showLastColumn="1" showRowStripes="0" showColumnStripes="1"/></table>"#;
        let t = roundtrip(src);
        assert_eq!(&*t.name.clone().unwrap(), "N & M");
        assert_eq!(t.table_type, Some(TableType::QueryTable));
        assert_eq!(t.published, Some(true));
        assert!(t.sort_state.is_some());
        let c = &t.table_columns[0];
        assert_eq!(&*c.name, "a < b");
        assert_eq!(c.xml_column_pr.as_ref().unwrap().map_id, 1);
        assert_eq!(&*c.totals_row_formula.as_ref().unwrap().text, "1+1");
    }

    #[test]
    fn write_uses_given_id_and_escapes() {
        let mut t = parse(OPENPYXL).unwrap();
        t.display_name = "a&b".into();
        t.table_columns[0].name = "x\"y<z".into();
        let mut out = Vec::new();
        write_part(&mut out, &t, 42).unwrap();
        let again = parse(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(again.id, 42);
        assert_eq!(&*again.display_name, "a&b");
        assert_eq!(&*again.table_columns[0].name, "x\"y<z");
    }

    #[test]
    fn strict_namespace() {
        let src = OPENPYXL.replace(
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
            "http://purl.oclc.org/ooxml/spreadsheetml/main",
        );
        assert_eq!(parse(&src).unwrap(), parse(OPENPYXL).unwrap());
    }

    #[test]
    fn rejects_unknown_and_incomplete() {
        let unsupported = |src: String| {
            assert_eq!(
                parse(&src).unwrap_err().kind(),
                ErrorKind::Unsupported,
                "{src}"
            )
        };
        unsupported(OPENPYXL.replace("<tableStyleInfo", "<extLst/><tableStyleInfo"));
        unsupported(OPENPYXL.replace("headerRowCount=\"1\"", "bogus=\"1\""));
        unsupported(OPENPYXL.replace(
            "<tableColumn id=\"1\" name=\"A\"/>",
            "<tableColumn id=\"1\" name=\"A\" foo=\"1\"/>",
        ));
        unsupported(OPENPYXL.replace("<filter val=\"1\"/>", "<filter val=\"1\"><x/></filter>"));
        let invalid = |src: String| assert!(parse(&src).is_err(), "{src}");
        invalid(
            OPENPYXL
                .replace(
                    "<tableColumns count=\"3\">",
                    "<tableColumns count=\"3\"><junk/>",
                )
                .replace("<junk/>", "text"),
        );
        invalid(OPENPYXL.replace(" ref=\"A1:C3\" headerRowCount", " headerRowCount"));
        invalid(OPENPYXL.replace("totalsRowFunction=\"sum\"", "totalsRowFunction=\"nope\""));
        invalid(OPENPYXL.replace("id=\"1\" name=\"T1\"", "id=\"-1\" name=\"T1\""));
        invalid(
            "<root xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"/>".into(),
        );
    }
}
