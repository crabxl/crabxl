//! Metadata operations for the canonical Worksheet owner.
use super::*;

impl Worksheet {
    /// Optional explicit display settings; absent settings allocate nothing.
    pub fn sheet_views(&self) -> Option<&crate::SheetViews> {
        self.views.as_deref()
    }
    /// Replace display settings atomically within the sheet's retained allowance.
    pub fn set_sheet_views(&mut self, views: Option<crate::SheetViews>) -> Result<()> {
        if let Some(views) = &views {
            views.validate()?;
        }
        let charged = self
            .charged
            .saturating_sub(self.view_bytes())
            .saturating_add(views.as_ref().map_or(0, crate::SheetViews::memory_bytes));
        self.check(charged, self.len())?;
        self.views = views.map(Box::new);
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    pub(super) fn view_bytes(&self) -> usize {
        self.views.as_ref().map_or(0, |views| views.memory_bytes())
    }
    /// Optional explicit printing metadata.
    pub fn print_settings(&self) -> Option<&crate::PrintSettings> {
        self.printing.as_deref()
    }
    /// Atomically replace printing settings within the retained-data allowance.
    pub fn set_print_settings(&mut self, settings: Option<crate::PrintSettings>) -> Result<()> {
        if let Some(settings) = &settings {
            settings.validate()?;
        }
        let charged = self
            .charged
            .saturating_sub(self.print_bytes())
            .saturating_add(
                settings
                    .as_ref()
                    .map_or(0, crate::PrintSettings::memory_bytes),
            );
        self.check(charged, self.len())?;
        self.printing = settings.map(Box::new);
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    /// Update one printing component without copying unrelated break vectors.
    /// An unconfigured sheet installs canonical defaults only after admission.
    pub fn update_print_settings(&mut self, change: crate::PrintSettingsChange) -> Result<()> {
        let other = self.charged.saturating_sub(self.print_bytes());
        let maximum = self.limits.max_bytes.saturating_sub(other);
        if let Some(settings) = &mut self.printing {
            settings.update(change, maximum)?;
        } else {
            let mut settings = crate::PrintSettings::default();
            settings.update(change, maximum)?;
            self.printing = Some(Box::new(settings));
        }
        self.charged = other.saturating_add(self.print_bytes());
        self.dirty = true;
        Ok(())
    }
    pub(super) fn print_bytes(&self) -> usize {
        self.printing
            .as_ref()
            .map_or(0, |settings| settings.memory_bytes())
    }
}
