// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The host fingerprint recorded beside every bench result.
//!
//! A speed number without the machine it was taken on is not comparable, so
//! the result carries what the standard library and the host's own procfs
//! disclose. Every field past the target triple is OPTIONAL, because the
//! engine reads no host beyond `std` and `/proc` and never spawns a process
//! to learn one: a field it cannot establish is absent rather than guessed.
//!
//! `std::thread::available_parallelism` is the documented source for the
//! parallelism the running process may actually use
//! (<https://doc.rust-lang.org/std/thread/fn.available_parallelism.html>),
//! which is the honest number under a container CPU quota.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What the engine could establish about the machine that offered the load.
///
/// This describes the LOAD GENERATOR's host, which is only the same machine
/// as the SUT when the operator put them there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentFingerprint {
    /// The target architecture (`std::env::consts::ARCH`).
    pub arch: String,
    /// The target operating system (`std::env::consts::OS`).
    pub os: String,
    /// The parallelism the process may use, when the platform discloses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_parallelism: Option<u32>,
    /// The CPU model string, when the host discloses one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_model: Option<String>,
    /// Total physical memory in bytes, when the host discloses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_memory_bytes: Option<u64>,
}

impl EnvironmentFingerprint {
    /// Reads the fingerprint of the machine this process runs on.
    #[must_use]
    pub fn detect() -> Self {
        Self {
            arch: std::env::consts::ARCH.to_owned(),
            os: std::env::consts::OS.to_owned(),
            available_parallelism: std::thread::available_parallelism()
                .ok()
                .and_then(|count| u32::try_from(count.get()).ok()),
            cpu_model: cpu_model(),
            total_memory_bytes: total_memory_bytes(),
        }
    }

    /// Whether two fingerprints describe the same generator host, which is
    /// the precondition a cross-result comparison states in its header.
    #[must_use]
    pub fn comparable_to(&self, other: &Self) -> bool {
        self == other
    }

    /// The fingerprint as an ordered label map, for a rendered header row.
    #[must_use]
    pub fn labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        let _replaced = labels.insert("arch".to_owned(), self.arch.clone());
        let _replaced = labels.insert("os".to_owned(), self.os.clone());
        if let Some(cores) = self.available_parallelism {
            let _replaced = labels.insert("available_parallelism".to_owned(), cores.to_string());
        }
        if let Some(model) = &self.cpu_model {
            let _replaced = labels.insert("cpu_model".to_owned(), model.clone());
        }
        if let Some(bytes) = self.total_memory_bytes {
            let _replaced = labels.insert("total_memory_bytes".to_owned(), bytes.to_string());
        }
        labels
    }
}

/// The CPU model from `/proc/cpuinfo`, where the host has one.
///
/// An absent or unreadable procfs is a host that legitimately does not
/// disclose the field, so it becomes `None` rather than an error.
fn cpu_model() -> Option<String> {
    cpu_model_in(&std::fs::read_to_string("/proc/cpuinfo").ok()?)
}

/// The CPU model a `/proc/cpuinfo` body discloses.
///
/// Split from the read so the parse is exercised on every host: only Linux
/// has a procfs, and a rule the engine applies to a recorded fingerprint must
/// not go unchecked on the machines this suite runs on.
fn cpu_model_in(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.trim() == "model name")
        })
        .map(|(_, value)| value.trim().to_owned())
        .filter(|model| !model.is_empty())
}

