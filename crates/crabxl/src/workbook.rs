//! The editable workbook.

use crate::features::DefinedName;
use crate::style::Style;
use crate::worksheet::Worksheet;
use crate::{Error, ErrorKind, Result};
use crabxl_core::{SheetId, SheetVisibility, StyleId};
use crabxl_xlsx::{DataLossPolicy, LoadOptions, LoadedWorkbook};
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

/// A readable, seekable byte source that a workbook can own.
pub trait Source: Read + Seek + Send {}
impl<T: Read + Seek + Send> Source for T {}

pub(crate) type Engine = LoadedWorkbook<Box<dyn Source>>;

/// Input safety limits for opening files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limits {
    /// Limits that open every file Excel writes and stop ZIP bombs and
    /// deeply nested XML.
    #[default]
    Default,
    /// Tighter limits for files from untrusted sources.
    Strict,
    /// No limits. Use only for files you trust.
    Unlimited,
}

impl Limits {
    pub(crate) fn resources(self) -> crabxl_core::ResourceLimits {
        match self {
            Self::Default => crabxl_core::ResourceLimits::default(),
            Self::Strict => crabxl_core::ResourceLimits::strict(),
            Self::Unlimited => crabxl_core::ResourceLimits::unlimited(),
        }
    }
}

/// How much memory a workbook may keep for loaded sheets, pending edits and
/// shared strings. Temporary I/O buffers are extra.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Memory {
    /// Choose a budget from the available memory.
    #[default]
    Auto,
    /// A fixed budget in bytes.
    Budget(usize),
}

/// Options for [`Workbook::open_with`] and [`Workbook::from_reader`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct OpenOptions {
    /// Input safety limits.
    pub limits: Limits,
    /// Memory budget.
    pub memory: Memory,
    /// What a save does when it cannot keep part of the source package.
    pub data_loss: DataLossPolicy,
    /// Read every non-integer number as its exact decimal text.
    pub exact_numbers: bool,
}

impl OpenOptions {
    /// The default options.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the input safety limits.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
    /// Set the memory budget.
    pub fn with_memory(mut self, memory: Memory) -> Self {
        self.memory = memory;
        self
    }
    /// Allow or reject saves that cannot keep part of the source.
    pub fn with_data_loss(mut self, policy: DataLossPolicy) -> Self {
        self.data_loss = policy;
        self
    }
    /// Read non-integer numbers as exact decimals.
    pub fn with_exact_numbers(mut self, exact: bool) -> Self {
        self.exact_numbers = exact;
        self
    }
    fn load(&self) -> LoadOptions {
        let mut options = match self.memory {
            Memory::Auto => LoadOptions::default(),
            Memory::Budget(bytes) => LoadOptions::with_memory_limit(bytes),
        };
        let budget = options.resources.max_materialized_bytes;
        options.resources = crabxl_core::ResourceLimits {
            max_materialized_bytes: budget,
            ..self.limits.resources()
        };
        options.editor.data_loss = self.data_loss;
        options.read.exact_numbers = self.exact_numbers;
        options.read.rich_text = true;
        options
    }
}

/// Options for [`Workbook::save_with`] and [`Workbook::write_to`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct SaveOptions {
    /// ZIP compression level: 0 stores, 1 to 9 deflate. `None` uses 6.
    pub compression: Option<u8>,
    /// Ask Excel to recalculate every formula when it opens the file.
    pub recalculate_on_open: bool,
}

impl SaveOptions {
    /// The default options.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the compression level, 0 to 9.
    pub fn with_compression(mut self, level: u8) -> Self {
        self.compression = Some(level);
        self
    }
    /// Ask Excel to recalculate every formula when it opens the file.
    pub fn with_recalculate_on_open(mut self, recalculate: bool) -> Self {
        self.recalculate_on_open = recalculate;
        self
    }
}

/// Visibility of a sheet tab.
pub use crabxl_core::SheetVisibility as Visibility;
/// Document properties: title, creator, dates and custom properties.
pub use crabxl_core::features::DocumentProperties as Properties;

const PLACEHOLDER: &str = "crabxl-empty";

