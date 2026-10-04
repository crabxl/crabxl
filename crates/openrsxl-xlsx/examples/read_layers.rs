//! Cumulative layer probe for the generated numeric benchmark input only.
//! XML-only mode is diagnostic and does not implement spreadsheet validation.
use quick_xml::{NsReader, events::Event};
use std::{fs::File, io::BufReader};
use zip::ZipArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let path = arguments
        .next()
        .ok_or("Usage: read_layers <xlsx> <zip|xml>")?;
    let mode = arguments.next().ok_or("Missing layer")?;
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let entry = archive.by_name("xl/worksheets/sheet1.xml")?;
    match mode.as_str() {
        "zip" => {
            let bytes = std::io::copy(&mut BufReader::new(entry), &mut std::io::sink())?;
            println!("{bytes}");
        }
        "xml" => {
            let mut xml = NsReader::from_reader(BufReader::new(entry));
            xml.config_mut().expand_empty_elements = true;
            let mut buffer = Vec::new();
            let mut cells = 0;
            loop {
                buffer.clear();
                match xml.read_resolved_event_into(&mut buffer)?.1 {
                    Event::Start(e) if e.local_name().as_ref() == b"c" => cells += 1,
                    Event::Eof => break,
                    _ => {}
                }
            }
            println!("{cells}");
        }
        _ => return Err("Layer must be zip or xml".into()),
    }
    Ok(())
}
