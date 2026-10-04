use crate::{Rows, WorkbookReader};
use openrsxl_core::{
    AccessPattern, DecisionReason, Error, ErrorKind, MemoryPolicy, MemorySource, ReadDecision,
    ReadMode, ResourceLimits, Result, Row, SheetData,
};
use std::{
    fs,
    io::{Read, Seek},
    path::Path,
};

/// Data returned by an automatic numeric read operation.
pub enum ReadData<'a, R: Read + Seek> {
    /// A lazy row stream borrowing the workbook.
    Streaming(Box<Rows<'a, R>>),
    /// An owned numeric snapshot; dropping the workbook does not invalidate it.
    Materialized(SheetData),
}

/// An automatically selected read strategy and its inspectable diagnostics.
pub struct AdaptiveRead<'a, R: Read + Seek> {
    /// Returned rows or owned sheet data.
    pub data: ReadData<'a, R>,
    /// Budget, estimate, strategy, and fallback explanation.
    pub decision: ReadDecision,
}

impl<R: Read + Seek> WorkbookReader<R> {
    /// Select streaming or owned materialization for the requested access pattern.
    ///
    /// Scan always streams. Repeated access samples at most 128 rows, without
    /// retaining them or trusting dimensions. If an estimate fits, materialize
    /// within the data allowance; discard and reopen as streaming if later data
    /// exceeds it. Format/value errors propagate instead of becoming fallbacks.
    /// This policy overrides only the materialized-data allowance for this call;
    /// direct `rows`/`read_sheet` calls retain their explicit semantics and limits.
    /// Catalog/dependency overhead and caller-retained outputs are outside the
    /// operation budget. Availability is a snapshot, not a memory reservation.
    pub fn read_with_policy(
        &mut self,
        name: &str,
        access: AccessPattern,
        policy: MemoryPolicy,
    ) -> Result<AdaptiveRead<'_, R>> {
        let mut decision = memory_decision(policy, self.limits)?;
        if access == AccessPattern::RepeatedAccess {
            let estimate = self.estimate_sheet(name)?;
            decision.estimated_data_bytes = Some(estimate);
            if estimate <= decision.retained_data_bytes {
                match self.collect_sheet(name, decision.retained_data_bytes) {
                    Ok(sheet) => {
                        decision.mode = ReadMode::Materialized;
                        decision.reason = DecisionReason::SampleFits;
                        return Ok(AdaptiveRead {
                            data: ReadData::Materialized(sheet),
                            decision,
                        });
                    }
                    Err(error) if error.kind() == ErrorKind::MemoryBudgetExceeded => {
                        decision.reason = DecisionReason::ActualDataExceedsBudget;
                    }
                    Err(error) => return Err(error),
                }
            } else {
                decision.reason = DecisionReason::EstimateExceedsBudget;
            }
        }
        Ok(AdaptiveRead {
            data: ReadData::Streaming(Box::new(self.rows(name)?)),
            decision,
        })
    }

    fn estimate_sheet(&mut self, name: &str) -> Result<usize> {
        let part = self
            .sheets()
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?
            .part()
            .to_owned();
        let size = self
            .archive
            .by_name(&part)
            .map_err(|error| {
                Error::caused_by(ErrorKind::Archive, "Cannot inspect worksheet", error)
                    .with_part(&part)
            })?
            .size();
        let mut rows = self.rows(name)?;
        let start = rows.bytes_consumed();
        let mut weight = 0u128;
        for _ in 0..128 {
            let Some(row) = rows.next_row()? else {
                // Include up to twice the live row count for vector growth.
                return Ok(
                    usize::try_from(weight + size_of::<SheetData>() as u128).unwrap_or(usize::MAX)
                );
            };
            weight += (row.memory_bytes() + size_of::<Row>()) as u128;
        }
        let consumed = rows.bytes_consumed().saturating_sub(start).max(1);
        let estimate = weight
            .saturating_mul(u128::from(size))
            .div_ceil(u128::from(consumed))
            .saturating_mul(3)
            .div_ceil(2)
            + size_of::<SheetData>() as u128;
        Ok(usize::try_from(estimate).unwrap_or(usize::MAX))
    }
}