/// An XLSX workbook that you can read and change, then save.
///
/// A workbook opened from a file loads each sheet when you first use it, and
/// a save copies every part you did not change byte for byte. Saving to the
/// file you opened is safe: saves to a path are atomic.
///
/// ```no_run
/// use crabxl::Workbook;
/// # fn main() -> crabxl::Result<()> {
/// let mut book = Workbook::open("report.xlsx")?;
/// let mut sheet = book.worksheet("Summary")?;
/// sheet.set_value("B2", 1250.5)?;
/// sheet.set_formula("B10", "SUM(B2:B9)")?;
/// book.save("report.xlsx")?;
/// # Ok(())
/// # }
/// ```
pub struct Workbook {
    pub(crate) engine: Engine,
    pub(crate) styles: HashMap<usize, (Style, StyleId)>,
    empty: bool,
}

impl Workbook {
    /// A new workbook with no sheets. Add at least one worksheet before you
    /// save it.
    pub fn new() -> Result<Self> {
        let mut writer = crabxl_xlsx::WorkbookWriter::new(crabxl_xlsx::WriteOptions::default())?;
        writer.start_sheet(PLACEHOLDER)?;
        let bytes = writer.finish(Cursor::new(Vec::new()))?.into_inner();
        let mut book = Self::from_reader(Cursor::new(bytes), &OpenOptions::default())?;
        book.empty = true;
        Ok(book)
    }
    /// Open a file with the default options.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, &OpenOptions::default())
    }
    /// Open a file.
    pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref()).map_err(|error| {
            Error::caused_by(
                ErrorKind::Io,
                format!("Cannot open {}", path.as_ref().display()),
                error,
            )
        })?;
        Self::from_reader(std::io::BufReader::new(file), options)
    }
    /// Open a workbook from any seekable byte source, such as a
    /// `Cursor<Vec<u8>>`.
    pub fn from_reader(source: impl Source + 'static, options: &OpenOptions) -> Result<Self> {
        let engine =
            LoadedWorkbook::with_options(Box::new(source) as Box<dyn Source>, options.load())?;
        Ok(Self {
            engine,
            styles: HashMap::new(),
            empty: false,
        })
    }

    pub(crate) fn ids(&self) -> Vec<(SheetId, String)> {
        self.engine
            .model()
            .sheets()
            .filter(|(_, sheet)| !(self.empty && sheet.name() == PLACEHOLDER))
            .map(|(id, sheet)| (id, sheet.name().to_owned()))
            .collect()
    }
    fn id(&self, name: &str) -> Result<SheetId> {
        self.ids()
            .into_iter()
            .find(|(_, sheet)| sheet.eq_ignore_ascii_case(name))
            .map(|(id, _)| id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, format!("No sheet named '{name}'")))
    }
    fn worksheet_id(&self, name: &str) -> Result<SheetId> {
        let id = self.id(name)?;
        if self.engine.sheet_kind(id) != Some(crabxl_xlsx::SheetKind::Worksheet) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("'{name}' is not a worksheet"),
            ));
        }
        Ok(id)
    }

    /// The names of all sheets, in tab order.
    pub fn sheet_names(&self) -> Vec<String> {
        self.ids().into_iter().map(|(_, name)| name).collect()
    }
    /// The names of the worksheets, in tab order. Chartsheets are left out.
    pub fn worksheet_names(&self) -> Vec<String> {
        self.ids()
            .into_iter()
            .filter(|(id, _)| {
                self.engine.sheet_kind(*id) == Some(crabxl_xlsx::SheetKind::Worksheet)
            })
            .map(|(_, name)| name)
            .collect()
    }
    /// A worksheet by name. Names compare without regard to ASCII case.
    pub fn worksheet(&mut self, name: &str) -> Result<Worksheet<'_>> {
        let id = self.worksheet_id(name)?;
        Ok(Worksheet::new(self, id))
    }
    /// A worksheet by its zero-based position among all sheets.
    pub fn worksheet_at(&mut self, index: usize) -> Result<Worksheet<'_>> {
        let name = self
            .ids()
            .get(index)
            .map(|(_, name)| name.clone())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    format!("No sheet at position {index}"),
                )
            })?;
        self.worksheet(&name)
    }
    /// Add an empty worksheet after the last sheet.
    ///
    /// Sheet names have 1 to 31 characters, cannot contain `: \ / ? * [ ]`,
    /// and are unique without regard to case.
    pub fn add_worksheet(&mut self, name: &str) -> Result<Worksheet<'_>> {
        if self.id(name).is_ok() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("A sheet named '{name}' already exists"),
            ));
        }
        let id = self.engine.create_sheet(name)?;
        if self.empty {
            if let Some(placeholder) = self.engine.sheet_id(PLACEHOLDER) {
                self.engine.set_active_sheet(id)?;
                self.engine.remove_sheet(placeholder)?;
            }
            self.empty = false;
        }
        Ok(Worksheet::new(self, id))
    }
    /// Copy a sheet completely, including its views, objects and notes.
    /// Table names in the copy get unique suffixes.
    pub fn copy_worksheet(&mut self, from: &str, name: &str) -> Result<Worksheet<'_>> {
        let source = self.worksheet_id(from)?;
        if self.id(name).is_ok() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("A sheet named '{name}' already exists"),
            ));
        }
        let id = self.engine.copy_sheet(source, name)?;
        self.engine
            .set_sheet_visibility(id, SheetVisibility::Visible)?;
        Ok(Worksheet::new(self, id))
    }
    /// Remove a sheet.
    pub fn remove_sheet(&mut self, name: &str) -> Result<()> {
        let id = self.id(name)?;
        self.engine.remove_sheet(id).map(|_| ())
    }
    /// Move a sheet to a zero-based tab position.
    pub fn move_sheet(&mut self, name: &str, position: usize) -> Result<()> {
        let id = self.id(name)?;
        self.engine.move_sheet(id, position)
    }
    /// The sheet shown when the file opens.
    pub fn active_sheet(&self) -> Option<String> {
        let index = usize::try_from(self.engine.active_view_index()).ok()?;
        self.ids().get(index).map(|(_, name)| name.clone())
    }
    /// Show a sheet when the file opens. Hidden sheets cannot be active.
    pub fn set_active_sheet(&mut self, name: &str) -> Result<()> {
        let id = self.id(name)?;
        self.engine.set_active_sheet(id)
    }
    /// The visibility of a sheet tab.
    pub fn visibility(&self, name: &str) -> Result<Visibility> {
        let id = self.id(name)?;
        Ok(self.engine.model().sheet(id)?.visibility())
    }
    /// Show or hide a sheet tab. One sheet must stay visible.
    pub fn set_visibility(&mut self, name: &str, visibility: Visibility) -> Result<()> {
        let id = self.id(name)?;
        self.engine.set_sheet_visibility(id, visibility)
    }

    /// The defined names, including print areas (`_xlnm.Print_Area`).
    pub fn defined_names(&mut self) -> Result<Vec<DefinedName>> {
        let names = self.ids();
        Ok(self
            .engine
            .workbook_features()?
            .defined_names
            .iter()
            .map(|name| DefinedName {
                name: name.name.to_string(),
                formula: name.value.to_string(),
                sheet: name
                    .local_sheet_id
                    .and_then(|index| names.get(index as usize))
                    .map(|(_, sheet)| sheet.clone()),
            })
            .collect())
    }
    /// Add a defined name, or replace one with the same name and scope.
    pub fn add_defined_name(&mut self, name: DefinedName) -> Result<()> {
        let scope = match &name.sheet {
            Some(sheet) => {
                let id = self.id(sheet)?;
                Some(
                    self.ids()
                        .iter()
                        .position(|(other, _)| *other == id)
                        .unwrap_or(0) as u32,
                )
            }
            None => None,
        };
        let mut features = self.engine.workbook_features()?.clone();
        features.defined_names.retain(|other| {
            !(other.name.eq_ignore_ascii_case(&name.name) && other.local_sheet_id == scope)
        });
        features
            .defined_names
            .push(crabxl_core::features::DefinedName {
                name: name.name.into(),
                value: name.formula.into(),
                local_sheet_id: scope,
                ..Default::default()
            });
        self.engine.set_workbook_features(features)
    }
    /// Remove a workbook-wide defined name.
    pub fn remove_defined_name(&mut self, name: &str) -> Result<()> {
        let mut features = self.engine.workbook_features()?.clone();
        let before = features.defined_names.len();
        features.defined_names.retain(|other| {
            !(other.name.eq_ignore_ascii_case(name) && other.local_sheet_id.is_none())
        });
        if features.defined_names.len() == before {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("No defined name '{name}'"),
            ));
        }
        self.engine.set_workbook_features(features)
    }
    /// The document properties.
    pub fn properties(&mut self) -> Result<Properties> {
        Ok(self.engine.workbook_features()?.properties.clone())
    }
    /// Replace the document properties.
    pub fn set_properties(&mut self, properties: Properties) -> Result<()> {
        let mut features = self.engine.workbook_features()?.clone();
        features.properties = properties;
        self.engine.set_workbook_features(features)
    }

    /// Calculate every formula and store the results as cached values.
    #[cfg(feature = "calc")]
    pub fn calculate(&mut self) -> Result<crabxl_calc::CalcStats> {
        self.engine.calculate()
    }

    /// Save to a path with the default options. The save is atomic: the file
    /// is either replaced completely or left unchanged.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.save_with(path, &SaveOptions::default())
    }
    /// Save to a path atomically.
    pub fn save_with(&mut self, path: impl AsRef<Path>, options: &SaveOptions) -> Result<()> {
        self.prepare_save(options)?;
        crabxl_xlsx::write_atomically(path, |file| {
            self.engine
                .save(file, self.engine_options(options))
                .map(|_| ())
        })
    }
    /// Write the workbook to a seekable writer and return the writer.
    pub fn write_to<W: Write + Seek>(&mut self, output: W, options: &SaveOptions) -> Result<W> {
        self.prepare_save(options)?;
        let save = self.engine_options(options);
        self.engine.save(output, save).map(|(output, _)| output)
    }
    fn prepare_save(&mut self, options: &SaveOptions) -> Result<()> {
        if self.empty {
            return Err(Error::new(
                ErrorKind::NoVisibleSheet,
                "Add a worksheet before you save a new workbook",
            ));
        }
        if options.recalculate_on_open {
            let mut features = self.engine.workbook_features()?.clone();
            let mut calculation = features
                .calculation
                .clone()
                .unwrap_or_else(|| crabxl_core::XmlElement::new("calcPr"));
            calculation
                .attributes
                .retain(|(name, _)| &**name != "fullCalcOnLoad");
            calculation
                .attributes
                .push(("fullCalcOnLoad".into(), "1".into()));
            features.calculation = Some(calculation);
            self.engine.set_workbook_features(features)?;
        }
        Ok(())
    }
    fn engine_options(&self, options: &SaveOptions) -> crabxl_xlsx::SaveOptions {
        crabxl_xlsx::SaveOptions {
            compression_level: options.compression,
            ..Default::default()
        }
    }

    /// Register a style once and return its id.
    pub(crate) fn style_id(&mut self, style: &Style) -> Result<StyleId> {
        let key = style.identity();
        if let Some((_, id)) = self.styles.get(&key) {
            return Ok(*id);
        }
        let id = self.engine.register_style(style.to_core())?;
        self.styles.insert(key, (style.clone(), id));
        Ok(id)
    }
    /// Every table name in the workbook, in lowercase.
    pub(crate) fn table_names(&mut self) -> Result<(Vec<String>, u32)> {
        let mut names = Vec::new();
        let mut next = 1;
        for (id, _) in self.ids() {
            if self.engine.sheet_kind(id) != Some(crabxl_xlsx::SheetKind::Worksheet) {
                continue;
            }
            for table in &self.engine.sheet_features(id)?.tables {
                names.push(table.display_name.to_lowercase());
                next = next.max(table.id + 1);
            }
        }
        Ok((names, next))
    }
    pub(crate) fn sheet_name(&self, id: SheetId) -> String {
        self.engine
            .model()
            .sheet(id)
            .map(|sheet| sheet.name().to_owned())
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for Workbook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workbook")
            .field("sheets", &self.sheet_names())
            .finish()
    }
}
