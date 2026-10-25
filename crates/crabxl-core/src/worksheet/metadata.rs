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
    /// Tables, filters, rules, comments, protection and header/footer.
    pub fn features(&self) -> Option<&crate::SheetFeatures> {
        self.features.as_deref()
    }
    /// Replace all worksheet features within the retained-data allowance.
    /// An empty value is stored as absent.
    pub fn set_features(&mut self, features: Option<crate::SheetFeatures>) -> Result<()> {
        let features = features.filter(|features| !features.is_empty());
        let charged = self
            .charged
            .saturating_sub(self.feature_bytes())
            .saturating_add(features.as_ref().map_or(0, |features| {
                size_of::<crate::SheetFeatures>() + features.heap_bytes()
            }));
        self.check(charged, self.len())?;
        self.features = features.map(Box::new);
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    /// Remove and return the worksheet features.
    pub fn take_features(&mut self) -> Option<crate::SheetFeatures> {
        let bytes = self.feature_bytes();
        let features = self.features.take().map(|features| *features);
        if features.is_some() {
            self.charged = self.charged.saturating_sub(bytes);
            self.dirty = true;
        }
        features
    }
    pub(super) fn feature_bytes(&self) -> usize {
        self.features.as_ref().map_or(0, |features| {
            size_of::<crate::SheetFeatures>() + features.heap_bytes()
        })
    }
    pub(super) fn print_bytes(&self) -> usize {
        self.printing
            .as_ref()
            .map_or(0, |settings| settings.memory_bytes())
    }
}
