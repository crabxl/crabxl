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
        self.save_with_models(output, options, None, 0, false)
    }
    pub(crate) fn save_with_models<W: Write + Seek>(
        &mut self,
        output: W,
        options: SaveOptions,
        bank: Option<&crabxl_core::Workbook>,
        extra_retained: usize,
        data_only: bool,
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
            self.validate_style_edit(Some(catalog))?;
        }
        if self.theme_dirty {
            self.validate_theme_edit()?;
            if bank.is_none() {
                return Err(invalid(
                    "Theme rewrite requires its canonical workbook owner",
                ));
            }
        }
        let catalog_parts = self.prepare_catalog_parts(bank, extra_retained)?;
        let catalog_bytes = catalog_parts.capacity() * size_of::<catalog_parts::CatalogPart>()
            + catalog_parts
                .iter()
                .map(catalog_parts::CatalogPart::heap_bytes)
                .sum::<usize>();
        let hyperlink_plans =
            self.prepare_hyperlink_save(bank, extra_retained.saturating_add(catalog_bytes))?;
        let relationship_plans: BTreeMap<_, _> = hyperlink_plans
            .iter()
            .filter(|(_, (_, plan))| plan.relationships.is_some())
            .map(|(part, (id, plan))| (crate::package::relationship_part(part), (*id, plan)))
            .collect();
        let hyperlink_relationships: BTreeMap<String, &[u8]> = relationship_plans
            .iter()
            .filter_map(|(part, (_, plan))| {
                plan.relationships
                    .as_deref()
                    .map(|bytes| (part.clone(), bytes))
            })
            .collect();
        let feature_save = self.prepare_feature_save(&hyperlink_relationships)?;
        let printers: Vec<_> = self
            .print_patches
            .iter()
            .filter(|(part, _)| self.feature_patches.contains_key(*part))
            .filter_map(|(part, settings)| {
                settings
                    .setup
                    .printer_relationship
                    .as_ref()
                    .map(|id| (part.clone(), id.to_string()))
            })
            .collect();
        for (part, id) in printers {
            let rels = crate::package::relationship_part(&part);
            let bytes = match feature_save.replace.get(&rels) {
                Some(bytes) => bytes.clone(),
                None => self.book.part_bytes(&rels)?,
            };
            let mut xml = XmlStream::new(
                io::Cursor::new(bytes.as_slice()),
                rels.clone(),
                bytes.len() as u64,
                self.options.resources,
            );
            let mut found = false;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e)
                        if frame.depth == 2
                            && frame.scope == Scope::Relationships
                            && e.local_name().as_ref() == "Relationship"
                            && attribute(&e, b"Id")?.as_deref() == Some(&id) =>
                    {
                        if found
                            || attribute(&e, b"Type")?
                                .as_deref()
                                .and_then(|kind| kind.rsplit('/').next())
                                != Some("printerSettings")
                            || attribute(&e, b"TargetMode")?.as_deref() == Some("External")
                        {
                            return Err(invalid("Invalid printer relationship"));
                        }
                        let target = crate::package::resolve_part(
                            &part,
                            &crate::xml::required_attribute(&e, b"Target")?,
                        )?;
                        if !feature_save.parts.iter().any(|part| part.name == target)
                            && (feature_save.remove.contains(&target)
                                || self.book.archive.index_for_name(&target).is_none())
                        {
                            return Err(invalid("Missing printer payload"));
                        }
                        found = true;
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
            if !found {
                return Err(
                    invalid("Page setup has no matching printer relationship").with_part(part)
                );
            }
        }
        let metadata_counts = if let Some(roots) = self
            .workbook_feature_patch
            .as_deref()
            .and_then(|features| features.metadata.as_deref())
        {
            Some(
                roots
                    .iter()
                    .find_map(crabxl_core::features::WorkbookMetadata::annotation_counts)
                    .unwrap_or((0, 0)),
            )
        } else if self
            .membership
            .as_ref()
            .is_some_and(|membership| !membership.created.is_empty())
            || !self.model_patches.is_empty()
            || self
                .patches
                .values()
                .flat_map(|patches| patches.values())
                .any(|patch| patch.cell.value.annotations().is_some())
        {
            Some(self.book.annotation_counts()?)
        } else {
            None
        };
        for patch in self.patches.values().flat_map(|patches| patches.values()) {
            crate::encode::validate_annotations(&patch.cell.value, metadata_counts)?;
        }
        if self
            .workbook_feature_patch
            .as_deref()
            .is_some_and(|features| features.metadata.is_some())
        {
            let sources: Vec<_> = self
                .book
                .sheets()
                .iter()
                .enumerate()
                .filter(|(index, sheet)| {
                    sheet.kind() == SheetKind::Worksheet
                        && !self.model_patches.contains_key(index)
                        && !self
                            .membership
                            .as_deref()
                            .is_some_and(|membership| membership.removes_part(sheet.part()))
                })
                .map(|(_, sheet)| sheet.part().to_owned())
                .collect();
            for part in sources {
                let mut xml = self.book.part_stream(&part)?;
                loop {
                    let frame = xml.next()?;
                    match frame.event {
                        Event::Start(e)
                            if frame.scope == Scope::Spreadsheet
                                && frame.depth == 4
                                && e.local_name().as_ref() == "c" =>
                        {
                            let address = attribute(&e, b"r")?
                                .map(|value| value.parse::<CellAddress>())
                                .transpose()?;
                            if address.is_some_and(|address| {
                                self.patches.get(&part).is_some_and(|patches| {
                                    patches.contains_key(&(address.row.get(), address.column.get()))
                                })
                            }) {
                                continue;
                            }
                            let cell = attribute(&e, b"cm")?;
                            let value = attribute(&e, b"vm")?;
                            crate::encode::validate_annotation_indices(
                                cell.as_deref(),
                                value.as_deref(),
                                metadata_counts,
                            )
                            .map_err(|error| error.with_part(&part))?;
                        }
                        Event::Eof => break,
                        _ => {}
                    }
                }
            }
            if let Some(bank) = bank {
                for (_, sheet) in bank.sheets() {
                    for cell in sheet.cells() {
                        crate::encode::validate_annotations(&cell.value, metadata_counts)
                            .map_err(|error| error.with_cell(cell.address))?;
                    }
                }
            }
        }
        let feature_plans = &feature_save.sheets;
        let relationships_added = relationship_plans
            .keys()
            .chain(feature_save.replace.keys())
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
        let epoch = self.epoch();
        let dirty = data_only
            || self.patch_cells != 0
            || !self.model_patches.is_empty()
            || self.membership.as_deref().is_some_and(|membership| {
                membership.removal_dirty
                    || membership.created.values().any(|entry| entry.values_dirty)
            });
        let removed = self
            .parts
            .iter()
            .filter(|part| {
                dirty && self.chain_removals.contains(part.name.as_ref())
                    || membership.is_some_and(|(member, _)| member.removes_part(&part.name))
                    || feature_save.remove.contains(part.name.as_ref())
            })
            .count();
        let created = catalog_parts.len()
            + relationship_plans
                .keys()
                .filter(|part| self.book.archive.index_for_name(part).is_none())
                .count()
            + membership.map_or(0, |(member, _)| member.created.len())
            + feature_save.new_parts();
        if self
            .parts
            .len()
            .saturating_sub(removed)
            .saturating_add(created)
            > self.options.resources.max_archive_entries
        {
            return Err(limit(
                "Created package parts exceed configured archive count",
            ));
        }
        let mut zip = ZipWriter::new(output);
        zip.set_raw_comment(self.book.archive.comment().to_vec().into_boxed_slice())
            .map_err(|error| zip_error("Cannot preserve ZIP archive comment", error))?;
        let mut stats = SaveStats::default();
        let empty_chains = HashSet::new();
        // openpyxl does not keep digital signatures, so a changed signed package
        // is saved without them.
        let drop_signature = self.signed && self.is_dirty();
        let unsigned_rels = if drop_signature {
            let maximum = self.options.resources.max_part_bytes;
            let file = self
                .book
                .archive
                .by_name("_rels/.rels")
                .map_err(|cause| zip_error("Cannot read package relationships", cause))?;
            let mut bytes = Vec::new();
            file.take(maximum)
                .read_to_end(&mut bytes)
                .map_err(|cause| io_error("Cannot read package relationships", cause))?;
            Some(unsigned_relationships(&bytes)?)
        } else {
            None
        };
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
            if drop_signature && part.name.starts_with("_xmlsignatures/") {
                stats.removed_parts += 1;
                total -= u128::from(part.uncompressed_bytes);
                continue;
            }
            if let Some(bytes) = unsigned_rels
                .as_ref()
                .filter(|_| part.name.as_ref() == "_rels/.rels")
            {
                zip.start_file(
                    part.name.as_ref(),
                    crate::writer::compression_options(options.compression_level),
                )
                .map_err(|cause| zip_error("Cannot start package relationships", cause))?;
                zip.write_all(bytes)
                    .map_err(|cause| io_error("Cannot write package relationships", cause))?;
                total = total - u128::from(part.uncompressed_bytes) + bytes.len() as u128;
                stats.rewritten_parts += 1;
                continue;
            }
            if (dirty && self.chain_removals.contains(part.name.as_ref()))
                || membership.is_some_and(|(member, _)| member.removes_part(&part.name))
            {
                stats.removed_parts += 1;
                continue;
            }
            if feature_save.remove.contains(part.name.as_ref()) {
                stats.removed_parts += 1;
                total -= u128::from(part.uncompressed_bytes);
                continue;
            }
            if let Some(bytes) = feature_save.replace.get(part.name.as_ref()) {
                zip.start_file(
                    part.name.as_ref(),
                    crate::writer::compression_options(options.compression_level),
                )
                .map_err(|cause| zip_error("Cannot start rewritten feature part", cause))?;
                let written = if part.name.as_ref() == self.workbook_relationships {
                    patch_package_graph(
                        io::Cursor::new(bytes.as_slice()),
                        PartOutput {
                            inner: BufWriter::with_capacity(64 * 1024, &mut zip),
                            bytes: 0,
                            maximum: self.options.resources.max_part_bytes,
                        },
                        &part.name,
                        PackageGraphRewrite {
                            chains: if dirty {
                                &self.calc_chain_parts
                            } else {
                                &empty_chains
                            },
                            limits: self.options.resources,
                            membership,
                            relationships_added,
                            additions: &catalog_parts,
                            features: Some(&feature_save),
                        },
                    )?
                } else {
                    zip.write_all(bytes)
                        .map_err(|cause| io_error("Cannot write rewritten feature part", cause))?;
                    bytes.len() as u64
                };
                total = total - u128::from(part.uncompressed_bytes) + u128::from(written);
                if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                    return Err(limit("Edited feature package exceeds configured limit"));
                }
                stats.rewritten_parts += 1;
                stats.rewritten_xml_bytes += written;
                continue;
            }
            let chain_metadata = (!catalog_parts.is_empty()
                || membership.is_some()
                || dirty && !self.calc_chain_parts.is_empty()
                || (relationships_added || feature_save.types_changed())
                    && part.name.as_ref() == "[Content_Types].xml")
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
            let worksheet = (dirty
                || view_patch.is_some()
                || print_patch.is_some()
                || hyperlink.is_some()
                || feature_plans.contains_key(part.name.as_ref()))
                && self.book.sheets().iter().any(|sheet| {
                    sheet.kind() == SheetKind::Worksheet && sheet.part() == part.name.as_ref()
                });
            let chartsheet = feature_plans.contains_key(part.name.as_ref())
                && self.book.sheets().iter().any(|sheet| {
                    sheet.kind() == SheetKind::ChartSheet && sheet.part() == part.name.as_ref()
                });
            let workbook = (dirty
                || active.is_some()
                || !self.visibility_patches.is_empty()
                || !self.name_patches.is_empty()
                || self.catalog_order.is_some()
                || membership.is_some()
                || feature_save.workbook.is_some()
                || !feature_save.pivot_caches.is_empty())
                && part.name.as_ref() == self.book.workbook_part;
            let shared_strings = dirty && self.shared_string_parts.contains(part.name.as_ref());
            let styles =
                self.styles_dirty && self.book.style_part.as_deref() == Some(part.name.as_ref());
            let theme =
                self.theme_dirty && self.book.theme_part.as_deref() == Some(part.name.as_ref());
            if styles
                || theme
                || worksheet
                || chartsheet
                || workbook
                || shared_strings
                || chain_metadata
            {
                let file = self.book.archive.by_index(index).map_err(|error| {
                    zip_error("Cannot read affected XML part", error).with_part(part.name.as_ref())
                })?;
                // Large worksheets compress on several threads.
                let mut packed = if worksheet && !styles && !theme && !chartsheet {
                    crate::parallel_deflate::threads(
                        options.compression_level,
                        part.uncompressed_bytes,
                    )
                    .map(|threads| {
                        crate::parallel_deflate::ParallelDeflate::new(
                            options.compression_level,
                            threads,
                            None,
                        )
                    })
                    .transpose()
                    .map_err(|error| io_error("Cannot start parallel compression", error))?
                } else {
                    None
                };
                if packed.is_none() {
                    // Rewritten parts use the selected level; copied parts keep their bytes.
                    zip.start_file(
                        part.name.as_ref(),
                        crate::writer::compression_options(options.compression_level).large_file(
                            self.options.resources.max_part_bytes >= u64::from(u32::MAX),
                        ),
                    )
                    .map_err(|error| {
                        zip_error("Cannot start affected XML part", error)
                            .with_part(part.name.as_ref())
                    })?;
                }
                let target: &mut dyn Write = match packed.as_mut() {
                    Some(packed) => packed,
                    None => &mut zip,
                };
                let budget = PartOutput {
                    // Batch XML event fragments before feeding the compressor.
                    // The fixed buffer fits the operation's 64 KiB work reserve.
                    inner: BufWriter::with_capacity(64 * 1024, target),
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
                } else if chartsheet {
                    let mut xml = XmlStream::new(
                        BufReader::with_capacity(self.options.resources.input_buffer_bytes, file),
                        part.name.to_string(),
                        self.options
                            .resources
                            .max_metadata_bytes
                            .min(self.options.resources.max_part_bytes),
                        self.options.resources,
                    );
                    let original = crate::drawing::read_chartsheet(&mut xml)?;
                    let root = feature_plans
                        .get(part.name.as_ref())
                        .and_then(|plan| plan.features.chartsheet.clone())
                        .unwrap_or(original);
                    let uri = crate::drawing::chartsheet_namespace(&root)?;
                    let mut budget = budget;
                    crate::drawing::write_chartsheet(
                        &mut budget,
                        Some(root),
                        feature_plans
                            .get(part.name.as_ref())
                            .and_then(|plan| plan.output.refs.drawing.as_deref()),
                        uri,
                    )
                    .map_err(|cause| io_error("Cannot rewrite chartsheet", cause))?;
                    budget
                        .flush()
                        .map_err(|cause| io_error("Cannot flush chartsheet", cause))?;
                    Ok(budget.bytes)
                } else if worksheet {
                    patch_worksheet(
                        file,
                        budget,
                        &part.name,
                        WorksheetRewrite {
                            data_only,
                            metadata_counts,
                            patches: self.patches.get(part.name.as_ref()),
                            limits: self.options.resources,
                            formula_attributes: self.options.formula_attributes,
                            views: view_patch,
                            printing: print_patch,
                            features: feature_plans
                                .get(part.name.as_ref())
                                .map(|plan| (&*plan.features, &plan.output.refs)),
                            hyperlinks: hyperlink,
                            invalidate_caches: dirty,
                            keep_model_caches: self.keep_model_caches,
                            model,
                            catalog: bank.and_then(crabxl_core::Workbook::style_catalog),
                            epoch,
                            iso_dates: self.dates.1,
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
                            date_1904: epoch == DateEpoch::Mac1904,
                            limits: self.options.resources,
                            invalidate_caches: dirty,
                            active,
                            visibility: &self.visibility_patches,
                            names: &self.name_patches,
                            order: self.catalog_order.as_deref(),
                            membership,
                            features: feature_save.workbook.as_deref(),
                            pivot_caches: &feature_save.pivot_caches,
                            external_refs: feature_save.external_refs.as_deref(),
                        },
                    )
                } else if shared_strings {
                    patch_shared_strings(file, budget, &part.name, self.options.resources)
                } else {
                    patch_package_graph(
                        file,
                        budget,
                        &part.name,
                        PackageGraphRewrite {
                            chains: if dirty {
                                &self.calc_chain_parts
                            } else {
                                &empty_chains
                            },
                            limits: self.options.resources,
                            membership,
                            relationships_added,
                            additions: &catalog_parts,
                            features: Some(&feature_save),
                        },
                    )
                }
                .map_err(|error| error.with_part(part.name.as_ref()))?;
                if let Some(packed) = packed {
                    packed.finish_into(&mut zip, &part.name).map_err(|error| {
                        io_error("Cannot write affected XML part", error)
                            .with_part(part.name.as_ref())
                    })?;
                }
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
        let mut new_features: Vec<(String, crabxl_core::BinaryPayload)> = feature_save
            .parts
            .iter()
            .map(|part| (part.name.clone(), part.bytes.clone()))
            .collect();
        new_features.extend(
            feature_save
                .replace
                .iter()
                .filter(|(name, _)| self.book.archive.index_for_name(name).is_none())
                .map(|(name, bytes)| (name.clone(), bytes.as_slice().into())),
        );
        for (name, bytes) in &new_features {
            if bytes.len() > self.options.resources.max_part_bytes {
                return Err(limit("New feature part exceeds configured limit").with_part(name));
            }
            zip.start_file(
                name,
                crate::writer::compression_options(options.compression_level)
                    .large_file(bytes.len() >= u64::from(u32::MAX)),
            )
            .map_err(|cause| zip_error("Cannot start new feature part", cause))?;
            bytes
                .write_to(&mut zip)
                .map_err(|cause| io_error("Cannot write new feature part", cause))?;
            total += u128::from(bytes.len());
            if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                return Err(limit("Created feature package exceeds configured limit"));
            }
            stats.created_parts += 1;
            stats.rewritten_xml_bytes += bytes.len();
        }
        for (part, (_, plan)) in &relationship_plans {
            if self.book.archive.index_for_name(part).is_some()
                || feature_save.replace.contains_key(part)
            {
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
        for added in &catalog_parts {
            let bank = bank.ok_or_else(|| invalid("Missing canonical catalog owner"))?;
            zip.start_file(
                &added.part,
                crate::writer::compression_options(options.compression_level)
                    .large_file(self.options.resources.max_part_bytes >= u64::from(u32::MAX)),
            )
            .map_err(|cause| {
                zip_error("Cannot start created catalog part", cause).with_part(&added.part)
            })?;
            let mut budget = PartOutput {
                inner: BufWriter::with_capacity(64 * 1024, &mut zip),
                bytes: 0,
                maximum: self.options.resources.max_part_bytes,
            };
            match added.kind {
                catalog_parts::CatalogKind::Styles => crate::styles::write_styles(
                    &mut budget,
                    bank.style_catalog()
                        .ok_or_else(|| invalid("Missing canonical stylesheet"))?,
                    crate::StyleWritePolicy::RetainExplicit,
                )
                .map_err(|cause| io_error("Cannot create source stylesheet", cause))?,
                catalog_parts::CatalogKind::Theme => budget
                    .write_all(bank.theme().map_or(
                        crate::default_theme::DEFAULT_THEME.as_bytes(),
                        crabxl_core::Theme::bytes,
                    ))
                    .map_err(|cause| io_error("Cannot create source theme", cause))?,
            }
            budget
                .flush()
                .map_err(|cause| io_error("Cannot flush created catalog", cause))?;
            total += u128::from(budget.bytes);
            if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                return Err(limit("Created catalog package exceeds configured limit"));
            }
            stats.created_parts += 1;
            stats.rewritten_xml_bytes += budget.bytes;
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
                let bytes = if created.kind == SheetKind::ChartSheet {
                    let source = if let Some(root) = feature_plans
                        .get(created.part.as_str())
                        .and_then(|plan| plan.features.chartsheet.clone())
                    {
                        Some(root)
                    } else if let Some(index) = created.template {
                        let part = self.book.sheets()[index].part().to_owned();
                        let mut xml = self.book.part_stream(&part)?;
                        Some(crate::drawing::read_chartsheet(&mut xml)?)
                    } else {
                        None
                    };
                    crate::drawing::write_chartsheet(
                        &mut output,
                        source,
                        feature_plans
                            .get(created.part.as_str())
                            .and_then(|plan| plan.output.refs.drawing.as_deref()),
                        membership.namespace(),
                    )
                    .map_err(|cause| io_error("Cannot create chartsheet", cause))?;
                    output
                        .flush()
                        .map_err(|cause| io_error("Cannot flush chartsheet", cause))?;
                    output.bytes
                } else if let Some(index) = created.template {
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
                    let empty_features = crabxl_core::SheetFeatures::default();
                    let empty_refs = crate::features::sheet::PartRefs::default();
                    patch_worksheet(
                        input,
                        output,
                        &part,
                        WorksheetRewrite {
                            data_only,
                            metadata_counts,
                            patches: None,
                            limits: self.options.resources,
                            formula_attributes: self.options.formula_attributes,
                            views: sheet.sheet_views(),
                            printing: sheet.print_settings(),
                            // openpyxl's copy omits tables, rules, validation and comments.
                            features: Some(
                                feature_plans
                                    .get(created.part.as_str())
                                    .map_or((&empty_features, &empty_refs), |plan| {
                                        (&*plan.features, &plan.output.refs)
                                    }),
                            ),
                            hyperlinks: hyperlink_plans
                                .get(created.part.as_str())
                                .map(|(_, plan)| (sheet.hyperlinks(), plan)),
                            invalidate_caches: true,
                            keep_model_caches: self.keep_model_caches,
                            model: Some(sheet),
                            catalog: bank.style_catalog(),
                            epoch,
                            iso_dates: self.dates.1,
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
                            metadata_counts,
                            epoch,
                            iso_dates: self.dates.1,
                            non_finite: self.options.non_finite,
                            formula_attributes: self.options.formula_attributes,
                            keep_caches: self.keep_model_caches,
                        },
                        membership.namespace(),
                        hyperlink_plans
                            .get(created.part.as_str())
                            .map(|(_, plan)| plan),
                        feature_plans
                            .get(created.part.as_str())
                            .map(|plan| (&*plan.features, &plan.output.refs)),
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

/// Package relationships without the digital signature origin.
fn unsigned_relationships(bytes: &[u8]) -> Result<Vec<u8>> {
    let xml = |cause| {
        Error::caused_by(
            ErrorKind::Xml,
            "Cannot rewrite package relationships",
            cause,
        )
    };
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut writer = Writer::new(Vec::with_capacity(bytes.len()));
    let mut buffer = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buffer).map_err(xml)?;
        let signature = match &event {
            Event::Eof => break,
            Event::Empty(e) => {
                e.local_name().as_ref() == "Relationship"
                    && attribute(e, b"Type")?.is_some_and(|kind| kind.contains("digital-signature"))
            }
            _ => false,
        };
        if !signature {
            writer
                .write_event(event)
                .map_err(|cause| io_error("Cannot write package relationships", cause))?;
        }
        buffer.clear();
    }
    Ok(writer.into_inner())
}
