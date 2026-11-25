//! Source content that an edit or save cannot keep.
//!
//! Codecs call [`note`] when they read content that their typed model does
//! not represent. Code that rewrites a part from such a model reads the source
//! again under [`track`] and calls [`remove`] before it drops anything.
//! [`remove`] follows the [`DataLossPolicy`] of the running save.

use crabxl_core::{Error, ErrorKind, Result};
use std::cell::Cell;

/// What a save does when it cannot keep part of the source package.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataLossPolicy {
    /// Fail with [`ErrorKind::Unsupported`] and write nothing. The error
    /// message names the content.
    #[default]
    Reject,
    /// Remove the content and continue.
    Allow,
}

thread_local! {
    static NOTED: Cell<u64> = const { Cell::new(0) };
    static LAST: Cell<&'static str> = const { Cell::new("") };
    static POLICY: Cell<DataLossPolicy> = const { Cell::new(DataLossPolicy::Reject) };
}

/// Record source content that a codec read but does not model.
pub(crate) fn note(what: &'static str) {
    NOTED.with(|count| count.set(count.get().wrapping_add(1)));
    LAST.with(|last| last.set(what));
}

/// Run `read` and return the last unmodeled content that it noted, if any.
pub(crate) fn track<T>(read: impl FnOnce() -> Result<T>) -> Result<(T, Option<&'static str>)> {
    let before = NOTED.with(Cell::get);
    let value = read()?;
    let lossy = (NOTED.with(Cell::get) != before).then(|| LAST.with(Cell::get));
    Ok((value, lossy))
}

/// Ask the running save whether `what` may be removed from the package.
pub(crate) fn remove(what: impl std::fmt::Display) -> Result<()> {
    match POLICY.with(Cell::get) {
        DataLossPolicy::Allow => Ok(()),
        DataLossPolicy::Reject => Err(Error::new(
            ErrorKind::Unsupported,
            format!("Saving would remove {what}. Set DataLossPolicy::Allow to save without it."),
        )),
    }
}

/// Run `save` with `policy` for every [`remove`] call it makes.
pub(crate) fn with_policy<T>(policy: DataLossPolicy, save: impl FnOnce() -> T) -> T {
    struct Restore(DataLossPolicy);
    impl Drop for Restore {
        fn drop(&mut self) {
            POLICY.with(|current| current.set(self.0));
        }
    }
    let _restore = Restore(POLICY.with(|current| current.replace(policy)));
    save()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn track_reports_noted_content_and_policy_scopes_nest() {
        let (_, lossy) = track(|| Ok(())).unwrap();
        assert_eq!(lossy, None);
        let (_, lossy) = track(|| {
            note("an attribute");
            Ok(())
        })
        .unwrap();
        assert_eq!(lossy, Some("an attribute"));
        assert_eq!(remove("x").unwrap_err().kind(), ErrorKind::Unsupported);
        with_policy(DataLossPolicy::Allow, || {
            assert!(remove("x").is_ok());
            with_policy(DataLossPolicy::Reject, || assert!(remove("x").is_err()));
            assert!(remove("x").is_ok());
        });
        assert!(remove("x").is_err());
    }
}
