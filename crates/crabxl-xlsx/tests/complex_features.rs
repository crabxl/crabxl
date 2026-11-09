//! Acceptance coverage for native drawing, pivot and complex package graphs.
#![allow(clippy::unwrap_used)]

use crabxl_core::{
    BinaryPayload, CellMetadataReadPolicy, CellValue, Chart, ChartKind, DrawingAnchor,
    DrawingExtent, DrawingMarker, Formula, FormulaAnnotations, Image, ImageFormat, ReadOptions,
    ResourceLimits, Shape, SheetFeatures, Workbook, WorkbookFeatures, WorkbookLimits, XmlContent,
    XmlElement,
    drawing::PartRelationship,
    features::{
        ExternalLink, MacroPolicy, PivotCache, PivotTable, PivotValue, VbaProject, WorkbookFormat,
        WorkbookMetadata,
    },
};
use crabxl_xlsx::{
    LoadOptions, LoadedWorkbook, PivotRecords, SaveOptions, WorkbookReader, WorkbookWriter,
    WriteOptions, write_pivot_records,
};
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    sync::Arc,
};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PNG: &[u8] = include_bytes!("fixtures/pixel.png");
const DRAWINGS: &[u8] = include_bytes!("fixtures/drawings.xlsx");

fn node(name: &str, attributes: &[(&str, &str)], children: Vec<XmlElement>) -> XmlElement {
    let mut element = XmlElement::new(name);
    for (name, value) in attributes {
        element.set_attribute(*name, *value);
    }
    element.children = children.into_iter().map(XmlContent::Element).collect();
    element
}

fn anchor(row: u32) -> DrawingAnchor {
    DrawingAnchor::OneCell {
        from: DrawingMarker::new(5, row).unwrap(),
        extent: DrawingExtent::pixels(300, 200),
    }
}

fn model(features: SheetFeatures) -> Workbook {
    let mut book = Workbook::new(WorkbookLimits::default()).unwrap();
    let id = book.create_sheet("Data").unwrap();
    let mut sheet = book.sheet_mut(id).unwrap();
    for row in [
        vec![CellValue::text("Category"), CellValue::text("Value")],
        vec![CellValue::text("A"), CellValue::Integer(4)],
        vec![CellValue::text("B"), CellValue::Integer(7)],
    ] {
        sheet.append(row).unwrap();
    }
    sheet.set_features(Some(features)).unwrap();
    drop(sheet);
    book
}

fn output(book: &Workbook) -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_workbook(book).unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap().into_inner()
}

fn save(book: &mut LoadedWorkbook<Cursor<Vec<u8>>>) -> Vec<u8> {
    book.save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner()
}

fn artifact(name: &str, bytes: &[u8]) {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/acceptance-output");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(name), bytes).unwrap();
}

