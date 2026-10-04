// SPDX-License-Identifier: MIT
// Shared record layout selected from calamine's cells_reader.rs,
// Copyright 2016-2026 Johann Tuffe. Sparse storage, actual-anchor translation,
// typed array/table records and resource/projection integration are refactored.
use crate::encode::write_attribute as attribute;
use crabxl_core::{
    CellAddress, CellRange, DataTableOptions, Error, ErrorKind, Formula, FormulaFlag,
    FormulaMetadata, FormulaRange, FormulaReadPolicy, FormulaType, Result,
};
use quick_xml::{encoding::Decoder, events::BytesStart};
use std::{collections::HashMap, io::Write};

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn limit() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "Shared formula template allowance exceeded",
    )
}

pub(crate) fn is_shared(e: &BytesStart<'_>, decoder: Decoder) -> Result<bool> {
    for attribute in e.attributes() {
        let attribute = attribute
            .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid formula attribute", e))?;
        if attribute.key.as_ref() == b"t" {
            return attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                .map(|value| value == "shared")
                .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid shared formula type", e));
        }
    }
    Ok(false)
}
/// Value projection and replacement analysis have different extension guarantees.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderPolicy {
    Compatible,
    Strict,
    KnownRecords,
}
impl From<FormulaReadPolicy> for HeaderPolicy {
    fn from(policy: FormulaReadPolicy) -> Self {
        match policy {
            FormulaReadPolicy::Compatible => Self::Compatible,
            FormulaReadPolicy::ValidateGroups => Self::Strict,
        }
    }
}
pub(crate) fn header(
    e: &BytesStart<'_>,
    decoder: Decoder,
    maximum: usize,
    policy: HeaderPolicy,
) -> Result<FormulaMetadata> {
    let compatible = policy == HeaderPolicy::Compatible;
    // Select semantics before reading hints: attribute order has no meaning.
    let mut kind = FormulaType::Normal;
    for attribute in e.attributes() {
        let attribute = attribute
            .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid formula attribute", e))?;
        if attribute.key.as_ref() != b"t" {
            continue;
        }
        let value = attribute
            .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
            .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid formula type", e))?;
        if value.len() > maximum {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Formula attribute exceeds cell byte limit",
            ));
        }
        kind = match value.as_ref() {
            "normal" => FormulaType::Normal,
            "shared" => FormulaType::Shared {
                index: 0,
                master: false,
            },
            "array" => FormulaType::Array,
            "dataTable" => FormulaType::DataTable,
            _ if compatible => FormulaType::Normal,
            _ => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown formula encoding type",
                ));
            }
        };
    }
    let mut metadata = FormulaMetadata {
        kind,
        ..Default::default()
    };
    let mut index = None;
    let mut table = DataTableOptions::default();
    let mut table_seen = false;
    for attribute in e.attributes() {
        let attribute = attribute
            .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid formula attribute", e))?;
        if attribute.key.as_ref() == b"xmlns" || attribute.key.as_ref().starts_with(b"xmlns:") {
            continue;
        }
        let value = attribute
            .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
            .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid formula attribute value", e))?;
        if value.len() > maximum {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Formula attribute exceeds cell byte limit",
            ));
        }
        let relevant = match kind {
            FormulaType::Normal => false,
            FormulaType::Shared { .. } => matches!(attribute.key.as_ref(), b"t" | b"si" | b"ref"),
            FormulaType::Array => matches!(
                attribute.key.as_ref(),
                b"t" | b"ref" | b"aca" | b"ca" | b"bx"
            ),
            FormulaType::DataTable => matches!(
                attribute.key.as_ref(),
                b"t" | b"ref" | b"ca" | b"dt2D" | b"dtr" | b"del1" | b"del2" | b"r1" | b"r2"
            ),
        };
        if compatible && !relevant {
            // Decode unused attributes above, but do not impose their semantics.
            continue;
        }
        let flag = |value: std::borrow::Cow<'_, str>| {
            let value = value.into_owned().into_boxed_str();
            if policy != HeaderPolicy::Strict {
                Ok(FormulaFlag::from_literal(value))
            } else {
                FormulaFlag::from_xml(value)
            }
        };
        match attribute.key.as_ref() {
            b"t" => {}
            b"si" => {
                index = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| invalid("Invalid shared formula index"))?,
                )
            }
            b"ref" => {
                metadata.reference = Some(FormulaRange::from_literal(
                    value.into_owned().into_boxed_str(),
                ))
            }
            b"aca" => metadata.flags.always_calculate = Some(flag(value)?),
            b"ca" => metadata.flags.calculate_cell = Some(flag(value)?),
            b"bx" => metadata.flags.data_box = Some(flag(value)?),
            b"dt2D" => {
                table.two_dimensions = Some(flag(value)?);
                table_seen = true;
            }
            b"dtr" => {
                table.row_table = Some(flag(value)?);
                table_seen = true;
            }
            b"del1" => {
                table.deleted1 = Some(flag(value)?);
                table_seen = true;
            }
            b"del2" => {
                table.deleted2 = Some(flag(value)?);
                table_seen = true;
            }
            b"r1" => {
                table.input1 = Some(value.into_owned().into_boxed_str());
                table_seen = true;
            }
            b"r2" => {
                table.input2 = Some(value.into_owned().into_boxed_str());
                table_seen = true;
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Unknown formula attribute",
                ));
            }
        }
    }
    if matches!(metadata.kind, FormulaType::Shared { .. }) {
        metadata.kind = FormulaType::Shared {
            index: index.ok_or_else(|| invalid("Shared formula index is missing"))?,
            master: false,
        };
    }
    if table_seen || metadata.kind == FormulaType::DataTable {
        metadata.data_table = Some(Box::new(table));
    }
    metadata.validate()?;
    if metadata.payload_bytes() > maximum {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "Formula metadata exceeds cell byte limit",
        ));
    }
    Ok(metadata)
}

