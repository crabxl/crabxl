//! Standalone rich-value XML utilities over the worksheet/shared-string codec.
use crate::{rich_text, xml};
use crabxl_core::{Error, ErrorKind, ResourceLimits, Result, RichText, RichTextRun};
use quick_xml::events::Event;
use std::io::{self, BufRead, Write};

/// Decode one namespaced `is` or `si` element with explicit resource limits.
/// Complete input consumption rejects extra roots and malformed trailing XML.
pub fn read_rich_text(input: impl BufRead, limits: ResourceLimits) -> Result<RichText> {
    let mut stream =
        xml::XmlStream::new(input, "rich-text.xml".into(), limits.max_part_bytes, limits);
    let mut value = None;
    loop {
        let frame = stream.next()?;
        xml::check_declaration(&frame.event)?;
        match frame.event {
            Event::Start(element)
                if value.is_none()
                    && frame.depth == 1
                    && frame.scope == xml::Scope::Spreadsheet =>
            {
                let name = element.local_name();
                let name = name.as_ref().as_bytes();
                let closing: &[u8] = match name {
                    b"is" => b"is",
                    b"si" => b"si",
                    _ => return Err(invalid("Expected an inline or shared rich-text root")),
                };
                let parsed = rich_text::read_container(
                    &mut stream,
                    1,
                    closing,
                    limits.max_cell_bytes,
                    true,
                )?;
                value = Some(match parsed {
                    rich_text::ParsedText::Rich(value) => *value,
                    rich_text::ParsedText::Plain(text) => RichText {
                        runs: if text.is_empty() {
                            Vec::new()
                        } else {
                            vec![RichTextRun { text, font: None }]
                        },
                        ..RichText::default()
                    },
                });
            }
            Event::Text(text) if text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
            Event::Comment(_) | Event::PI(_) | Event::Decl(_) => {}
            Event::Eof => {
                let value = value.ok_or_else(|| invalid("Missing rich-text root"))?;
                rich_text::validate(&value, limits.max_cell_bytes)?;
                return Ok(value);
            }
            _ => return Err(invalid("Unexpected standalone rich-text XML content")),
        }
    }
}

/// Write a namespaced inline rich value using the canonical format codec.
pub fn write_rich_text(
    output: &mut impl Write,
    value: &RichText,
    limits: ResourceLimits,
) -> Result<()> {
    rich_text::validate(value, limits.max_cell_bytes)?;
    let mut bounded = BoundedOutput {
        output,
        remaining: limits.max_part_bytes,
    };
    rich_text::write_namespaced_container(&mut bounded, value).map_err(|cause| {
        Error::caused_by(
            ErrorKind::Io,
            "Cannot write standalone rich-text XML",
            cause,
        )
    })
}

struct BoundedOutput<'a, W> {
    output: &'a mut W,
    remaining: u64,
}
impl<W: Write> Write for BoundedOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other(
                "Rich-text XML exceeds configured part limit",
            ));
        }
        let count = self.output.write(bytes)?;
        self.remaining -= count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message).with_part("rich-text.xml")
}
