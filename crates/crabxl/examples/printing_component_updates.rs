//! Repeated margin updates keep the existing break vectors in their owned overlay.
#[path = "support/printing_updates.rs"]
mod support;
use crabxl::{PageMargins, PrintSettingsChange, Result};
fn main() -> Result<()> {
    support::run(|editor, index| {
        // The benchmark source uses default margins; real clients can borrow the
        // pending component after the first update without cloning other metadata.
        editor.update_print_settings(
            "Sheet",
            PrintSettingsChange::Margins(Some(PageMargins {
                left: index as f64 / 100.0,
                ..PageMargins::default()
            })),
        )
    })
}
