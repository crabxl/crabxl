//! Formula calculation for CrabXL workbooks, using the Formualizer engine.
//!
//! [`calculate`] evaluates every formula of a workbook and stores each result
//! as the formula's cached value, so the saved file shows values without
//! being opened in Excel first. [`Calculation`] takes the cells, tables and
//! names one by one, for workbooks that are not a [`Workbook`] model.
use crabxl_core::{
    Cell, CellAddress, CellError, CellRange, CellValue, DateEpoch, Error, ErrorKind, FormulaType,
    Result, SheetId, Workbook,
    features::{DefinedName, Table},
};
use formualizer_common::{DateSystem, ExcelError, ExcelErrorKind, LiteralValue};
use formualizer_eval::engine::{
    FormulaParsePolicy, TemporalEgress,
    named_range::{NameScope, NamedDefinition},
};
use formualizer_workbook::{Workbook as Engine, WorkbookConfig};

/// What [`calculate`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CalcStats {
    /// Formulas whose cached value was replaced.
    pub calculated: usize,
    /// Formulas left unchanged: array and data-table formulas, and results
    /// with no XLSX representation.
    pub skipped: usize,
}

fn failure(error: impl std::fmt::Display) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("Formula calculation failed: {error}"),
    )
}

/// Excel error codes that can be stored in a cell.
fn error_kind(code: &str) -> Option<ExcelErrorKind> {
    Some(match code {
        "#NULL!" => ExcelErrorKind::Null,
        "#REF!" => ExcelErrorKind::Ref,
        "#NAME?" => ExcelErrorKind::Name,
        "#VALUE!" => ExcelErrorKind::Value,
        "#DIV/0!" => ExcelErrorKind::Div,
        "#N/A" => ExcelErrorKind::Na,
        "#NUM!" => ExcelErrorKind::Num,
        "#SPILL!" => ExcelErrorKind::Spill,
        "#CALC!" => ExcelErrorKind::Calc,
        _ => return None,
    })
}

/// The engine input for a stored value; None for an empty cell.
fn literal(value: &CellValue) -> Result<Option<LiteralValue>> {
    Ok(Some(match value {
        CellValue::Number(number) => LiteralValue::Number(*number),
        CellValue::Integer(integer) => LiteralValue::Number(*integer as f64),
        CellValue::BigInteger(integer) => {
            LiteralValue::Number(integer.as_str().parse().map_err(failure)?)
        }
        CellValue::Boolean(boolean) => LiteralValue::Boolean(*boolean),
        CellValue::Text(text) => LiteralValue::Text(text.as_str().into()),
        CellValue::RichText(text) => LiteralValue::Text(text.plain_text()?.into()),
        CellValue::Error(error) => match error_kind(error.as_str()) {
            Some(kind) => LiteralValue::Error(ExcelError::new(kind)),
            None => return Ok(None),
        },
        CellValue::DateTime(date) => LiteralValue::Number(date.serial()),
        CellValue::Annotated(annotated) => return literal(annotated.value.unannotated()),
        _ => return Ok(None),
    }))
}

/// The cached value for an engine result; None when XLSX cannot store it.
fn cached(value: LiteralValue) -> Option<CellValue> {
    Some(match value {
        LiteralValue::Number(number) if number.is_finite() => CellValue::Number(number),
        LiteralValue::Number(_) => CellValue::Error(Box::new(CellError::new("#NUM!"))),
        LiteralValue::Int(integer) => CellValue::Integer(integer),
        LiteralValue::Boolean(boolean) => CellValue::Boolean(boolean),
        LiteralValue::Text(text) => CellValue::text(text),
        // A reference to an empty cell shows 0, as in Excel.
        LiteralValue::Empty => CellValue::Integer(0),
        LiteralValue::Error(error) => {
            let code = error.kind.to_string();
            error_kind(&code)?;
            CellValue::Error(Box::new(CellError::new(code)))
        }
        // A spilled result stores its top-left value in the formula cell.
        LiteralValue::Array(rows) => {
            return cached(rows.into_iter().next()?.into_iter().next()?);
        }
        _ => return None,
    })
}

/// A formula to calculate: its sheet position, address and expression.
struct Pending {
    sheet: usize,
    address: CellAddress,
    expression: Box<str>,
}

/// One calculation. Add sheets, then their cells, tables and names, then
/// [`run`](Self::run) it.
pub struct Calculation {
    engine: Engine,
    sheets: Vec<String>,
    names: Vec<DefinedName>,
    formulas: Vec<Pending>,
    skipped: usize,
}

