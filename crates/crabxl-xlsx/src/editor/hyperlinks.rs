//! Sparse source identities and save-time relationship graph preparation.
use super::*;

pub(super) type HyperlinkPlans =
    BTreeMap<String, (crabxl_core::SheetId, crate::hyperlinks::source::Plan)>;
impl<R: Read + Seek> WorkbookEditor<R> {
    pub(crate) fn prepare_hyperlink_patch(&self, index: usize, base_bytes: usize) -> Result<usize> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed hyperlinks requires an explicit signature policy",
            ));
        }
        if self
            .book
            .sheets()
            .get(index)
            .is_none_or(|sheet| sheet.kind() != SheetKind::Worksheet)
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Only worksheet hyperlink edits are implemented",
            ));
        }
        let bytes = base_bytes.saturating_add(
            usize::from(!self.hyperlink_patches.contains_key(&index)) * PATCH_BYTES,
        );
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Hyperlink rewrite ledger exceeds allowance",
            ));
        }
        Ok(bytes)
    }
    pub(crate) fn commit_hyperlink_patch(
        &mut self,
        index: usize,
        id: crabxl_core::SheetId,
        bytes: usize,
    ) {
        self.hyperlink_patches.insert(index, id);
        self.patch_bytes = bytes;
    }
    pub(crate) fn prepare_hyperlink_save(
        &mut self,
        bank: Option<&crabxl_core::Workbook>,
        extra_retained: usize,
    ) -> Result<HyperlinkPlans> {
        let mut result = HyperlinkPlans::new();
        let Some(bank) = bank else {
            if !self.hyperlink_patches.is_empty() {
                return Err(invalid("Hyperlink rewrite requires its canonical workbook"));
            }
            return Ok(result);
        };
        let maximum = self
            .allowance
            .retained_data_bytes
            .saturating_sub(self.retained_package_bytes())
            .saturating_sub(bank.charged_bytes())
            .saturating_sub(extra_retained)
            .min(
                self.options
                    .resources
                    .max_metadata_bytes
                    .min(usize::MAX as u64) as usize,
            );
        let mut charged = 0_usize;
        for (&index, &id) in &self.hyperlink_patches {
            let part = self
                .book
                .sheets()
                .get(index)
                .ok_or_else(|| invalid("Missing hyperlink source identity"))?
                .part()
                .to_owned();
            if self
                .membership
                .as_ref()
                .is_some_and(|member| member.removes_part(&part))
            {
                continue;
            }
            let node = METADATA_ENTRY_BYTES
                .saturating_add(part.capacity())
                .saturating_add(crate::package::relationship_part(&part).len());
            let available = maximum
                .checked_sub(charged.saturating_add(node))
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Hyperlink save plans exceed allowance",
                    )
                })?;
            let plan = crate::hyperlinks::source::prepare(
                &mut self.book,
                &part,
                bank.sheet(id)?.hyperlinks(),
                available,
            )?;
            charged = charged
                .saturating_add(node)
                .saturating_add(plan.heap_bytes());
            result.insert(part, (id, plan));
        }
        if let Some(member) = &self.membership {
            for created in member.created.values() {
                let id = created
                    .bank_id
                    .ok_or_else(|| invalid("Created hyperlink sheet has no bank identity"))?;
                let links = bank.sheet(id)?.hyperlinks();
                if links.is_empty() && created.template.is_none() {
                    continue;
                }
                let node = METADATA_ENTRY_BYTES
                    .saturating_add(created.part.len())
                    .saturating_add(crate::package::relationship_part(&created.part).len());
                let available = maximum
                    .checked_sub(charged.saturating_add(node))
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Created hyperlink save plans exceed allowance",
                        )
                    })?;
                let plan = crate::hyperlinks::source::prepare(
                    &mut self.book,
                    &created.part,
                    links,
                    available,
                )?;
                charged = charged
                    .saturating_add(node)
                    .saturating_add(plan.heap_bytes());
                result.insert(created.part.to_string(), (id, plan));
            }
        }
        Ok(result)
    }
}