struct Template {
    anchor: CellAddress,
    range: Option<CellRange>,
    expression: Box<str>,
}
/// Diagnostics for worksheet-local dependency work, separate from decoded cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SharedFormulaStats {
    /// Distinct identities currently retained.
    pub templates: usize,
    /// Managed table/capacity/payload estimate, excluding allocator/runtime overhead.
    pub accounted_bytes: usize,
    /// Followers expanded from a template.
    pub expanded: u64,
    /// Followers without a template under compatible policy.
    pub unresolved: u64,
}
pub(crate) struct SharedFormulas {
    templates: HashMap<u32, Template>,
    payload: usize,
    maximum: usize,
    maximum_count: usize,
    stats: SharedFormulaStats,
}
impl SharedFormulas {
    pub(crate) fn new(maximum: usize, maximum_count: usize) -> Self {
        Self {
            templates: HashMap::new(),
            payload: 0,
            maximum,
            maximum_count,
            stats: SharedFormulaStats::default(),
        }
    }
    fn bucket_bytes(capacity: usize) -> usize {
        if capacity == 0 {
            0
        } else {
            (capacity.saturating_add(1))
                .checked_next_power_of_two()
                .unwrap_or(usize::MAX)
                .saturating_mul(size_of::<(u32, Template)>() + 1)
                .saturating_add(16)
        }
    }
    pub(crate) fn required_bytes(&self, index: u32, expression_bytes: usize) -> usize {
        if self.contains(index) {
            return self.stats.accounted_bytes;
        }
        let capacity = self.templates.capacity();
        let buckets = if self.templates.len() == capacity {
            if capacity == 0 {
                Self::bucket_bytes(3)
            } else {
                Self::bucket_bytes(capacity).saturating_mul(2)
            }
        } else {
            Self::bucket_bytes(capacity)
        };
        size_of::<Self>()
            .saturating_add(buckets)
            .saturating_add(self.payload)
            .saturating_add(expression_bytes)
    }
    pub(crate) fn set_maximum(&mut self, maximum: usize) {
        self.maximum = maximum;
    }
    pub(crate) fn stats(&self) -> SharedFormulaStats {
        self.stats
    }
    pub(crate) fn contains(&self, index: u32) -> bool {
        self.templates.contains_key(&index)
    }
    pub(crate) fn resolve(
        &mut self,
        address: CellAddress,
        mut expression: Box<str>,
        metadata: &mut FormulaMetadata,
        policy: FormulaReadPolicy,
        maximum_expression: usize,
    ) -> Result<Box<str>> {
        let FormulaType::Shared { index, .. } = metadata.kind else {
            return Ok(expression);
        };
        let strict = policy == FormulaReadPolicy::ValidateGroups;
        if let Some(template) = self.templates.get(&index) {
            if strict && !expression.is_empty() {
                return Err(invalid("Duplicate shared formula master"));
            }
            if strict && template.range.is_some_and(|range| !range.contains(address)) {
                return Err(invalid("Shared follower lies outside its declared range"));
            }
            // The public baseline retains the first group definition. Later
            // source bodies do not silently replace an existing template.
            metadata.kind = FormulaType::Shared {
                index,
                master: false,
            };
            expression = crabxl_core::translate_expression(
                &template.expression,
                i64::from(address.row.get()) - i64::from(template.anchor.row.get()),
                i64::from(address.column.get()) - i64::from(template.anchor.column.get()),
                maximum_expression,
            )?
            .into_boxed_str();
            self.stats.expanded += 1;
        } else {
            if strict && expression.is_empty() {
                return Err(invalid("Shared formula template is missing"));
            }
            metadata.kind = FormulaType::Shared {
                index,
                master: !expression.is_empty(),
            };
            let range = if strict {
                metadata
                    .reference
                    .as_ref()
                    .map(|value| value.range())
                    .transpose()?
            } else {
                None
            };
            if strict && range.is_some_and(|range| !range.contains(address)) {
                return Err(invalid("Shared master lies outside its declared range"));
            }
            if self.templates.len() >= self.maximum_count {
                return Err(limit());
            }
            let payload = self
                .payload
                .checked_add(expression.len())
                .ok_or_else(limit)?;
            let capacity = self.templates.capacity();
            let buckets = if self.templates.len() == capacity {
                if capacity == 0 {
                    Self::bucket_bytes(3)
                } else {
                    Self::bucket_bytes(capacity).saturating_mul(2)
                }
            } else {
                Self::bucket_bytes(capacity)
            };
            if size_of::<Self>()
                .saturating_add(buckets)
                .saturating_add(payload)
                > self.maximum
            {
                return Err(limit());
            }
            self.templates.try_reserve(1).map_err(|e| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot reserve shared formula templates",
                    e,
                )
            })?;
            let accounted = size_of::<Self>()
                .saturating_add(Self::bucket_bytes(self.templates.capacity()))
                .saturating_add(payload);
            if accounted > self.maximum {
                return Err(limit());
            }
            let mut stored = String::new();
            stored.try_reserve_exact(expression.len()).map_err(|e| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot own shared formula template",
                    e,
                )
            })?;
            stored.push_str(&expression);
            self.templates.insert(
                index,
                Template {
                    anchor: address,
                    range,
                    expression: stored.into_boxed_str(),
                },
            );
            if expression.is_empty() {
                self.stats.unresolved += 1;
            }
            self.payload = payload;
            self.stats.templates = self.templates.len();
            self.stats.accounted_bytes = accounted;
        }
        Ok(expression)
    }
}