impl Calculation {
    /// Start a calculation with this date system. `threads` limits the
    /// engine's threads; 1 calculates on the calling thread.
    pub fn new(epoch: DateEpoch, threads: usize) -> Self {
        let mut config = WorkbookConfig::ephemeral();
        config.eval.enable_parallel = threads > 1;
        config.eval.max_threads = Some(threads.max(1));
        config.eval.temporal_egress = TemporalEgress::Serial;
        // A malformed formula becomes an error value instead of failing all.
        config.eval.formula_parse_policy = FormulaParsePolicy::CoerceToError;
        config.eval.date_system = match epoch {
            DateEpoch::Mac1904 => DateSystem::Excel1904,
            _ => DateSystem::Excel1900,
        };
        Self {
            engine: Engine::new_with_config(config),
            sheets: Vec::new(),
            names: Vec::new(),
            formulas: Vec::new(),
            skipped: 0,
        }
    }

    /// Add the next sheet in workbook order and return its position.
    /// Positions match `localSheetId` of defined names.
    pub fn add_sheet(&mut self, name: &str) -> Result<usize> {
        self.engine.add_sheet(name).map_err(failure)?;
        self.sheets.push(name.to_owned());
        Ok(self.sheets.len() - 1)
    }

    /// Add one stored cell of a sheet. Array and data-table formulas are not
    /// calculated; their cells take part with no value.
    pub fn add_cell(&mut self, sheet: usize, cell: &Cell) -> Result<()> {
        let name = self.sheet_name(sheet)?;
        let value = match &cell.value {
            CellValue::Annotated(annotated) => annotated.value.unannotated(),
            value => value,
        };
        if let CellValue::Formula(formula) = value {
            if matches!(
                formula.formula_type(),
                FormulaType::Array | FormulaType::DataTable
            ) {
                self.skipped += 1;
            } else {
                self.formulas.push(Pending {
                    sheet,
                    address: cell.address,
                    expression: formula.expression().into(),
                });
            }
        } else if let Some(value) = literal(value)? {
            let (row, column) = position(cell.address);
            self.engine
                .set_value(&name, row, column, value)
                .map_err(failure)?;
        }
        Ok(())
    }

    /// Add a table of a sheet, so structured references to it resolve.
    pub fn add_table(&mut self, sheet: usize, table: &Table) -> Result<()> {
        let name = self.sheet_name(sheet)?;
        let range: CellRange = table.ref_range.parse()?;
        let (first_row, first_column) = position(range.start);
        let (last_row, last_column) = position(range.end);
        let headers = table
            .table_columns
            .iter()
            .map(|column| column.name.to_string())
            .collect();
        self.engine
            .define_table(
                &table.display_name,
                &name,
                (first_row, first_column, last_row, last_column),
                headers,
                table.header_row_count != Some(0),
                table.totals_row_count.is_some_and(|count| count > 0),
            )
            .map_err(failure)
    }

    /// Add a defined name. Names that are not formulas Excel calculates, such
    /// as print areas, are ignored.
    pub fn add_name(&mut self, name: &DefinedName) {
        self.names.push(name.clone());
    }

    fn sheet_name(&self, sheet: usize) -> Result<String> {
        self.sheets
            .get(sheet)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown calculation sheet"))
    }

    /// Calculate. Calls `result` with each formula's sheet position, address
    /// and cached value; None means XLSX cannot store the result.
    pub fn run(
        mut self,
        mut result: impl FnMut(usize, CellAddress, Option<CellValue>) -> Result<()>,
    ) -> Result<usize> {
        for defined in &self.names {
            let Ok(ast) = formualizer_parse::parse(format!("={}", defined.value)) else {
                continue;
            };
            let scope = match defined.local_sheet_id {
                Some(index) => match self
                    .sheets
                    .get(index as usize)
                    .and_then(|name| self.engine.engine().sheet_id(name))
                {
                    Some(id) => NameScope::Sheet(id),
                    None => continue,
                },
                None => NameScope::Workbook,
            };
            let definition = NamedDefinition::Formula {
                ast,
                dependencies: Vec::new(),
                range_deps: Vec::new(),
            };
            // A name the engine cannot use leaves its formulas at #NAME?.
            let _ = self
                .engine
                .engine_mut()
                .define_name(&defined.name, definition, scope);
        }
        // A formula the engine rejects, such as one that refers to itself or
        // to another workbook, gets no result; the others still calculate.
        let accepted: Vec<bool> = self
            .formulas
            .iter()
            .map(|formula| {
                let (row, column) = position(formula.address);
                self.engine
                    .set_formula(
                        &self.sheets[formula.sheet],
                        row,
                        column,
                        &format!("={}", formula.expression),
                    )
                    .is_ok()
            })
            .collect();
        self.engine.evaluate_all().map_err(failure)?;
        for (formula, accepted) in self.formulas.iter().zip(accepted) {
            let (row, column) = position(formula.address);
            let value = accepted
                .then(|| {
                    self.engine
                        .get_value(&self.sheets[formula.sheet], row, column)
                })
                .flatten()
                .and_then(cached);
            result(formula.sheet, formula.address, value)?;
        }
        Ok(self.skipped)
    }
}