fn parts(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut file = zip.by_index(index).unwrap();
            let name = file.name().to_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

fn graph_is_complete(bytes: &[u8]) {
    let parts = parts(bytes);
    let mut relationship_ids = HashMap::new();
    let mut references = Vec::new();
    for (name, bytes) in &parts {
        if !name.ends_with(".xml") && !name.ends_with(".rels") {
            continue;
        }
        let mut xml = quick_xml::Reader::from_reader(bytes.as_slice());
        let mut ids = HashSet::new();
        loop {
            use quick_xml::events::Event;
            match xml.read_event().unwrap() {
                Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "Relationship" => {
                    let attributes: HashMap<_, _> = e
                        .attributes()
                        .map(|attribute| {
                            let attribute = attribute.unwrap();
                            (
                                attribute.key.as_ref().to_owned(),
                                attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .unwrap()
                                    .into_owned(),
                            )
                        })
                        .collect();
                    assert!(
                        ids.insert(attributes["Id"].clone()),
                        "duplicate relationship in {name}"
                    );
                    if attributes
                        .get("TargetMode")
                        .is_some_and(|mode| mode == "External")
                    {
                        continue;
                    }
                    let owner = if name == "_rels/.rels" {
                        String::new()
                    } else {
                        name.replace("/_rels/", "/")
                            .trim_end_matches(".rels")
                            .to_owned()
                    };
                    let target = &attributes["Target"];
                    let path = if target.starts_with('/') {
                        target.trim_start_matches('/').to_owned()
                    } else {
                        format!(
                            "{}{target}",
                            owner
                                .rsplit_once('/')
                                .map_or("", |(folder, _)| &owner[..folder.len() + 1])
                        )
                    };
                    let mut components = Vec::new();
                    for component in path.split('/') {
                        match component {
                            ".." => {
                                components.pop();
                            }
                            "." | "" => {}
                            _ => components.push(component),
                        }
                    }
                    let target = components.join("/");
                    assert!(
                        parts.contains_key(&target),
                        "{name} points to missing {target}"
                    );
                }
                Event::Start(e) | Event::Empty(e) => {
                    for attribute in e.attributes() {
                        let attribute = attribute.unwrap();
                        let key = attribute.key.as_ref();
                        if key.contains(':')
                            && !key.starts_with("xmlns:")
                            && matches!(key.rsplit(':').next(), Some("id" | "embed" | "link"))
                        {
                            let owner = name.rsplit_once('/').map_or_else(
                                || format!("_rels/{name}.rels"),
                                |(folder, file)| format!("{folder}/_rels/{file}.rels"),
                            );
                            references.push((
                                owner,
                                attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .unwrap()
                                    .into_owned(),
                            ));
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if name.ends_with(".rels") {
            relationship_ids.insert(name.clone(), ids);
        }
    }
    for (owner, id) in references {
        assert!(
            relationship_ids
                .get(&owner)
                .is_some_and(|ids| ids.contains(&id)),
            "{owner} has no relationship {id}"
        );
    }
}

#[test]
fn openpyxl_drawings_survive_loaded_edits_copies_removals_and_repeat_saves() {
    let mut source = WorkbookReader::new(Cursor::new(DRAWINGS)).unwrap();
    assert_eq!(source.sheet_images("Data").unwrap().len(), 1);
    assert_eq!(
        source.sheet_images("Data").unwrap()[0]
            .drawing_parts
            .relationships[0]
            .target
            .as_ref(),
        "https://example.invalid/image"
    );
    assert_eq!(source.sheet_charts("Data").unwrap().len(), 16);
    assert_eq!(source.sheet_charts("Chart Only").unwrap().len(), 1);
    let mut book =
        LoadedWorkbook::with_options(Cursor::new(DRAWINGS.to_vec()), LoadOptions::default())
            .unwrap();
    let data = book.sheet_id("Data").unwrap();
    let mut images = book.sheet_images(data).unwrap().to_vec();
    images[0].anchor = anchor(12);
    book.set_sheet_images(data, images).unwrap();
    let mut charts = book.sheet_charts(data).unwrap().to_vec();
    charts[0].name = "Edited chart".into();
    book.set_sheet_charts(data, charts).unwrap();
    let copy = book.copy_sheet(data, "Copied").unwrap();
    let chart_only = book.sheet_id("Chart Only").unwrap();
    let chart_copy = book.copy_sheet(chart_only, "Copied Chart").unwrap();
    for _ in 0..2 {
        let bytes = save(&mut book);
        graph_is_complete(&bytes);
        let mut reader = WorkbookReader::new(Cursor::new(bytes.clone())).unwrap();
        for name in ["Data", "Copied"] {
            assert_eq!(reader.sheet_charts(name).unwrap().len(), 16);
            assert_eq!(reader.sheet_images(name).unwrap()[0].anchor, anchor(12));
        }
        assert_eq!(reader.sheet_charts("Copied Chart").unwrap().len(), 1);
        for name in ["Chart Only", "Copied Chart"] {
            let features = reader.sheet_features(name).unwrap();
            let graph = features.chartsheet_parts.unwrap();
            assert_eq!(graph.relationships.len(), 1);
            assert_eq!(
                graph.parts[0].data,
                BinaryPayload::from(b"opaque printer settings".as_slice())
            );
        }
        assert_eq!(
            reader.sheet_images("Copied").unwrap()[0]
                .drawing_parts
                .relationships[0]
                .target
                .as_ref(),
            "https://example.invalid/image"
        );
        assert_eq!(
            reader.sheet_charts("Copied").unwrap()[0]
                .drawing_parts
                .relationships[0]
                .target
                .as_ref(),
            "https://example.invalid/chart"
        );
        artifact("openpyxl-drawings-edited.xlsx", &bytes);
    }
    book.remove_sheet(copy).unwrap();
    book.remove_sheet(chart_copy).unwrap();
    let bytes = save(&mut book);
    graph_is_complete(&bytes);
    assert_eq!(
        WorkbookReader::new(Cursor::new(bytes))
            .unwrap()
            .sheet_charts("Data")
            .unwrap()
            .len(),
        16
    );
}

#[test]
fn native_chart_families_images_shapes_and_connectors_have_valid_graphs() {
    let kinds = [
        ChartKind::Bar,
        ChartKind::Bar3D,
        ChartKind::Line,
        ChartKind::Line3D,
        ChartKind::Area,
        ChartKind::Area3D,
        ChartKind::Pie,
        ChartKind::Pie3D,
        ChartKind::Doughnut,
        ChartKind::OfPie,
        ChartKind::Scatter,
        ChartKind::Bubble,
        ChartKind::Radar,
        ChartKind::Stock,
        ChartKind::Surface,
        ChartKind::Surface3D,
    ];
    let charts: Vec<_> = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let mut chart = Chart::new(kind, anchor(index as u32 * 16)).unwrap();
            let xy = matches!(kind, ChartKind::Scatter | ChartKind::Bubble);
            chart
                .add_series(
                    0,
                    "Data!$B$2:$B$3",
                    Some(if xy {
                        "Data!$B$2:$B$3"
                    } else {
                        "Data!$A$2:$A$3"
                    }),
                    Some("Values"),
                    (kind == ChartKind::Bubble).then_some("Data!$B$2:$B$3"),
                )
                .unwrap();
            if kind == ChartKind::Doughnut {
                assert_eq!(
                    chart
                        .space
                        .child("chart")
                        .unwrap()
                        .child("plotArea")
                        .unwrap()
                        .child("doughnutChart")
                        .unwrap()
                        .child("holeSize")
                        .unwrap()
                        .attribute("val"),
                    Some("10"),
                );
            }
            chart
        })
        .collect();
    let shape = node(
        "xdr:sp",
        &[
            (
                "xmlns:xdr",
                "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing",
            ),
            (
                "xmlns:a",
                "http://schemas.openxmlformats.org/drawingml/2006/main",
            ),
        ],
        vec![
            node(
                "xdr:nvSpPr",
                &[],
                vec![
                    node("xdr:cNvPr", &[("id", "50"), ("name", "Rectangle")], vec![]),
                    node("xdr:cNvSpPr", &[], vec![]),
                ],
            ),
            node(
                "xdr:spPr",
                &[],
                vec![node(
                    "a:prstGeom",
                    &[("prst", "rect")],
                    vec![node("a:avLst", &[], vec![])],
                )],
            ),
        ],
    );
    let connector = node(
        "xdr:cxnSp",
        &[
            (
                "xmlns:xdr",
                "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing",
            ),
            (
                "xmlns:a",
                "http://schemas.openxmlformats.org/drawingml/2006/main",
            ),
        ],
        vec![
            node(
                "xdr:nvCxnSpPr",
                &[],
                vec![
                    node("xdr:cNvPr", &[("id", "51"), ("name", "Connector")], vec![]),
                    node(
                        "xdr:cNvCxnSpPr",
                        &[],
                        vec![node("a:stCxn", &[("id", "50"), ("idx", "0")], vec![])],
                    ),
                ],
            ),
            node(
                "xdr:spPr",
                &[],
                vec![node(
                    "a:prstGeom",
                    &[("prst", "line")],
                    vec![node("a:avLst", &[], vec![])],
                )],
            ),
        ],
    );
    let features = SheetFeatures {
        images: Some(vec![Image::new(PNG, ImageFormat::Png, anchor(3)).unwrap()]),
        charts: Some(charts),
        shapes: Some(vec![
            Shape::new(anchor(4), shape).unwrap(),
            Shape::new(anchor(5), connector).unwrap(),
        ]),
        ..Default::default()
    };
    let bytes = output(&model(features));
    graph_is_complete(&bytes);
    let mut reader = WorkbookReader::new(Cursor::new(bytes.clone())).unwrap();
    assert_eq!(
        reader
            .sheet_charts("Data")
            .unwrap()
            .iter()
            .flat_map(|chart| chart.kinds())
            .collect::<Vec<_>>(),
        kinds
    );
    let shapes = reader.sheet_shapes("Data").unwrap();
    let identity = shapes[0]
        .properties
        .child("nvSpPr")
        .unwrap()
        .child("cNvPr")
        .unwrap()
        .attribute("id")
        .unwrap();
    assert_eq!(
        shapes[1]
            .properties
            .child("nvCxnSpPr")
            .unwrap()
            .child("cNvCxnSpPr")
            .unwrap()
            .child("stCxn")
            .unwrap()
            .attribute("id"),
        Some(identity)
    );
    assert_eq!(
        reader.sheet_images("Data").unwrap()[0].data,
        BinaryPayload::from(PNG)
    );
    artifact("native-drawings.xlsx", &bytes);
}

#[test]
fn pivot_records_and_caches_survive_layout_edits_and_sheet_copies() {
    let mut cache =
        PivotCache::worksheet(7, "Data", "A1:B3".parse().unwrap(), &["Category", "Value"]).unwrap();
    let records = vec![
        vec![PivotValue::Text("A".into()), PivotValue::Number(4.0)],
        vec![PivotValue::Text("B".into()), PivotValue::Number(7.0)],
    ];
    let (payload, count) = write_pivot_records(records.clone(), ResourceLimits::default()).unwrap();
    cache.set_records(payload, count);
    let table = PivotTable::new("Summary", "E1:F4".parse().unwrap(), Arc::new(cache)).unwrap();
    let bytes = output(&model(SheetFeatures {
        pivots: Some(vec![table]),
        ..Default::default()
    }));
    let mut book =
        LoadedWorkbook::with_options(Cursor::new(bytes), LoadOptions::default()).unwrap();
    let id = book.sheet_id("Data").unwrap();
    let mut tables = book.sheet_pivots(id).unwrap().to_vec();
    tables[0].definition.set_attribute("name", "EditedSummary");
    book.set_sheet_pivots(id, tables).unwrap();
    let copy = book.copy_sheet(id, "Snapshot").unwrap();
    for _ in 0..2 {
        let bytes = save(&mut book);
        graph_is_complete(&bytes);
        let mut reader = WorkbookReader::new(Cursor::new(bytes.clone())).unwrap();
        for name in ["Data", "Snapshot"] {
            let tables = reader.sheet_pivots(name).unwrap();
            let mut source =
                PivotRecords::new(&tables[0].cache, ResourceLimits::default()).unwrap();
            for row in &records {
                assert_eq!(source.next_record().unwrap().as_ref(), Some(row));
            }
            assert!(source.next_record().unwrap().is_none());
        }
        artifact("native-pivots.xlsx", &bytes);
    }
    book.remove_sheet(copy).unwrap();
    graph_is_complete(&save(&mut book));
}

#[test]
fn complex_graphs_metadata_indices_formats_and_vba_policies_are_checked() {
    let metadata = node(
        "metadata",
        &[("xmlns", MAIN)],
        vec![
            node(
                "metadataTypes",
                &[("count", "1")],
                vec![node(
                    "metadataType",
                    &[("name", "Acceptance"), ("minSupportedVersion", "120000")],
                    vec![],
                )],
            ),
            node(
                "futureMetadata",
                &[("name", "Acceptance"), ("count", "1")],
                vec![node("bk", &[], vec![])],
            ),
            node(
                "cellMetadata",
                &[("count", "1")],
                vec![node(
                    "bk",
                    &[],
                    vec![node("rc", &[("t", "1"), ("v", "0")], vec![])],
                )],
            ),
        ],
    );
    let external = node(
        "externalLink",
        &[("xmlns", MAIN), ("xmlns:r", REL)],
        vec![node(
            "externalBook",
            &[("r:id", "file")],
            vec![node(
                "sheetNames",
                &[],
                vec![node("sheetName", &[("val", "Remote")], vec![])],
            )],
        )],
    );
    let mut book = model(SheetFeatures::default());
    let id = book.sheets().next().unwrap().0;
    book.sheet_mut(id)
        .unwrap()
        .set(crabxl_core::Cell {
            address: "A1".parse().unwrap(),
            style: crabxl_core::StyleId::new(0),
            value: CellValue::Integer(8).with_annotations(FormulaAnnotations {
                cell_metadata: Some("1".into()),
                value_metadata: None,
            }),
        })
        .unwrap();
    book.sheet_mut(id)
        .unwrap()
        .set(crabxl_core::Cell {
            address: "C1".parse().unwrap(),
            style: crabxl_core::StyleId::new(0),
            value: CellValue::Formula(Box::new(
                Formula::new("1+1", Some(CellValue::Integer(2))).unwrap(),
            )),
        })
        .unwrap();
    book.set_features(Some(WorkbookFeatures {
        format: Some(WorkbookFormat::Xlsm),
        macros: MacroPolicy::Replace(VbaProject {
            data: BinaryPayload::from(b"opaque VBA payload".as_slice()),
            source_part: None,
            relationships: vec![],
            parts: vec![],
        }),
        external_links: Some(vec![ExternalLink {
            definition: external,
            source_part: None,
            relationships: vec![PartRelationship {
                id: "file".into(),
                kind: format!("{REL}/externalLinkPath").into(),
                target: "file:///C:/remote.xlsx".into(),
                external: true,
            }],
            parts: vec![],
        }]),
        metadata: Some(vec![WorkbookMetadata {
            kind: format!("{REL}/sheetMetadata").into(),
            content_type:
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"
                    .into(),
            definition: metadata,
            source_part: None,
            relationships: vec![],
            parts: vec![],
        }]),
        ..Default::default()
    }));
    let bytes = output(&book);
    graph_is_complete(&bytes);
    let mut loaded =
        LoadedWorkbook::with_options(Cursor::new(bytes.clone()), LoadOptions::default()).unwrap();
    assert_eq!(
        loaded.vba_project().unwrap().unwrap().data,
        BinaryPayload::from(b"opaque VBA payload".as_slice())
    );
    loaded.set_workbook_metadata(Vec::new()).unwrap();
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("protected.xlsx");
    std::fs::write(&path, b"keep").unwrap();
    assert!(loaded.save_path(&path, SaveOptions::default()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    let data = loaded.sheet_id("Data").unwrap();
    loaded
        .set_value(data, "A1".parse().unwrap(), CellValue::Integer(9))
        .unwrap();
    loaded.set_macro_policy(MacroPolicy::Remove).unwrap();
    loaded.set_workbook_format(WorkbookFormat::Xltx).unwrap();
    let result = save(&mut loaded);
    graph_is_complete(&result);
    let mut reader = WorkbookReader::new(Cursor::new(result.clone())).unwrap();
    assert_eq!(reader.workbook_format(), WorkbookFormat::Xltx);
    assert!(reader.vba_project().unwrap().is_none());
    assert_eq!(reader.external_links().unwrap().len(), 1);
    assert!(reader.workbook_metadata().unwrap().is_empty());
    artifact("native-template.xltx", &result);
    let mut data_only = LoadedWorkbook::with_options(
        Cursor::new(bytes),
        LoadOptions {
            read: ReadOptions {
                data_only: true,
                cell_metadata_policy: CellMetadataReadPolicy::RetainAllReferences,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let data = data_only.sheet_id("Data").unwrap();
    data_only
        .set_value(data, "B1".parse().unwrap(), CellValue::Integer(12))
        .unwrap();
    let bytes = save(&mut data_only);
    assert!(
        !String::from_utf8(parts(&bytes)["xl/worksheets/sheet1.xml"].clone())
            .unwrap()
            .contains("<f>")
    );
    let values = WorkbookReader::new(Cursor::new(bytes))
        .unwrap()
        .read_sheet("Data")
        .unwrap();
    assert_eq!(values.rows[0].cells[2].value, CellValue::Integer(2));
}

#[test]
fn disk_payload_readers_keep_independent_positions_and_record_buffers_are_bounded() {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"0123456789").unwrap();
    let data = BinaryPayload::from_file(file).unwrap();
    let mut first = data.reader();
    let mut second = data.reader();
    let mut bytes = [0; 3];
    first.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"012");
    second.seek(SeekFrom::Start(7)).unwrap();
    second.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"789");
    first.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"345");
    assert!(
        write_pivot_records(
            [vec![PivotValue::Text("x".repeat(1000).into())]],
            ResourceLimits {
                max_row_bytes: 128,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        write_pivot_records(
            [vec![PivotValue::Number(f64::NAN)]],
            ResourceLimits::default()
        )
        .is_err()
    );
}

#[test]
fn streamed_hyperlinks_merge_with_feature_footers_and_relationships() {
    for live in [false, true] {
        let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
        let features = SheetFeatures {
            images: Some(vec![Image::new(PNG, ImageFormat::Png, anchor(4)).unwrap()]),
            comments: vec![crabxl_core::features::Comment::new(
                "A1".parse().unwrap(),
                "A note",
                "Author",
            )],
            printer_settings: Some(crabxl_core::features::PrinterSettings {
                relationship: "device".into(),
                data: BinaryPayload::from(b"printer payload".as_slice()),
                source_part: None,
                relationships: vec![],
                parts: vec![],
            }),
            ..Default::default()
        };
        let mut sheet = crabxl_core::Worksheet::new("Data", Default::default()).unwrap();
        sheet.set_features(Some(features)).unwrap();
        let mut printing = crabxl_core::PrintSettings::default();
        printing.setup.printer_relationship = Some("device".into());
        sheet.set_print_settings(Some(printing)).unwrap();
        sheet
            .set_hyperlink(
                "A1".parse().unwrap(),
                Some(crabxl_core::Hyperlink::external(
                    "https://example.invalid/static",
                )),
            )
            .unwrap();
        writer
            .start_sheet_with_metadata(
                sheet.name(),
                None,
                sheet.print_settings(),
                sheet.hyperlinks(),
                sheet.features(),
            )
            .unwrap();
        let address = "A2".parse().unwrap();
        let mut row = crabxl_core::Row::new(crabxl_core::RowIndex::new(1).unwrap());
        row.cells.push(crabxl_core::Cell {
            address,
            value: CellValue::text("Link"),
            style: crabxl_core::StyleId::new(0),
        });
        let link = crabxl_core::Hyperlink::external("https://example.invalid/first");
        if live {
            let group = writer.register_hyperlink_group(&link).unwrap();
            writer
                .write_row_with_hyperlink_groups(&row, &[(address, group)])
                .unwrap();
            writer
                .update_hyperlink_group(
                    group,
                    &crabxl_core::Hyperlink::external("https://example.invalid/final"),
                )
                .unwrap();
        } else {
            writer
                .write_row_with_hyperlinks(&row, &[(address, link)])
                .unwrap();
        }
        let bytes = writer.finish(Cursor::new(Vec::new())).unwrap().into_inner();
        graph_is_complete(&bytes);
        let mut reader = WorkbookReader::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(reader.sheet_images("Data").unwrap().len(), 1);
        assert_eq!(reader.sheet_features("Data").unwrap().comments.len(), 1);
        assert_eq!(
            reader
                .sheet_features("Data")
                .unwrap()
                .printer_settings
                .unwrap()
                .data,
            BinaryPayload::from(b"printer payload".as_slice())
        );
        assert_eq!(
            reader
                .hyperlinks("Data")
                .unwrap()
                .get("A1".parse().unwrap())
                .unwrap()
                .target
                .as_deref(),
            Some("https://example.invalid/static")
        );
        let link = reader
            .hyperlinks("Data")
            .unwrap()
            .get(address)
            .unwrap()
            .clone();
        assert_eq!(
            link.target.as_deref(),
            Some(if live {
                "https://example.invalid/final"
            } else {
                "https://example.invalid/first"
            })
        );
        artifact(
            if live {
                "live-links.xlsx"
            } else {
                "snapshot-links.xlsx"
            },
            &bytes,
        );
    }
}

#[test]
fn malformed_pivot_records_reject_counts_widths_and_shared_indices() {
    for body in [
        "<pivotCacheRecords count=\"2\"><r><s v=\"A\"/><n v=\"4\"/></r></pivotCacheRecords>",
        "<pivotCacheRecords count=\"1\"><r><s v=\"A\"/></r></pivotCacheRecords>",
        "<pivotCacheRecords count=\"1\"><r><x v=\"0\"/><n v=\"4\"/></r></pivotCacheRecords>",
        "<pivotCacheRecords count=\"1\"><r><s v=\"A\"/><n v=\"NaN\"/></r></pivotCacheRecords>",
    ] {
        let mut cache =
            PivotCache::worksheet(0, "Data", "A1:B2".parse().unwrap(), &["Category", "Value"])
                .unwrap();
        let xml = body.replacen(
            "<pivotCacheRecords",
            &format!("<pivotCacheRecords xmlns=\"{MAIN}\""),
            1,
        );
        cache.set_records(BinaryPayload::from(xml.into_bytes()), 1);
        let mut rows = PivotRecords::new(&cache, ResourceLimits::default()).unwrap();
        let mut rejected = false;
        loop {
            match rows.next_record() {
                Err(_) => {
                    rejected = true;
                    break;
                }
                Ok(None) => break,
                Ok(Some(_)) => {}
            }
        }
        assert!(rejected, "accepted malformed records: {body}");
    }
}