fn memory_decision(policy: MemoryPolicy, limits: ResourceLimits) -> Result<ReadDecision> {
    let (budget, available, source) = match policy {
        MemoryPolicy::Budget(bytes) => (bytes, None, MemorySource::ExplicitBudget),
        MemoryPolicy::Auto(auto) => {
            if auto.fraction_per_mille == 0 || auto.fraction_per_mille > 1000 {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Automatic memory fraction must be 1 through 1000",
                ));
            }
            let (available, source) = auto
                .available_bytes
                .map(|bytes| (bytes, MemorySource::CallerAvailability))
                .unwrap_or_else(detect_available);
            let headroom = auto.headroom_bytes.min(available / 2);
            let budget =
                u128::from(available - headroom) * u128::from(auto.fraction_per_mille) / 1000;
            let budget = usize::try_from(budget)
                .unwrap_or(usize::MAX)
                .min(auto.maximum_bytes.unwrap_or(usize::MAX));
            (budget, Some(available), source)
        }
    };
    let working = limits
        .input_buffer_bytes
        .checked_add(limits.max_xml_event_bytes.saturating_mul(2))
        .and_then(|bytes| bytes.checked_add(limits.max_cell_bytes))
        .and_then(|bytes| bytes.checked_add(limits.max_row_bytes))
        .and_then(|bytes| bytes.checked_add(64 * 1024))
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Working reserve overflows"))?;
    let retained = budget
        .checked_sub(working)
        .filter(|bytes| *bytes >= size_of::<SheetData>())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Operation budget cannot cover the configured working reserve",
            )
        })?;
    Ok(ReadDecision {
        mode: ReadMode::Streaming,
        budget_bytes: budget,
        working_reserve_bytes: working,
        retained_data_bytes: retained,
        estimated_data_bytes: None,
        available_bytes: available,
        memory_source: source,
        reason: DecisionReason::SequentialAccess,
    })
}

