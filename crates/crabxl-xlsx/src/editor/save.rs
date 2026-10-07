//! Save.
use super::*;

impl<R: Read + Seek> WorkbookEditor<R> {
    /// Save to a caller-owned fresh/truncated sink; failures may leave partial
    /// sink bytes. Caller ownership can be retained by passing &mut W.
    /// Unchanged entries preserve compressed payloads. Value/formula edits rewrite
    /// all worksheets to remove old caches, plus calculation properties; pure
    /// display/printing edits rewrite only the selected worksheets.
    pub fn save<W: Write + Seek>(
        &mut self,
        output: W,
        options: SaveOptions,
    ) -> Result<(W, SaveStats)> {
        self.save_with_models(output, options, None, 0)
    }
    pub(crate) fn save_with_models<W: Write + Seek>(
        &mut self,
        output: W,
        options: SaveOptions,
        bank: Option<&crabxl_core::Workbook>,
        extra_retained: usize,
    ) -> Result<(W, SaveStats)> {
        if (!self.model_patches.is_empty() || self.membership.is_some()) && bank.is_none() {
            return Err(invalid(
                "Model rewrite requires its canonical workbook owner",
            ));
        }
        crate::writer::validate_compression_level(options.compression_level)?;
        if self.styles_dirty {
            let catalog = bank
                .and_then(crabxl_core::Workbook::style_catalog)
                .ok_or_else(|| invalid("Style rewrite requires its canonical catalog"))?;
            self.validate_style_edit(catalog)?;
        }
        if self.theme_dirty {
            self.validate_theme_edit()?;
            if bank.is_none() {
                return Err(invalid(
                    "Theme rewrite requires its canonical workbook owner",
                ));
            }
        }
        let hyperlink_plans = self.prepare_hyperlink_save(bank, extra_retained)?;
        let relationship_plans: BTreeMap<_, _> = hyperlink_plans
            .iter()
            .filter(|(_, (_, plan))| plan.relationships.is_some())
            .map(|(part, (id, plan))| (crate::package::relationship_part(part), (*id, plan)))
            .collect();
        let relationships_added = relationship_plans
            .keys()
            .any(|part| self.book.archive.index_for_name(part).is_none());
        let membership = self.membership.as_deref().zip(bank);
        let active = if let Some((_, bank)) = membership {
            Some(crabxl_core::normalize_active_view(
                self.active_view_index(),
                bank.sheets().count(),
                |position| {
                    bank.sheets()
                        .nth(position)
                        .map_or(crabxl_core::SheetVisibility::Hidden, |(_, sheet)| {
                            sheet.visibility()
                        })
                },
            )?)
        } else {
            self.active_for_save()?
        };
        let epoch = if self.book.date_1904() {
            DateEpoch::Mac1904
        } else {
            DateEpoch::Windows1900
        };
        let dirty = self.patch_cells != 0
            || !self.model_patches.is_empty()
            || self.membership.as_deref().is_some_and(|membership| {
                membership.removal_dirty
                    || membership.created.values().any(|entry| entry.values_dirty)
            });
        let mut zip = ZipWriter::new(output);
        zip.set_raw_comment(self.book.archive.comment().to_vec().into_boxed_slice())
            .map_err(|error| zip_error("Cannot preserve ZIP archive comment", error))?;
        let mut stats = SaveStats::default();
        let empty_chains = HashSet::new();
        let mut total: u128 = self
            .parts
            .iter()
            .filter(|part| {
                (!dirty || !self.chain_removals.contains(part.name.as_ref()))
                    && !membership.is_some_and(|(member, _)| member.removes_part(&part.name))
            })
            .map(|part| u128::from(part.uncompressed_bytes))
            .sum();
        for index in 0..self.parts.len() {
            let part = &self.parts[index];
            if (dirty && self.chain_removals.contains(part.name.as_ref()))
                || membership.is_some_and(|(member, _)| member.removes_part(&part.name))
            {
                stats.removed_parts += 1;
                continue;
            }
            let chain_metadata = (membership.is_some()
                || dirty && !self.calc_chain_parts.is_empty()
                || relationships_added && part.name.as_ref() == "[Content_Types].xml")
                && (part.name.as_ref() == "[Content_Types].xml"
                    || part.name.as_ref() == self.workbook_relationships);
            let hyperlink = hyperlink_plans
                .get(part.name.as_ref())
                .map(|(id, plan)| {
                    let links = bank
                        .ok_or_else(|| invalid("Missing hyperlink bank owner"))?
                        .sheet(*id)?
                        .hyperlinks();
                    Ok::<_, Error>((links, plan))
                })
                .transpose()?;
            if let Some((_, plan)) = relationship_plans.get(part.name.as_ref()) {
                let bytes = plan
                    .relationships
                    .as_deref()
                    .ok_or_else(|| invalid("Missing planned hyperlink relationships"))?;
                if bytes.len() as u64 > self.options.resources.max_part_bytes {
                    return Err(limit(
                        "Hyperlink relationship part exceeds configured limit",
                    ));
                }
                zip.start_file(
                    part.name.as_ref(),
                    crate::writer::compression_options(options.compression_level),
                )
                .map_err(|cause| {
                    zip_error("Cannot start rewritten hyperlink relationships", cause)
                })?;
                zip.write_all(bytes)
                    .map_err(|cause| io_error("Cannot write hyperlink relationships", cause))?;
                total = total - u128::from(part.uncompressed_bytes) + bytes.len() as u128;
                if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                    return Err(limit("Edited hyperlink package exceeds configured limit"));
                }
                stats.rewritten_parts += 1;
                stats.rewritten_xml_bytes += bytes.len() as u64;
                continue;
            }
            let view_patch = self.view_patches.get(part.name.as_ref()).map(Box::as_ref);
            let print_patch = self.print_patches.get(part.name.as_ref()).map(Box::as_ref);
            let model = self
                .book
                .sheets()
                .iter()
                .position(|sheet| sheet.part() == part.name.as_ref())
                .and_then(|index| self.model_patches.get(&index))
                .map(|id| {
                    bank.ok_or_else(|| invalid("Missing canonical model owner"))?
                        .sheet(*id)
                })
                .transpose()?;
            let worksheet =
                (dirty || view_patch.is_some() || print_patch.is_some() || hyperlink.is_some())
                    && self.book.sheets().iter().any(|sheet| {
                        sheet.kind() == SheetKind::Worksheet && sheet.part() == part.name.as_ref()
                    });
            let workbook = (dirty
                || active.is_some()
                || !self.visibility_patches.is_empty()
                || !self.name_patches.is_empty()
                || self.catalog_order.is_some()
                || membership.is_some())
                && part.name.as_ref() == self.book.workbook_part;
            let shared_strings = dirty && self.shared_string_parts.contains(part.name.as_ref());
            let styles =
                self.styles_dirty && self.book.style_part.as_deref() == Some(part.name.as_ref());
            let theme =
                self.theme_dirty && self.book.theme_part.as_deref() == Some(part.name.as_ref());
            if styles || theme || worksheet || workbook || shared_strings || chain_metadata {
                let file = self.book.archive.by_index(index).map_err(|error| {
                    zip_error("Cannot read affected XML part", error).with_part(part.name.as_ref())
                })?;
                // Rewritten parts use the selected level; copied parts keep their bytes.
                zip.start_file(
                    part.name.as_ref(),
                    crate::writer::compression_options(options.compression_level)
                        .large_file(self.options.resources.max_part_bytes >= u64::from(u32::MAX)),
                )
                .map_err(|error| {
                    zip_error("Cannot start affected XML part", error).with_part(part.name.as_ref())
                })?;
                let budget = PartOutput {
                    // Batch XML event fragments before feeding the compressor.
                    // The fixed buffer fits the operation's 64 KiB work reserve.
                    inner: BufWriter::with_capacity(64 * 1024, &mut zip),
                    bytes: 0,
                    maximum: self.options.resources.max_part_bytes,
                };
                let written = if styles {
                    drop(file);
                    let mut budget = budget;
                    crate::styles::write_styles(
                        &mut budget,
                        bank.and_then(crabxl_core::Workbook::style_catalog)
                            .ok_or_else(|| invalid("Missing canonical style catalog"))?,
                        crate::StyleWritePolicy::RetainExplicit,
                    )
                    .map_err(|cause| io_error("Cannot rewrite source stylesheet", cause))?;
                    budget
                        .inner
                        .flush()
                        .map_err(|cause| io_error("Cannot flush source stylesheet", cause))?;
                    Ok(budget.bytes)
                } else if theme {
                    drop(file);
                    let mut budget = budget;
                    let bytes = bank.and_then(crabxl_core::Workbook::theme).map_or(
                        crate::default_theme::DEFAULT_THEME.as_bytes(),
                        crabxl_core::Theme::bytes,
                    );
                    budget
                        .write_all(bytes)
                        .map_err(|cause| io_error("Cannot rewrite source theme", cause))?;
                    budget
                        .inner
                        .flush()
                        .map_err(|cause| io_error("Cannot flush source theme", cause))?;
                    Ok(budget.bytes)
                } else if worksheet {
                    patch_worksheet(
                        file,
                        budget,
                        &part.name,
                        WorksheetRewrite {
                            patches: self.patches.get(part.name.as_ref()),
                            limits: self.options.resources,
                            formula_attributes: self.options.formula_attributes,
                            views: view_patch,
                            printing: print_patch,
                            hyperlinks: hyperlink,
                            invalidate_caches: dirty,
                            model,
                            catalog: bank.and_then(crabxl_core::Workbook::style_catalog),
                            epoch,
                            non_finite: self.options.non_finite,
                            copying: false,
                        },
                    )
                } else if workbook {
                    patch_workbook(
                        file,
                        budget,
                        &part.name,
                        WorkbookRewrite {
                            limits: self.options.resources,
                            invalidate_caches: dirty,
                            active,
                            visibility: &self.visibility_patches,
                            names: &self.name_patches,
                            order: self.catalog_order.as_deref(),
                            membership,
                        },
                    )
                } else if shared_strings {
                    patch_shared_strings(file, budget, &part.name, self.options.resources)
                } else {
                    patch_chain_metadata(
                        file,
                        budget,
                        &part.name,
                        if dirty {
                            &self.calc_chain_parts
                        } else {
                            &empty_chains
                        },
                        self.options.resources,
                        membership,
                        relationships_added,
                    )
                }
                .map_err(|error| error.with_part(part.name.as_ref()))?;
                total = total - u128::from(part.uncompressed_bytes) + u128::from(written);
                if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                    return Err(limit("Edited package uncompressed byte limit exceeded"));
                }
                stats.rewritten_parts += 1;
                stats.rewritten_xml_bytes += written;
            } else {
                if options.verify_unchanged {
                    let file = self
                        .book
                        .archive
                        .by_index(index)
                        .map_err(|error| zip_error("Cannot validate unchanged part", error))?;
                    let bytes = io::copy(
                        &mut file.take(self.options.resources.max_part_bytes.saturating_add(1)),
                        &mut io::sink(),
                    )
                    .map_err(|error| {
                        io_error("Cannot validate unchanged part CRC", error)
                            .with_part(part.name.as_ref())
                    })?;
                    if bytes > self.options.resources.max_part_bytes {
                        return Err(limit("Unchanged part byte limit exceeded")
                            .with_part(part.name.as_ref()));
                    }
                }
                let file = self
                    .book
                    .archive
                    .by_index(index)
                    .map_err(|error| zip_error("Cannot reopen original part", error))?;
                zip.raw_copy_file(file).map_err(|error| {
                    zip_error("Cannot copy original compressed part", error)
                        .with_part(part.name.as_ref())
                })?;
                stats.copied_parts += 1;
            }
        }
        for (part, (_, plan)) in &relationship_plans {
            if self.book.archive.index_for_name(part).is_some() {
                continue;
            }
            let bytes = plan
                .relationships
                .as_deref()
                .ok_or_else(|| invalid("Missing new hyperlink relationships"))?;
            if bytes.len() as u64 > self.options.resources.max_part_bytes {
                return Err(limit(
                    "New hyperlink relationship part exceeds configured limit",
                ));
            }
            zip.start_file(
                part.as_str(),
                crate::writer::compression_options(options.compression_level),
            )
            .map_err(|cause| zip_error("Cannot start new hyperlink relationships", cause))?;
            zip.write_all(bytes)
                .map_err(|cause| io_error("Cannot write new hyperlink relationships", cause))?;
            total += bytes.len() as u128;
            if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                return Err(limit("Created hyperlink package exceeds configured limit"));
            }
            stats.created_parts += 1;
            stats.rewritten_xml_bytes += bytes.len() as u64;
        }
        if let Some((membership, bank)) = membership {
            for created in membership.created.values() {
                let sheet = bank.sheet(
                    created
                        .bank_id
                        .ok_or_else(|| invalid("Created sheet has no model identity"))?,
                )?;
                zip.start_file(
                    &created.part,
                    crate::writer::compression_options(options.compression_level)
                        .large_file(self.options.resources.max_part_bytes >= u64::from(u32::MAX)),
                )
                .map_err(|cause| {
                    zip_error("Cannot start created worksheet", cause).with_part(&created.part)
                })?;
                let mut output = PartOutput {
                    inner: BufWriter::with_capacity(64 * 1024, &mut zip),
                    bytes: 0,
                    maximum: self.options.resources.max_part_bytes,
                };
                let bytes = if let Some(index) = created.template {
                    let part = self
                        .book
                        .sheets()
                        .get(index)
                        .ok_or_else(|| invalid("Missing copied worksheet template"))?
                        .part()
                        .to_owned();
                    let input = self.book.archive.by_name(&part).map_err(|cause| {
                        zip_error("Cannot reopen copied worksheet template", cause)
                    })?;
                    patch_worksheet(
                        input,
                        output,
                        &part,
                        WorksheetRewrite {
                            patches: None,
                            limits: self.options.resources,
                            formula_attributes: self.options.formula_attributes,
                            views: None,
                            printing: None,
                            hyperlinks: hyperlink_plans
                                .get(created.part.as_str())
                                .map(|(_, plan)| (sheet.hyperlinks(), plan)),
                            invalidate_caches: true,
                            model: Some(sheet),
                            catalog: bank.style_catalog(),
                            epoch,
                            non_finite: self.options.non_finite,
                            copying: true,
                        },
                    )?
                } else {
                    crate::loaded_codec::write_new(
                        &mut output,
                        sheet,
                        bank.style_catalog(),
                        self.options.resources,
                        crate::loaded_codec::Encoding {
                            epoch,
                            non_finite: self.options.non_finite,
                            formula_attributes: self.options.formula_attributes,
                        },
                        membership.namespace(),
                        hyperlink_plans
                            .get(created.part.as_str())
                            .map(|(_, plan)| plan),
                    )
                    .map_err(|error| error.with_part(&created.part))?;
                    output
                        .flush()
                        .map_err(|cause| io_error("Cannot flush created worksheet", cause))?;
                    output.bytes
                };
                total += u128::from(bytes);
                if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                    return Err(limit("Created package uncompressed byte limit exceeded"));
                }
                stats.created_parts += 1;
                stats.rewritten_xml_bytes += bytes;
            }
        }
        let mut output = zip
            .finish()
            .map_err(|error| zip_error("Cannot finish edited package", error))?;
        output
            .flush()
            .map_err(|error| io_error("Cannot flush edited package", error))?;
        if let Some(active) = active {
            self.active_patch = Some(match self.active_patch {
                Some(ActivePatch::Deferred(_)) => ActivePatch::Deferred(active.requested_index),
                _ => ActivePatch::Visible(active.requested_index as usize),
            });
        }
        Ok((output, stats))
    }
    /// Write a fresh adjacent temporary output and replace the target only after
    /// successful ZIP completion. Supports saving over the original path without
    /// truncating its live source. Failed saves leave the target unchanged.
    /// No crash-durability or cross-platform replacement guarantee is implied.
    pub fn save_path(&mut self, path: impl AsRef<Path>, options: SaveOptions) -> Result<SaveStats> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::Builder::new()
            .prefix("crabxl-save-")
            .tempfile_in(parent)
            .map_err(|error| io_error("Cannot create adjacent output temporary file", error))?;
        let (_, stats) = self.save(&mut temporary, options)?;
        // std::fs::rename supports Windows replacement with a live source handle.
        // Keep the temporary path guarded so failures still remove the output.
        let temporary = temporary.into_temp_path();
        std::fs::rename(&temporary, path)
            .map_err(|error| io_error("Cannot replace edited workbook target", error))?;
        Ok(stats)
    }
}
