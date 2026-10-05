//! Public-reference default theme and generated bounded typed-theme fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::{ErrorKind, ResourceLimits, Theme, ThemeColor};
use crabxl_xlsx::{ThemeWritePolicy, WorkbookReader, WorkbookWriter, WriteOptions};
use std::io::Cursor;

fn package(theme: ThemeWritePolicy) -> Cursor<Vec<u8>> {
    let mut writer = WorkbookWriter::new(WriteOptions {
        theme,
        ..Default::default()
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap()
}
fn custom(xml: &str) -> Cursor<Vec<u8>> {
    package(ThemeWritePolicy::Custom(Theme::from_bytes(
        xml.as_bytes().to_vec().into_boxed_slice(),
    )))
}

#[test]
fn typed_default_palette_fonts_and_owned_lifetime_match_public_generated_theme() {
    let mut reader = WorkbookReader::new(package(ThemeWritePolicy::ReferenceDefault)).unwrap();
    let raw = reader.theme().unwrap().unwrap().bytes().to_vec();
    let catalog = reader.read_theme_catalog().unwrap().unwrap();
    assert_eq!(catalog.name.as_deref(), Some("Office Theme"));
    assert_eq!(catalog.color_scheme_name.as_deref(), Some("Office"));
    assert_eq!(catalog.font_scheme_name.as_deref(), Some("Office"));
    let expected = [
        0xFFFFFF, 0x000000, 0xEEECE1, 0x1F497D, 0x4F81BD, 0xC0504D, 0x9BBB59, 0x8064A2, 0x4BACC6,
        0xF79646, 0x0000FF, 0x800080,
    ];
    for (color, rgb) in catalog.colors.iter().zip(expected) {
        assert_eq!(color.as_ref().unwrap().rgb(), Some(rgb));
    }
    assert!(
        matches!(&catalog.colors[0], Some(ThemeColor::System { name, .. }) if name.as_ref() == "window")
    );
    assert_eq!(
        catalog
            .major_fonts
            .latin
            .as_ref()
            .unwrap()
            .typeface
            .as_ref(),
        "Cambria"
    );
    assert_eq!(
        catalog
            .minor_fonts
            .latin
            .as_ref()
            .unwrap()
            .typeface
            .as_ref(),
        "Calibri"
    );
    assert_eq!(
        catalog
            .major_fonts
            .east_asian
            .as_ref()
            .unwrap()
            .typeface
            .as_ref(),
        ""
    );
    assert!(
        catalog
            .major_fonts
            .supplemental
            .iter()
            .any(|font| font.script.as_ref() == "Hant" && font.typeface.as_ref() == "新細明體")
    );
    assert_eq!(reader.theme().unwrap().unwrap().bytes(), raw);
    assert_eq!(reader.read_theme_catalog().unwrap().unwrap(), catalog);
    drop(reader);
    assert!(catalog.memory_bytes() > size_of_val(&catalog));
    assert_eq!(catalog.colors[4].as_ref().unwrap().rgb(), Some(0x4F81BD));
    let mut absent = WorkbookReader::new(package(ThemeWritePolicy::Omit)).unwrap();
    assert_eq!(absent.read_theme_catalog().unwrap(), None);
}

#[test]
fn typed_theme_bounds_namespace_identity_and_failures_preserve_opaque_access() {
    for namespace in [
        "http://schemas.openxmlformats.org/drawingml/2006/main",
        "http://purl.oclc.org/ooxml/drawingml/main",
    ] {
        let xml = format!(
            r#"<a:theme xmlns:a="{namespace}" xmlns:x="urn:extension" name="A &amp; B"><a:themeElements><a:clrScheme name="Palette"><a:accent1><a:srgbClr val="aAbBcC"/></a:accent1><a:dk1><a:sysClr val="windowText"/></a:dk1></a:clrScheme><a:fontScheme name="Fonts"><a:majorFont><a:latin typeface="Example" panose="020F0502020204030204" pitchFamily="34" charset="0"/><a:font script="Hant" typeface="&#x65B0;"/><x:font script="Ignored" typeface="Spoof"/></a:majorFont></a:fontScheme><x:clrScheme><a:accent2><a:srgbClr val="FFFFFF"/></a:accent2></x:clrScheme></a:themeElements><a:extLst><x:payload>opaque</x:payload></a:extLst></a:theme>"#
        );
        let mut reader = WorkbookReader::new(custom(&xml)).unwrap();
        let catalog = reader.read_theme_catalog().unwrap().unwrap();
        assert_eq!(catalog.name.as_deref(), Some("A & B"));
        let Some(ThemeColor::Rgb(color)) = &catalog.colors[4] else {
            panic!("Missing palette color")
        };
        assert_eq!(color.to_string(), "00aAbBcC");
        assert_eq!(catalog.colors[1].as_ref().unwrap().rgb(), None);
        assert_eq!(catalog.colors[5], None);
        assert_eq!(catalog.major_fonts.supplemental.len(), 1);
        assert_eq!(catalog.major_fonts.supplemental[0].typeface.as_ref(), "新");
        assert_eq!(
            catalog.major_fonts.latin.as_ref().unwrap().pitch_family,
            Some(34)
        );
        assert_eq!(catalog.major_fonts.latin.as_ref().unwrap().charset, Some(0));
        assert_eq!(reader.theme().unwrap().unwrap().bytes(), xml.as_bytes());
        let limits = ResourceLimits {
            max_style_records: 0,
            ..Default::default()
        };
        let mut limited = WorkbookReader::with_limits(custom(&xml), limits).unwrap();
        assert_eq!(
            limited.read_theme_catalog().unwrap_err().kind(),
            ErrorKind::LimitExceeded
        );
        assert_eq!(limited.theme().unwrap().unwrap().bytes(), xml.as_bytes());
        let limits = ResourceLimits {
            max_theme_bytes: xml.len() + size_of::<Theme>() + 2 * size_of::<usize>(),
            ..Default::default()
        };
        let mut limited = WorkbookReader::with_limits(custom(&xml), limits).unwrap();
        assert_eq!(
            limited.read_theme_catalog().unwrap_err().kind(),
            ErrorKind::MemoryBudgetExceeded
        );
    }
    for (contents, kind) in [
        (
            "<a:accent1><a:srgbClr val=\"xyzxyz\"/></a:accent1>",
            ErrorKind::InvalidData,
        ),
        (
            "<a:accent1><a:srgbClr val=\"FFFFFF\"/></a:accent1><a:accent1/>",
            ErrorKind::InvalidData,
        ),
        (
            "<a:accent1><a:srgbClr val=\"FFFFFF\"><a:shade val=\"50000\"/></a:srgbClr></a:accent1>",
            ErrorKind::Unsupported,
        ),
    ] {
        let xml = format!(
            r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme>{contents}</a:clrScheme></a:themeElements></a:theme>"#
        );
        let mut reader = WorkbookReader::new(custom(&xml)).unwrap();
        let error = reader.read_theme_catalog().unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.part(), Some("xl/theme/theme1.xml"));
        assert_eq!(reader.theme().unwrap().unwrap().bytes(), xml.as_bytes());
    }
}