fn kib_field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let remainder = line.strip_prefix(key)?;
        remainder
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    })
}
fn finite_limit(text: &str, label: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        line.strip_prefix(label)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}
fn cgroup_headroom(directory: &Path) -> Option<u64> {
    let maximum = fs::read_to_string(directory.join("memory.max")).ok()?;
    if maximum.trim() == "max" {
        return Some(u64::MAX);
    }
    let maximum: u64 = maximum.trim().parse().ok()?;
    let current: u64 = fs::read_to_string(directory.join("memory.current"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(maximum.saturating_sub(current))
}
fn cgroup_available(root: &Path, membership: &str) -> Option<u64> {
    let relative = Path::new(membership.trim_start_matches('/'));
    if relative
        .components()
        .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    let mut directory = root.join(relative);
    let mut available = u64::MAX;
    let mut observed = false;
    loop {
        match fs::metadata(directory.join("memory.max")) {
            Ok(_) => {
                // A present but unreadable/malformed limit makes discovery incomplete.
                available = available.min(cgroup_headroom(&directory)?);
                observed = true;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        if directory == root || !directory.pop() {
            break;
        }
    }
    observed.then_some(available)
}
fn detect_available() -> (u64, MemorySource) {
    // Portable conservative fallback; Linux v1 and other OS probes are not implemented.
    const FALLBACK: u64 = 256 * 1024 * 1024;
    if !cfg!(target_os = "linux") {
        return (FALLBACK, MemorySource::ConservativeFallback);
    }
    let host = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| kib_field(&text, "MemAvailable:"));
    let membership = fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("0::").map(str::to_owned))
        });
    let Some(mut available) = host else {
        return (FALLBACK, MemorySource::ConservativeFallback);
    };
    let root = Path::new("/sys/fs/cgroup");
    let Some(membership) = membership else {
        return (available.min(FALLBACK), MemorySource::ConservativeFallback);
    };
    let Some(cgroup) = cgroup_available(root, &membership) else {
        return (available.min(FALLBACK), MemorySource::ConservativeFallback);
    };
    available = available.min(cgroup);
    if let (Ok(limits), Ok(status)) = (
        fs::read_to_string("/proc/self/limits"),
        fs::read_to_string("/proc/self/status"),
    ) {
        for (label, key) in [
            ("Max address space", "VmSize:"),
            ("Max data size", "VmData:"),
        ] {
            if let (Some(maximum), Some(current)) =
                (finite_limit(&limits, label), kib_field(&status, key))
            {
                available = available.min(maximum.saturating_sub(current));
            }
        }
    } else {
        return (available.min(FALLBACK), MemorySource::ConservativeFallback);
    }
    (available, MemorySource::Linux)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use openrsxl_core::AutoMemory;

    #[test]
    fn automatic_budget_obeys_availability_headroom_fraction_and_ceiling() {
        let options = AutoMemory {
            available_bytes: Some(32 * 1024 * 1024 * 1024),
            maximum_bytes: Some(2 * 1024 * 1024 * 1024),
            ..AutoMemory::default()
        };
        let decision = memory_decision(MemoryPolicy::Auto(options), ResourceLimits::default())
            .expect("Valid options");
        assert_eq!(decision.budget_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(decision.memory_source, MemorySource::CallerAvailability);
        let options = AutoMemory {
            available_bytes: Some(128 * 1024 * 1024),
            ..AutoMemory::default()
        };
        let decision = memory_decision(MemoryPolicy::Auto(options), ResourceLimits::default())
            .expect("Small host");
        assert_eq!(decision.budget_bytes, 16 * 1024 * 1024);
    }
    #[test]
    fn invalid_and_too_small_budgets_are_errors() {
        assert!(memory_decision(MemoryPolicy::Budget(1), ResourceLimits::default()).is_err());
        for fraction in [0, 1001] {
            let options = AutoMemory {
                fraction_per_mille: fraction,
                ..AutoMemory::default()
            };
            assert!(
                memory_decision(MemoryPolicy::Auto(options), ResourceLimits::default()).is_err()
            );
        }
    }
    #[test]
    fn linux_fields_and_process_limits_are_parsed_without_overflow() {
        assert_eq!(
            kib_field("MemAvailable: 123 kB\n", "MemAvailable:"),
            Some(125952)
        );
        assert_eq!(
            kib_field("MemAvailable: 18446744073709551615 kB", "MemAvailable:"),
            None
        );
        assert_eq!(
            finite_limit(
                "Max address space         12345       67890    bytes",
                "Max address space"
            ),
            Some(12345)
        );
        assert_eq!(
            finite_limit(
                "Max address space         unlimited unlimited bytes",
                "Max address space"
            ),
            None
        );
    }

    #[test]
    fn cgroup_hierarchy_uses_parent_headroom_and_rejects_incomplete_limits() {
        let root =
            std::env::temp_dir().join(format!("openrsxl-memory-test-{}", std::process::id()));
        let child = root.join("group/child");
        fs::create_dir_all(&child).expect("Create isolated test hierarchy");
        fs::write(root.join("memory.max"), "1000").expect("Root limit");
        fs::write(root.join("memory.current"), "900").expect("Root usage");
        fs::write(child.join("memory.max"), "500").expect("Child limit");
        fs::write(child.join("memory.current"), "100").expect("Child usage");
        assert_eq!(cgroup_available(&root, "/group/child"), Some(100));
        fs::write(child.join("memory.current"), "600").expect("Over limit");
        assert_eq!(cgroup_available(&root, "/group/child"), Some(0));
        fs::write(child.join("memory.max"), "max").expect("Unlimited child");
        assert_eq!(cgroup_available(&root, "/group/child"), Some(100));
        fs::write(child.join("memory.max"), "broken").expect("Malformed child");
        assert_eq!(cgroup_available(&root, "/group/child"), None);
        assert_eq!(cgroup_available(&root, "/../escape"), None);
        fs::remove_dir_all(root).expect("Clean test hierarchy");
    }
}
