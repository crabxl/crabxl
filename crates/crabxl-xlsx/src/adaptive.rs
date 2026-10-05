use crate::{Rows, WorkbookReader};
use crabxl_core::{
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
    /// This policy governs managed allocations for this call;
    /// direct `rows`/`read_sheet` calls retain their explicit semantics and limits.
    /// Managed package/style/theme/string catalogs, template tables and retained rows
    /// share the allowance. Dependency/allocator overhead and caller-retained outputs
    /// remain additional. Availability is a snapshot, not a memory reservation.
    pub fn read_with_policy(
        &mut self,
        name: &str,
        access: AccessPattern,
        policy: MemoryPolicy,
    ) -> Result<AdaptiveRead<'_, R>> {
        self.read_with_policy_options(name, crabxl_core::ReadOptions::default(), access, policy)
    }
    /// Compose projection, rich/cache/date/formula policies with the joint allocation policy.
    /// Sampling, materialization and fallback retain the same read options.
    pub fn read_with_policy_options(
        &mut self,
        name: &str,
        options: crabxl_core::ReadOptions,
        access: AccessPattern,
        policy: MemoryPolicy,
    ) -> Result<AdaptiveRead<'_, R>> {
        let mut decision = memory_decision(policy, self.limits)?;
        let retained_allowance = decision.retained_data_bytes;
        if access == AccessPattern::RepeatedAccess {
            let estimate = self.estimate_sheet(name, &options, retained_allowance)?;
            self.rebalance_strings_for_retained(estimate, retained_allowance)?;
            decision.catalog_bytes = self.policy_catalog_bytes();
            decision.cache_bytes = self.shared_cache_bytes();
            decision.retained_data_bytes = retained_allowance
                .checked_sub(decision.catalog_bytes)
                .ok_or_else(|| {
                Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Prepared catalogs exceed managed policy allowance",
                )
            })?;
            decision.estimated_data_bytes = Some(estimate);
            if estimate <= decision.retained_data_bytes {
                match self.collect_sheet_with_allowance(
                    name,
                    retained_allowance,
                    options.clone(),
                    Some(retained_allowance),
                ) {
                    Ok(sheet) => {
                        decision.catalog_bytes = self.policy_catalog_bytes();
                        decision.cache_bytes = self.shared_cache_bytes();
                        decision.retained_data_bytes =
                            retained_allowance.saturating_sub(decision.catalog_bytes);
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
        let stream = self.rows_with_allowance(name, options, Some(retained_allowance))?;
        decision.catalog_bytes = stream.policy_catalog_bytes();
        decision.cache_bytes = stream.shared_cache_bytes();
        decision.retained_data_bytes = retained_allowance
            .checked_sub(decision.catalog_bytes)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Prepared catalogs exceed managed policy allowance",
                )
            })?;
        Ok(AdaptiveRead {
            data: ReadData::Streaming(Box::new(stream)),
            decision,
        })
    }

    fn estimate_sheet(
        &mut self,
        name: &str,
        options: &crabxl_core::ReadOptions,
        allowance: usize,
    ) -> Result<usize> {
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
        let mut rows = self.rows_with_allowance(name, options.clone(), Some(allowance))?;
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
    let allowance = memory_allowance(policy, limits)?;
    Ok(ReadDecision {
        mode: ReadMode::Streaming,
        budget_bytes: allowance.budget_bytes,
        working_reserve_bytes: allowance.working_reserve_bytes,
        retained_data_bytes: allowance.retained_data_bytes,
        catalog_bytes: 0,
        cache_bytes: 0,
        estimated_data_bytes: None,
        available_bytes: allowance.available_bytes,
        memory_source: allowance.memory_source,
        reason: DecisionReason::SequentialAccess,
    })
}

