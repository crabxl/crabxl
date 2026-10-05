//! Equivalent repeated margin replacement through the snapshot API.
#[path = "support/printing_updates.rs"]
mod support;
use crabxl::{PrintSettings, Result};
fn main() -> Result<()> {
    let mut settings: Option<PrintSettings> = None;
    support::run(|editor, index| {
        if settings.is_none() {
            settings = Some(editor.print_settings("Sheet")?);
        }
        if let Some(settings) = &mut settings {
            if let Some(margins) = &mut settings.margins {
                margins.left = index as f64 / 100.0;
            }
            editor.set_print_settings("Sheet", settings.clone())?;
        }
        Ok(())
    })
}