/// Total memory from `/proc/meminfo`, where the host has one.
///
/// `MemTotal` is reported in kibibytes, which this converts to bytes. An
/// absent or unreadable procfs becomes `None`, as in [`cpu_model`].
fn total_memory_bytes() -> Option<u64> {
    total_memory_bytes_in(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// The total memory a `/proc/meminfo` body discloses, in bytes.
///
/// Split from the read for the reason [`cpu_model_in`] gives.
fn total_memory_bytes_in(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kibibytes: u64 = line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())?;
    kibibytes.checked_mul(1024)
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test in the Book ch11 shape that also asserts; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// The two fields the standard library always answers are always present.
    #[test]
    fn the_target_triple_fields_are_always_present() {
        let fingerprint = EnvironmentFingerprint::detect();
        assert!(!fingerprint.arch.is_empty());
        assert!(!fingerprint.os.is_empty());
    }

    /// Detection is stable within a process, so a repeated run does not
    /// report a machine that changed underneath it.
    #[test]
    fn detection_is_stable() {
        assert_eq!(
            EnvironmentFingerprint::detect(),
            EnvironmentFingerprint::detect()
        );
    }

    /// A differing host is reported as not comparable, which is what the
    /// comparison header warns about.
    #[test]
    fn a_differing_host_is_not_comparable() -> Result<(), serde_json::Error> {
        let here = EnvironmentFingerprint::detect();
        let mut elsewhere = here.clone();
        elsewhere.arch = format!("{}-elsewhere", here.arch);
        assert!(here.comparable_to(&here));
        assert!(!here.comparable_to(&elsewhere));
        let text = serde_json::to_string(&here)?;
        let back: EnvironmentFingerprint = serde_json::from_str(&text)?;
        assert_eq!(here, back);
        Ok(())
    }

    /// The label map always names the two mandatory fields.
    #[test]
    fn the_label_map_names_the_mandatory_fields() {
        let labels = EnvironmentFingerprint::detect().labels();
        assert!(labels.contains_key("arch"));
        assert!(labels.contains_key("os"));
    }

    /// Every optional field the host disclosed reaches the label map, and one
    /// it did not disclose is absent rather than labelled with a guess.
    #[test]
    fn the_label_map_carries_exactly_what_the_host_disclosed() {
        let disclosed = EnvironmentFingerprint {
            arch: String::from("aarch64"),
            os: String::from("linux"),
            available_parallelism: Some(8),
            cpu_model: Some(String::from("Neoverse-N1")),
            total_memory_bytes: Some(16_777_216),
        };
        let labels = disclosed.labels();
        assert_eq!(
            labels.get("available_parallelism").map(String::as_str),
            Some("8")
        );
        assert_eq!(
            labels.get("cpu_model").map(String::as_str),
            Some("Neoverse-N1")
        );
        assert_eq!(
            labels.get("total_memory_bytes").map(String::as_str),
            Some("16777216")
        );

        let silent = EnvironmentFingerprint {
            available_parallelism: None,
            cpu_model: None,
            total_memory_bytes: None,
            ..disclosed
        };
        let labels = silent.labels();
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert!(!labels.contains_key("cpu_model"));
        assert!(!labels.contains_key("total_memory_bytes"));
        assert!(!labels.contains_key("available_parallelism"));
    }

    /// A `/proc/cpuinfo` body as Linux writes it: the model name is read from
    /// the first processor block, trimmed of the padding procfs puts there.
    #[test]
    fn the_cpu_model_is_read_from_a_procfs_body() {
        const CPUINFO: &str = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\ncache size\t: 49152 KB\n\nprocessor\t: 1\nmodel name\t: Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\n";
        assert_eq!(
            cpu_model_in(CPUINFO).as_deref(),
            Some("Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz")
        );
        // An aarch64 procfs discloses no `model name` at all, which is a host
        // that legitimately does not answer rather than an empty model.
        assert_eq!(
            cpu_model_in("processor\t: 0\nBogoMIPS\t: 50.00\nCPU part\t: 0xd0c\n"),
            None
        );
        assert_eq!(cpu_model_in("model name\t:   \n"), None);
        assert_eq!(cpu_model_in(""), None);
    }

    /// `MemTotal` is kibibytes, and the recorded field is bytes.
    #[test]
    fn the_total_memory_is_converted_from_kibibytes() {
        const MEMINFO: &str = "MemTotal:       16307176 kB\nMemFree:         9312604 kB\nBuffers:          123456 kB\n";
        assert_eq!(total_memory_bytes_in(MEMINFO), Some(16_307_176_u64 * 1024));
        // A body with no MemTotal line, and one whose value is not a number,
        // are both hosts that did not disclose the field.
        assert_eq!(total_memory_bytes_in("MemFree: 100 kB\n"), None);
        assert_eq!(total_memory_bytes_in("MemTotal:  unknown kB\n"), None);
        assert_eq!(total_memory_bytes_in("MemTotal:\n"), None);
        // The kibibyte-to-byte multiply is checked, so a nonsense value that
        // would overflow is absent rather than a wrapped number.
        assert_eq!(
            total_memory_bytes_in(&format!("MemTotal: {} kB\n", u64::MAX)),
            None
        );
    }
}