fn flag(
    output: &mut impl Write,
    name: &str,
    value: &Option<FormulaFlag>,
    policy: crate::FormulaWritePolicy,
) -> std::io::Result<()> {
    if let Some(value) = value {
        if policy == crate::FormulaWritePolicy::Compatible
            && (value.source() == Some("")
                || (value.source().is_none() && value.value() == Some(false)))
        {
            return Ok(());
        }
        attribute(output, name, value.spelling())?;
    }
    Ok(())
}
pub(crate) fn write(
    output: &mut impl Write,
    formula: &Formula,
    policy: crate::FormulaWritePolicy,
) -> std::io::Result<()> {
    output.write_all(b"<f")?;
    if let Some(metadata) = formula.metadata() {
        match metadata.kind {
            FormulaType::Normal => {}
            // Public reference save expands ordinary shared groups. Only an
            // unresolved source follower keeps its index instead of inventing text.
            FormulaType::Shared { index, .. } if formula.expression().is_empty() => {
                attribute(output, "t", "shared")?;
                write!(output, " si=\"{index}\"")?;
            }
            FormulaType::Shared { .. } => {}
            FormulaType::Array => attribute(output, "t", "array")?,
            FormulaType::DataTable => attribute(output, "t", "dataTable")?,
        }
        if !matches!(metadata.kind, FormulaType::Shared { .. }) {
            if let Some(reference) = &metadata.reference {
                let spelling = reference.spelling();
                if !spelling.is_empty() || policy == crate::FormulaWritePolicy::RetainExplicit {
                    attribute(output, "ref", &spelling)?;
                }
            }
        }
        flag(output, "aca", &metadata.flags.always_calculate, policy)?;
        flag(output, "ca", &metadata.flags.calculate_cell, policy)?;
        flag(output, "bx", &metadata.flags.data_box, policy)?;
        if let Some(table) = &metadata.data_table {
            flag(output, "dt2D", &table.two_dimensions, policy)?;
            flag(output, "dtr", &table.row_table, policy)?;
            flag(output, "del1", &table.deleted1, policy)?;
            flag(output, "del2", &table.deleted2, policy)?;
            if let Some(reference) = &table.input1 {
                if !reference.is_empty() || policy == crate::FormulaWritePolicy::RetainExplicit {
                    attribute(output, "r1", reference)?;
                }
            }
            if let Some(reference) = &table.input2 {
                if !reference.is_empty() || policy == crate::FormulaWritePolicy::RetainExplicit {
                    attribute(output, "r2", reference)?;
                }
            }
        }
    }
    output.write_all(b">")?;
    crate::encode::write_text(output, formula.expression())?;
    output.write_all(b"</f>")
}