/// One-based engine coordinates.
fn position(address: CellAddress) -> (u32, u32) {
    (address.row.get() + 1, address.column.get() + 1)
}

/// Calculate every formula of `book` and store each result as the formula's
/// cached value. `threads` limits the engine's threads; 1 calculates on the
/// calling thread.
pub fn calculate(book: &mut Workbook, threads: usize) -> Result<CalcStats> {
    let mut calculation = Calculation::new(book.epoch(), threads);
    let ids: Vec<SheetId> = book.sheets().map(|(id, _)| id).collect();
    for (id, sheet) in book.sheets() {
        let position = calculation.add_sheet(sheet.name())?;
        debug_assert_eq!(ids[position], id);
        for cell in sheet.cells() {
            calculation.add_cell(position, cell)?;
        }
        for table in sheet
            .features()
            .map_or(&[][..], |features| &features.tables)
        {
            calculation.add_table(position, table)?;
        }
    }
    for name in book
        .features()
        .map_or(&[][..], |features| &features.defined_names)
    {
        calculation.add_name(name);
    }
    let mut results = Vec::new();
    let skipped = calculation.run(|sheet, address, value| {
        results.push((sheet, address, value));
        Ok(())
    })?;
    let mut stats = CalcStats {
        calculated: 0,
        skipped,
    };
    for (position, address, value) in results {
        let Some(value) = value else {
            stats.skipped += 1;
            continue;
        };
        let sheet = book.sheet(ids[position])?;
        let Some(mut cell) = sheet.cells().find(|cell| cell.address == address).cloned() else {
            continue;
        };
        let formula = match &mut cell.value {
            CellValue::Formula(formula) => formula,
            CellValue::Annotated(annotated) => match annotated.value.unannotated_mut() {
                CellValue::Formula(formula) => formula,
                _ => continue,
            },
            _ => continue,
        };
        formula.set_cached(Some(value))?;
        book.sheet_mut(ids[position])?.set(cell)?;
        stats.calculated += 1;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crabxl_core::{Formula, StyleId, WorkbookLimits};

    fn set(book: &mut Workbook, id: SheetId, address: &str, value: CellValue) {
        let address: CellAddress = address.parse().unwrap();
        book.sheet_mut(id)
            .unwrap()
            .set(Cell {
                address,
                value,
                style: StyleId::new(0),
            })
            .unwrap();
    }

    fn formula(expression: &str) -> CellValue {
        CellValue::Formula(Box::new(Formula::new(expression, None).unwrap()))
    }

    fn cache(book: &Workbook, id: SheetId, address: &str) -> Option<CellValue> {
        let address: CellAddress = address.parse().unwrap();
        let sheet = book.sheet(id).unwrap();
        let cell = sheet.cells().find(|cell| cell.address == address).unwrap();
        match &cell.value {
            CellValue::Formula(formula) => formula.cached().cloned(),
            _ => None,
        }
    }

    #[test]
    fn formulas_get_their_results_as_cached_values() {
        let mut book = Workbook::new(WorkbookLimits::default()).unwrap();
        let data = book.create_sheet("Data").unwrap();
        let sums = book.create_sheet("Sums").unwrap();
        set(&mut book, data, "A1", CellValue::Integer(2));
        set(&mut book, data, "A2", CellValue::Number(3.5));
        set(&mut book, data, "B1", CellValue::text("x"));
        set(&mut book, sums, "A1", formula("=SUM(Data!A1:A2)"));
        set(&mut book, sums, "A2", formula("A1*2"));
        set(&mut book, sums, "A3", formula("Data!B1&\"y\""));
        set(&mut book, sums, "A4", formula("1/0"));
        set(&mut book, sums, "A5", formula("Data!C1"));

        let stats = calculate(&mut book, 1).unwrap();

        assert_eq!(
            stats,
            CalcStats {
                calculated: 5,
                skipped: 0
            }
        );
        assert_eq!(cache(&book, sums, "A1"), Some(CellValue::Number(5.5)));
        assert_eq!(cache(&book, sums, "A2"), Some(CellValue::Number(11.0)));
        assert_eq!(cache(&book, sums, "A3"), Some(CellValue::text("xy")));
        assert_eq!(
            cache(&book, sums, "A4"),
            Some(CellValue::Error(Box::new(CellError::new("#DIV/0!"))))
        );
        assert_eq!(cache(&book, sums, "A5"), Some(CellValue::Number(0.0)));
    }
}
