//! Bounded identities and relationship preservation for source hyperlink edits.
use super::*;
use crate::{
    WorkbookReader, package,
    xml::{Scope, XmlStream},
};
use quick_xml::{Writer, events::Event};
use std::io::{BufReader, Read, Seek};

pub(crate) struct Identity {
    pub(crate) value: Option<Box<str>>,
    added: bool,
}
pub(crate) struct Plan {
    pub(crate) ids: Vec<Identity>,
    pub(crate) relationships: Option<Vec<u8>>,
}
impl Plan {
    pub(crate) fn heap_bytes(&self) -> usize {
        self.ids
            .capacity()
            .saturating_mul(size_of::<Identity>())
            .saturating_add(
                self.ids
                    .iter()
                    .map(|id| id.value.as_ref().map_or(0, |value| value.len()))
                    .sum::<usize>(),
            )
            .saturating_add(self.relationships.as_ref().map_or(0, Vec::capacity))
    }
    pub(crate) fn write_links(
        &self,
        output: &mut impl Write,
        links: &Hyperlinks,
        uri: Option<&str>,
    ) -> io::Result<()> {
        write_links_with_ids(output, links, uri, |index| {
            self.ids
                .get(index)
                .and_then(|id| id.value.as_deref())
                .map(Cow::Borrowed)
        })
    }
}
fn output_error(message: &'static str, cause: io::Error) -> Error {
    Error::caused_by(
        if cause.kind() == io::ErrorKind::FileTooLarge {
            ErrorKind::MemoryBudgetExceeded
        } else {
            ErrorKind::Io
        },
        message,
        cause,
    )
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Source hyperlink graph exceeds save allowance",
    )
}

pub(crate) fn prepare<R: Read + Seek>(
    book: &mut WorkbookReader<R>,
    part: &str,
    links: &Hyperlinks,
    maximum: usize,
) -> Result<Plan> {
    validate(links)?;
    let rels_part = package::relationship_part(part);
    let present = book.archive.index_for_name(&rels_part).is_some();
    let mut remaining = maximum as u64;
    let relationships = if present && links.iter().any(|(_, link)| link.target.is_some()) {
        package::read_relationships(&mut book.archive, &rels_part, book.limits, &mut remaining)?
    } else {
        Default::default()
    };
    let mut plan = Plan {
        ids: Vec::new(),
        relationships: None,
    };
    if links.len().saturating_mul(size_of::<Identity>()) > remaining.min(usize::MAX as u64) as usize
    {
        return Err(budget());
    }
    plan.ids.try_reserve_exact(links.len()).map_err(|cause| {
        Error::caused_by(
            ErrorKind::MemoryBudgetExceeded,
            "Cannot reserve source hyperlink identities",
            cause,
        )
    })?;
    let mut charged = plan.heap_bytes();
    let mut next = 1_u64;
    for (_, link) in links.iter() {
        let (value, added) = if let Some(target) = &link.target {
            if let Some(id) = link.relationship_id.as_deref().filter(|id| {
                relationships.get(*id).is_some_and(|relation| {
                    package::relationship_is(&relation.kind, "hyperlink")
                        && relation.external
                        && relation.target.as_str() == target.as_ref()
                })
            }) {
                (Some(id.into()), false)
            } else {
                let id = loop {
                    let id = format!("rIdCrabXL{next}");
                    next = next.checked_add(1).ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidData,
                            "Source hyperlink identity space exhausted",
                        )
                    })?;
                    if !relationships.contains_key(&id) {
                        break id.into_boxed_str();
                    }
                };
                (Some(id), true)
            }
        } else {
            (None, false)
        };
        charged = charged.saturating_add(value.as_ref().map_or(0, |value| value.len()));
        if charged > remaining.min(usize::MAX as u64) as usize {
            return Err(budget());
        }
        plan.ids.push(Identity { value, added });
    }
    let added = plan.ids.iter().filter(|id| id.added).count();
    if added == 0 {
        return Ok(plan);
    }
    if relationships.len().saturating_add(added) > book.limits.max_archive_entries {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "Hyperlink relationship count exceeds configured limit",
        ));
    }
    drop(relationships);
    let limit = maximum;
    let maximum = maximum.saturating_sub(plan.heap_bytes());
    let mut output = RowBuffer {
        data: Vec::new(),
        maximum,
    };
    if present {
        let file = book.archive.by_name(&rels_part).map_err(|cause| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot reopen hyperlink relationship part",
                cause,
            )
            .with_part(&rels_part)
        })?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(book.limits.input_buffer_bytes, file),
            rels_part.clone(),
            book.limits.max_part_bytes,
            book.limits,
        );
        let mut writer = Writer::new(&mut output);
        loop {
            let frame = xml.next()?;
            crate::xml::check_declaration(&frame.event)
                .map_err(|error| error.with_part(&rels_part))?;
            if let Event::End(e) = &frame.event
                && frame.depth == 0
                && frame.scope == Scope::Relationships
                && e.local_name().as_ref().as_bytes() == b"Relationships"
            {
                write_added(writer.get_mut(), links, &plan)
                    .map_err(|cause| output_error("Cannot add hyperlink relationships", cause))?;
            }
            if matches!(frame.event, Event::Eof) {
                break;
            }
            writer.write_event(frame.event).map_err(|cause| {
                output_error("Cannot preserve source hyperlink relationships", cause)
                    .with_part(&rels_part)
            })?;
        }
    } else {
        (|| -> io::Result<()> {
            output.write_all(RELATIONSHIPS_HEADER)?;
            write_added(&mut output, links, &plan)?;
            output.write_all(b"</Relationships>")
        })()
        .map_err(|cause| output_error("Cannot create hyperlink relationships", cause))?;
    }
    plan.relationships = Some(output.data);
    if plan.heap_bytes() > limit {
        return Err(budget());
    }
    Ok(plan)
}
fn write_added(output: &mut impl Write, links: &Hyperlinks, plan: &Plan) -> io::Result<()> {
    for ((_, link), id) in links.iter().zip(&plan.ids) {
        if id.added {
            write_relationship(
                output,
                id.value.as_deref().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Missing added hyperlink identity",
                    )
                })?,
                link.target.as_deref().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Missing added hyperlink target")
                })?,
                true,
            )?;
        }
    }
    Ok(())
}