/// Compute an allowance using the reader/editor's shared availability policy.
/// Reserves the configured parser components. Catalogs, dependencies, allocator
/// overhead and caller-retained data are additional; this is not a hard RSS cap.
pub fn memory_allowance(
    policy: MemoryPolicy,
    limits: ResourceLimits,
) -> Result<crabxl_core::MemoryAllowance> {
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
    Ok(crabxl_core::MemoryAllowance {
        budget_bytes: budget,
        working_reserve_bytes: working,
        retained_data_bytes: retained,
        available_bytes: available,
        memory_source: source,
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
#[derive(Clone, Copy)]
enum CgroupMemory {
    Unified,
    Legacy,
}
impl CgroupMemory {
    fn files(self) -> (&'static str, &'static str) {
        match self {
            Self::Unified => ("memory.max", "memory.current"),
            Self::Legacy => ("memory.limit_in_bytes", "memory.usage_in_bytes"),
        }
    }
}
fn cgroup_headroom(directory: &Path, version: CgroupMemory) -> Option<u64> {
    let (limit, usage) = version.files();
    let maximum = fs::read_to_string(directory.join(limit)).ok()?;
    if matches!(version, CgroupMemory::Unified) && maximum.trim() == "max" {
        return Some(u64::MAX);
    }
    let maximum: u64 = maximum.trim().parse().ok()?;
    let current: u64 = fs::read_to_string(directory.join(usage))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(maximum.saturating_sub(current))
}
#[cfg(test)]
fn cgroup_available(root: &Path, membership: &str) -> Option<u64> {
    cgroup_directory_available(root, membership, CgroupMemory::Unified)
}
fn cgroup_directory_available(root: &Path, membership: &str, version: CgroupMemory) -> Option<u64> {
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
        match fs::metadata(directory.join(version.files().0)) {
            Ok(_) => {
                // A present but unreadable/malformed limit makes discovery incomplete.
                available = available.min(cgroup_headroom(&directory, version)?);
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
// mountinfo escapes space, tab, newline and backslash in path fields.
fn mount_path(value: &str) -> Option<std::path::PathBuf> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut input = value.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'\\' {
            let digits = [input.next()?, input.next()?, input.next()?];
            bytes.push(match &digits {
                b"040" => b' ',
                b"011" => b'\t',
                b"012" => b'\n',
                b"134" => b'\\',
                _ => return None,
            });
        } else {
            bytes.push(byte);
        }
    }
    let value = String::from_utf8(bytes).ok()?;
    let path = Path::new(&value);
    (path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir)))
    .then(|| path.to_owned())
}

fn mounted_cgroup_available(membership: &str, mounts: &str) -> Option<u64> {
    let legacy = membership.lines().find_map(|line| {
        let mut fields = line.splitn(3, ':');
        fields.next()?;
        let controllers = fields.next()?;
        controllers
            .split(',')
            .any(|item| item == "memory")
            .then(|| fields.next())
            .flatten()
    });
    let (member, version) = if let Some(member) = legacy {
        (member, CgroupMemory::Legacy)
    } else {
        (
            membership
                .lines()
                .find_map(|line| line.strip_prefix("0::"))?,
            CgroupMemory::Unified,
        )
    };
    let member = Path::new(member);
    if !member.is_absolute()
        || member
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return None;
    }
    let mut available = u64::MAX;
    let mut observed = false;
    for line in mounts.lines() {
        let Some((details, filesystem)) = line.split_once(" - ") else {
            continue;
        };
        let mut filesystem = filesystem.split_whitespace();
        let Some(kind) = filesystem.next() else {
            continue;
        };
        filesystem.next();
        let options = filesystem.next().unwrap_or_default();
        let matches = match version {
            CgroupMemory::Unified => kind == "cgroup2",
            CgroupMemory::Legacy => {
                kind == "cgroup" && options.split(',').any(|item| item == "memory")
            }
        };
        if !matches {
            continue;
        }
        let mut fields = details.split_whitespace().skip(3);
        let root_field = fields.next()?;
        let directory = mount_path(fields.next()?)?;
        // A cgroup namespace can expose its mounted ancestor as /.. while
        // reporting process membership as /. Inspect the visible mount root;
        // never append or traverse the synthetic parent marker on the host FS.
        // Descendant constraints hidden by the namespace remain unobservable.
        let relative = if root_field == "/.." && member == Path::new("/") {
            String::new()
        } else {
            let root = mount_path(root_field)?;
            let Ok(relative) = member.strip_prefix(root) else {
                continue;
            };
            relative.to_str()?.to_owned()
        };
        available = available.min(cgroup_directory_available(&directory, &relative, version)?);
        observed = true;
    }
    observed.then_some(available)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn detect_non_linux() -> (u64, MemorySource) {
    // Refresh RAM only: no process enumeration, CPU sampling or retained cache.
    let mut system = sysinfo::System::new();
    system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::nothing().with_ram());
    native_host_availability(system.total_memory(), system.available_memory())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn detect_non_linux() -> (u64, MemorySource) {
    (256 * 1024 * 1024, MemorySource::ConservativeFallback)
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn native_host_availability(total: u64, available: u64) -> (u64, MemorySource) {
    if total == 0 || available > total {
        (256 * 1024 * 1024, MemorySource::ConservativeFallback)
    } else {
        // Zero is a valid observation under pressure, not a failed probe.
        (available, MemorySource::NativeHost)
    }
}

fn detect_available() -> (u64, MemorySource) {
    const FALLBACK: u64 = 256 * 1024 * 1024;
    if !cfg!(target_os = "linux") {
        return detect_non_linux();
    }
    let host = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| kib_field(&text, "MemAvailable:"));
    let cgroup = fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|membership| {
            let mounts = fs::read_to_string("/proc/self/mountinfo").ok()?;
            mounted_cgroup_available(&membership, &mounts)
        });
    let Some(mut available) = host else {
        return (FALLBACK, MemorySource::ConservativeFallback);
    };
    let Some(cgroup) = cgroup else {
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
    use crabxl_core::AutoMemory;

    #[test]
    fn native_host_probe_distinguishes_failure_from_memory_pressure() {
        assert_eq!(
            native_host_availability(1024, 512),
            (512, MemorySource::NativeHost)
        );
        assert_eq!(
            native_host_availability(1024, 0),
            (0, MemorySource::NativeHost)
        );
        for observation in [(0, 0), (1024, 1025)] {
            assert_eq!(
                native_host_availability(observation.0, observation.1),
                (256 * 1024 * 1024, MemorySource::ConservativeFallback)
            );
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn native_memory_probe_runs_on_supported_desktop_hosts() {
        let (available, source) = detect_available();
        assert_eq!(source, MemorySource::NativeHost);
        let mut system = sysinfo::System::new();
        system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::nothing().with_ram());
        assert!(available <= system.total_memory());
    }

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
        let root = std::env::temp_dir().join(format!("crabxl-memory-test-{}", std::process::id()));
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
    #[test]
    #[cfg(target_os = "linux")]
    fn legacy_mount_discovery_obeys_visible_parent_usage_and_hybrid_membership() {
        let temporary = tempfile::tempdir().expect("Isolated controller mount");
        let root = temporary.path();
        let child = root.join("child");
        fs::create_dir(&child).expect("Child controller");
        for (directory, limit, usage) in [(root, "1000", "950"), (child.as_path(), "600", "100")] {
            fs::write(directory.join("memory.limit_in_bytes"), limit).expect("Legacy limit");
            fs::write(directory.join("memory.usage_in_bytes"), usage).expect("Legacy usage");
        }
        let mounts = format!(
            "25 1 0:20 /tenant {} rw - cgroup cgroup rw,memory,other\n",
            root.display()
        );
        let membership = "0::/unified\n7:cpu,cpuacct:/other\n8:memory:/tenant/child\n";
        assert_eq!(mounted_cgroup_available(membership, &mounts), Some(50));
        fs::write(child.join("memory.usage_in_bytes"), "700").expect("Over limit");
        assert_eq!(mounted_cgroup_available(membership, &mounts), Some(0));
        fs::write(child.join("memory.limit_in_bytes"), "9223372036854771712")
            .expect("Legacy unlimited sentinel");
        assert_eq!(mounted_cgroup_available(membership, &mounts), Some(50));
        fs::remove_file(child.join("memory.usage_in_bytes")).expect("Remove usage");
        assert_eq!(mounted_cgroup_available(membership, &mounts), None);
        assert_eq!(
            mounted_cgroup_available("8:memory:/unrelated", &mounts),
            None
        );
        assert_eq!(
            mounted_cgroup_available("8:memory:/tenant/../escape", &mounts),
            None
        );
        assert_eq!(
            mounted_cgroup_available(
                "8:memory:/tenant/child",
                &mounts.replace("rw,memory,other", "rw,cpu")
            ),
            None
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn unified_mount_discovery_handles_bind_roots_escaped_paths_and_aliases() {
        let temporary = tempfile::tempdir().expect("Isolated mount");
        let root = temporary.path().join("memory mount");
        fs::create_dir(&root).expect("Mount with space");
        fs::write(root.join("memory.max"), "400").expect("Unified limit");
        fs::write(root.join("memory.current"), "150").expect("Unified usage");
        let directory = root.to_str().expect("UTF8 test path").replace(' ', "\\040");
        let mounts = format!("25 1 0:20 /tenant {directory} rw shared:2 - cgroup2 cgroup rw\n");
        assert_eq!(mounted_cgroup_available("0::/tenant", &mounts), Some(250));
        assert_eq!(mounted_cgroup_available("0::/tenant2", &mounts), None);
        assert_eq!(mounted_cgroup_available("0::/", &mounts), None);
        let namespace_mount = mounts.replace(" /tenant ", " /.. ");
        assert_eq!(
            mounted_cgroup_available("0::/", &namespace_mount),
            Some(250)
        );
        assert_eq!(
            mounted_cgroup_available("0::/tenant", &namespace_mount),
            None
        );
        assert_eq!(
            mounted_cgroup_available("0::/tenant", &(mounts.clone() + &mounts)),
            Some(250)
        );
        assert_eq!(
            mount_path("/a\\134b\\011c\\012d"),
            Some(std::path::PathBuf::from("/a\\b\tc\nd"))
        );
        for path in ["relative", "/a/../b", "/a\\999", "/a\\04", "/a\\000"] {
            assert_eq!(mount_path(path), None);
        }
        fs::write(root.join("memory.max"), "max").expect("Unlimited controller");
        assert_eq!(
            mounted_cgroup_available("0::/tenant", &mounts),
            Some(u64::MAX)
        );
        fs::write(root.join("memory.max"), "broken").expect("Invalid controller");
        assert_eq!(mounted_cgroup_available("0::/tenant", &mounts), None);
    }
}
