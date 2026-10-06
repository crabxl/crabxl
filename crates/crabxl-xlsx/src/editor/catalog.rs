//! Bounded live catalog membership for the canonical source-backed bank.
//! Original CrabXL package coordination; no worksheet bodies are cached here.
use super::{PATCH_BYTES, SheetDeclaration, WorkbookEditor, attribute, emit, invalid, zip_error};
use crate::xml::{Scope, XmlStream};
use crabxl_core::{Error, ErrorKind, Result, SheetId, Workbook};
use quick_xml::{
    Writer,
    events::{BytesStart, Event},
};
use std::{
    collections::BTreeMap,
    io::{BufReader, Read, Seek, Write},
};

// Cover a sparsely occupied B-tree node as well as its retained entry.
const CREATED_BYTES: usize = 2048;
pub(super) struct Created {
    pub(super) part: String,
    pub(super) bank_id: Option<SheetId>,
    target: String,
    relationship: String,
    sheet_id: u32,
    pub(super) values_dirty: bool,
    pub(super) template: Option<usize>,
}
impl Created {
    fn bytes(&self) -> usize {
        CREATED_BYTES + self.part.capacity() + self.target.capacity() + self.relationship.capacity()
    }
}
pub(super) struct Membership {
    pub(super) declarations: Vec<SheetDeclaration>,
    originals: Vec<SheetId>,
    pub(super) created: BTreeMap<u32, Created>,
}
impl Membership {
    fn bytes(&self) -> usize {
        PATCH_BYTES
            + self.originals.capacity() * size_of::<SheetId>()
            + self.declarations.capacity() * size_of::<SheetDeclaration>()
            + self
                .declarations
                .iter()
                .map(|entry| {
                    entry.start.as_ref().len()
                        + entry.relationship.as_ref().map_or(0, |id| id.len())
                })
                .sum::<usize>()
            + self.created.values().map(Created::bytes).sum::<usize>()
    }
    pub(super) fn write_entries<W: Write>(
        &self,
        writer: &mut Writer<super::PartOutput<W>>,
        bank: &Workbook,
    ) -> Result<()> {
        for (id, sheet) in bank.sheets() {
            let start = if let Some(created) = self
                .created
                .values()
                .find(|entry| entry.bank_id == Some(id))
            {
                let mut start = BytesStart::new("sheet");
                let uri = self.namespace();
                start.push_attribute(("xmlns", uri));
                start.push_attribute(("xmlns:r", self.relationship_namespace()));
                let number = created.sheet_id.to_string();
                start.push_attribute(("sheetId", number.as_str()));
                start.push_attribute(("r:id", created.relationship.as_str()));
                start.push_attribute(("name", sheet.name()));
                start.push_attribute(("state", sheet.visibility().as_str()));
                start
            } else {
                let index = self
                    .originals
                    .iter()
                    .position(|original| *original == id)
                    .ok_or_else(|| invalid("Canonical sheet has no package catalog identity"))?;
                let original = self
                    .declarations
                    .get(index)
                    .ok_or_else(|| invalid("Original declaration identity is inconsistent"))?;
                super::rewritten_catalog_entry(
                    &original.start,
                    Some(sheet.name()),
                    Some(sheet.visibility()),
                )?
            };
            emit(writer, Event::Empty(start))?;
        }
        Ok(())
    }
    pub(super) fn namespace(&self) -> &'static str {
        self.declarations
            .first()
            .map_or(crate::xml::MAIN_URI, |entry| entry.namespace)
    }
    fn relationship_namespace(&self) -> &'static str {
        if self.namespace() == crate::xml::STRICT_MAIN_URI {
            crate::xml::STRICT_OFFICE_REL_URI
        } else {
            crate::xml::OFFICE_REL_URI
        }
    }
    pub(super) fn write_additions<W: Write>(
        &self,
        writer: &mut Writer<super::PartOutput<W>>,
        types: bool,
        bank: &Workbook,
    ) -> Result<()> {
        for created in self.created.values() {
            bank.sheet(
                created
                    .bank_id
                    .ok_or_else(|| invalid("Created sheet has no model identity"))?,
            )?;
            let mut start = BytesStart::new(if types { "Override" } else { "Relationship" });
            if types {
                start.push_attribute((
                    "xmlns",
                    "http://schemas.openxmlformats.org/package/2006/content-types",
                ));
                let part = format!("/{}", created.part);
                start.push_attribute(("PartName", part.as_str()));
                start.push_attribute((
                    "ContentType",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
                ));
            } else {
                start.push_attribute((
                    "xmlns",
                    "http://schemas.openxmlformats.org/package/2006/relationships",
                ));
                start.push_attribute(("Id", created.relationship.as_str()));
                let kind = format!("{}/worksheet", self.relationship_namespace());
                start.push_attribute(("Type", kind.as_str()));
                start.push_attribute(("Target", created.target.as_str()));
            }
            emit(writer, Event::Empty(start))?;
        }
        Ok(())
    }
}
pub(crate) struct CreatePlan {
    incoming: Option<Box<Membership>>,
    created: Created,
    pub(crate) bytes: usize,
    /// Old declaration cache remains resident until this metadata plan commits.
    pub(crate) scratch_bytes: usize,
    view_index: i64,
}
impl<R: Read + Seek> WorkbookEditor<R> {
    pub(crate) fn copy_template(&self, id: SheetId) -> Option<usize> {
        self.membership
            .as_deref()?
            .created
            .values()
            .find(|created| created.bank_id == Some(id))?
            .template
    }
    pub(crate) fn prepare_copy_template(&mut self, index: usize) -> Result<()> {
        let info = self
            .book
            .sheets()
            .get(index)
            .ok_or_else(|| invalid("Missing copied source identity"))?;
        let name = info.name().to_owned();
        let part = info.part().to_owned();
        // Reuse the affected-cell/feature and SST checks without marking the source dirty.
        self.prepare_model(&name)?;
        let limits = self.options.resources;
        let file = self
            .book
            .archive
            .by_name(&part)
            .map_err(|cause| zip_error("Cannot inspect copied worksheet metadata", cause))?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, file),
            part.clone(),
            limits.max_part_bytes,
            limits,
        );
        let mut properties = false;
        let mut data = false;
        loop {
            let frame = xml.next()?;
            if let Event::Start(e) = &frame.event {
                let name = e.local_name();
                let name = name.as_ref().as_bytes();
                if frame.depth == 2 {
                    if matches!(name, b"sheetViews" | b"headerFooter") {
                        let depth = frame.depth;
                        crate::style_codec::skip(&mut xml, depth)?;
                        continue;
                    }
                    properties = name == b"sheetPr";
                    data = name == b"sheetData";
                }
                if !data
                    && frame.depth >= 3
                    && !(properties
                        && frame.depth == 3
                        && frame.scope == Scope::Spreadsheet
                        && matches!(name, b"tabColor" | b"outlinePr" | b"pageSetUpPr"))
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Copying affected worksheet metadata remains unimplemented",
                    )
                    .with_part(&part));
                }
                if !data {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|cause| {
                            Error::caused_by(
                                ErrorKind::Xml,
                                "Invalid copied worksheet metadata",
                                cause,
                            )
                        })?;
                        let key = attr.key.as_ref().as_bytes();
                        if key != b"xmlns"
                            && !key.starts_with(b"xmlns:")
                            && (key.contains(&b':') || frame.depth == 1 || key == b"codeName")
                        {
                            return Err(Error::new(
                                ErrorKind::Unsupported,
                                "Copying affected worksheet identities remains unimplemented",
                            )
                            .with_part(&part));
                        }
                    }
                }
            }
            if matches!(&frame.event, Event::End(_)) && frame.depth == 1 {
                properties = false;
                data = false;
            }
            if frame.office_relationship.is_some() {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Copying affected worksheet relationships remains unimplemented",
                )
                .with_part(&part));
            }
            if let Event::Start(e) = &frame.event
                && attribute(e, b"codeName")?.is_some()
            {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Copying worksheet VBA identity remains unimplemented",
                )
                .with_part(&part));
            }
            if matches!(frame.event, Event::Eof) {
                break;
            }
        }
        Ok(())
    }
    pub(crate) fn membership_is_dirty(&self) -> bool {
        self.membership.is_some()
    }
    pub(crate) fn prepare_create(
        &mut self,
        originals: Vec<SheetId>,
        allowance: usize,
    ) -> Result<CreatePlan> {
        self.validate_workbook_patch(0, self.patch_bytes, false)?;
        let incoming = if self.membership.is_none() {
            if originals.len() != self.book.sheets().len() {
                return Err(invalid(
                    "Original stable catalog identities are inconsistent",
                ));
            }
            let declarations = self.read_catalog_entries(allowance)?;
            Some(Box::new(Membership {
                declarations,
                originals,
                created: BTreeMap::new(),
            }))
        } else {
            None
        };
        let membership = incoming
            .as_deref()
            .or(self.membership.as_deref())
            .ok_or_else(|| invalid("Missing live catalog membership"))?;
        if self
            .parts
            .len()
            .saturating_add(membership.created.len())
            .saturating_add(1)
            > self.options.resources.max_archive_entries
        {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Created package entry limit exceeded",
            ));
        }
        let mut maximum = 0u32;
        for entry in &membership.declarations {
            let id = attribute(&entry.start, b"sheetId")?
                .ok_or_else(|| invalid("Original sheet has no sheetId"))?
                .parse::<u32>()
                .map_err(|cause| {
                    Error::caused_by(ErrorKind::InvalidData, "Invalid original sheetId", cause)
                })?;
            maximum = maximum.max(id);
        }
        for entry in membership.created.values() {
            maximum = maximum.max(entry.sheet_id);
        }
        let sheet_id = maximum
            .checked_add(1)
            .ok_or_else(|| invalid("Sheet identifier space is exhausted"))?;
        let mut candidate = u64::from(sheet_id);
        let (part, target) = loop {
            let target = format!("worksheets/crabxl-sheet-{candidate}.xml");
            let part = crate::package::resolve_part(&self.book.workbook_part, &target)?;
            if !self.parts.iter().any(|entry| entry.name.as_ref() == part)
                && !membership.created.values().any(|entry| entry.part == part)
            {
                break (part, target);
            }
            candidate = candidate
                .checked_add(1)
                .ok_or_else(|| invalid("Worksheet part identifier space is exhausted"))?;
        };
        let mut rid = 0u64;
        for entry in membership.created.values() {
            if let Some(number) = entry
                .relationship
                .strip_prefix("rIdCrabxl")
                .and_then(|number| number.parse::<u64>().ok())
            {
                rid = rid.max(number);
            }
        }
        let limits = self.options.resources;
        let input = self
            .book
            .archive
            .by_name(&self.workbook_relationships)
            .map_err(|cause| zip_error("Cannot allocate created worksheet relationship", cause))?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, input),
            self.workbook_relationships.clone(),
            limits.max_metadata_bytes.min(limits.max_part_bytes),
            limits,
        );
        loop {
            let frame = xml.next()?;
            match &frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Relationships
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"Relationship" =>
                {
                    let id = attribute(e, b"Id")?
                        .ok_or_else(|| invalid("Relationship has no identifier"))?;
                    if let Some(number) = id
                        .strip_prefix("rIdCrabxl")
                        .and_then(|number| number.parse::<u64>().ok())
                    {
                        rid = rid.max(number);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        drop(xml);
        let relationship = format!(
            "rIdCrabxl{}",
            rid.checked_add(1)
                .ok_or_else(|| invalid("Relationship identifier space is exhausted"))?
        );
        let created = Created {
            bank_id: None,
            part,
            target,
            relationship,
            sheet_id,
            values_dirty: false,
            template: None,
        };
        let old_order = self.catalog_order.as_ref().map_or(0, |order| order.charged);
        let prior = self.membership.as_deref().map_or(0, Membership::bytes);
        let bytes = self
            .patch_bytes
            .saturating_sub(old_order)
            .saturating_sub(prior)
            .saturating_add(membership.bytes())
            .saturating_add(created.bytes())
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Created worksheet catalog allowance exceeded",
            ));
        }
        Ok(CreatePlan {
            incoming,
            created,
            bytes,
            scratch_bytes: old_order,
            view_index: self.active_view_index(),
        })
    }
    pub(crate) fn prepare_copy(
        &mut self,
        originals: Vec<SheetId>,
        allowance: usize,
        template: Option<usize>,
    ) -> Result<CreatePlan> {
        let mut plan = self.prepare_create(originals, allowance)?;
        plan.created.template = template;
        plan.created.values_dirty = true;
        Ok(plan)
    }
    pub(crate) fn commit_create(&mut self, plan: CreatePlan, id: SheetId) {
        if let Some(incoming) = plan.incoming {
            self.membership = Some(incoming);
        }
        if let Some(membership) = &mut self.membership {
            let mut created = plan.created;
            created.bank_id = Some(id);
            membership.created.insert(created.sheet_id, created);
        }
        self.catalog_order = None;
        if self.active_patch.is_none() {
            self.active_patch = Some(super::ActivePatch::Deferred(plan.view_index));
        }
        self.patch_bytes = plan.bytes;
    }
    pub(crate) fn created_values_dirty(&mut self, id: SheetId) {
        if let Some(entry) = self.membership.as_mut().and_then(|membership| {
            membership
                .created
                .values_mut()
                .find(|entry| entry.bank_id == Some(id))
        }) {
            entry.values_dirty = true;
        }
    }
    pub(crate) fn prepare_membership_metadata(&mut self) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        self.validate_workbook_patch(0, bytes, false)?;
        Ok(bytes)
    }
}
