//! Facts about the code and machine being measured.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::history::Machine;

/// Commit hash, whether the tree has uncommitted changes, and the commit subject.
pub fn git_state(repo: &Path, history_file: &Path) -> (String, bool, String) {
    let git = |args: &[&str]| -> String {
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let commit = git(&["rev-parse", "--short", "HEAD"]);
    let subject = git(&["log", "-1", "--format=%s"]);
    // Adding a result to the history shouldn't make the next run look dirty.
    let exclude = format!(
        ":(exclude){}",
        history_file
            .strip_prefix(repo)
            .unwrap_or(history_file)
            .display()
    );
    let dirty = !git(&["status", "--porcelain", "--", ".", &exclude]).is_empty();
    (commit, dirty, subject)
}

pub fn machine() -> Machine {
    let read = |path: &str| {
        fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let cpu = read("/proc/cpuinfo")
        .lines()
        .find_map(|line| line.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, name)| name.trim().to_string())
        .unwrap_or_default();
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Machine {
        cpu,
        rustc,
        governor: read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
        energy_preference: read(
            "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference",
        ),
        on_ac_power: on_ac_power(),
    }
}

/// Whether a mains power supply reports being plugged in; `None` if there is no such supply.
fn on_ac_power() -> Option<bool> {
    let supplies = fs::read_dir("/sys/class/power_supply").ok()?;
    let mut found = None;
    for supply in supplies.flatten() {
        let path = supply.path();
        if fs::read_to_string(path.join("type")).is_ok_and(|t| t.trim() == "Mains") {
            let online = fs::read_to_string(path.join("online")).is_ok_and(|o| o.trim() == "1");
            found = Some(found.unwrap_or(false) || online);
        }
    }
    found
}

/// Most resident memory this process has used, in KiB.
pub fn peak_rss_kib() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmHWM:"))
                .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        })
        .unwrap_or(0)
}

/// `2026-10-03 23:50 UTC` for a Unix timestamp.
pub fn format_timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let minutes_today = (seconds % 86_400) / 60;
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minutes_today / 60,
        minutes_today % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_format_as_utc_dates() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_timestamp(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(format_timestamp(1_791_078_600), "2026-10-04 01:50 UTC");
    }

    #[test]
    fn peak_rss_is_reported() {
        assert!(peak_rss_kib() > 0);
    }
}
