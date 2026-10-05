//! Hardware performance counters (cycles, cache misses, ...) from `perf stat`.
//!
//! `perf` runs this same binary with `workload <name>`, which does one pass of the workload
//! and prints its node count. With `--sudo`, only that child runs as root: it writes no
//! files, so nothing in the repository ends up owned by root.

use std::fs;
use std::process::{Command, Stdio};

use crate::history::Counters;

const EVENTS: &str = "cycles,instructions,cache-references,cache-misses,branches,branch-misses,\
                      L1-dcache-loads,L1-dcache-load-misses";

/// Above this, the kernel won't let ordinary users read hardware counters.
const MAX_UNPRIVILEGED_PARANOIA: i32 = 2;

/// Asks for the sudo password now, so `perf` can run non-interactively later instead of
/// prompting in the middle of a run.
pub(crate) fn authorize_sudo() -> Result<(), String> {
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .map_err(|e| format!("couldn't run sudo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("sudo was refused".to_string())
    }
}

/// Counters for one pass of `workload`, and the node count it reported. With `sudo`, call
/// [`authorize_sudo`] first.
pub(crate) fn measure(workload: &str, sudo: bool) -> Result<(Counters, u64), String> {
    if !sudo {
        let paranoia = fs::read_to_string("/proc/sys/kernel/perf_event_paranoid")
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok());
        if let Some(level) = paranoia.filter(|&l| l > MAX_UNPRIVILEGED_PARANOIA) {
            return Err(format!(
                "kernel.perf_event_paranoid is {level}; rerun with --sudo, or allow it with \
                 `sudo sysctl kernel.perf_event_paranoid={MAX_UNPRIVILEGED_PARANOIA}`"
            ));
        }
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = if sudo {
        let mut c = Command::new("sudo");
        // Fail instead of prompting: the password was asked for up front.
        c.args(["-n", "perf"]);
        c
    } else {
        Command::new("perf")
    };
    command
        .args(["stat", "-x", ",", "-e", EVENTS, "--"])
        .arg(exe)
        .args(["workload", workload])
        .stdin(Stdio::null());
    let output = command
        .output()
        .map_err(|e| format!("couldn't run perf: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let reason = stderr.lines().last().unwrap_or("no output");
        return Err(format!("perf stat failed: {reason}"));
    }
    let nodes = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| "workload didn't report a node count".to_string())?;
    Ok((parse(&stderr), nodes))
}

/// Reads `perf stat -x,` output: one `value,unit,event,...` line per event.
fn parse(output: &str) -> Counters {
    let mut counters = Counters::default();
    for line in output.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        let [value, _unit, event, ..] = fields.as_slice() else {
            continue;
        };
        // "<not supported>" and "<not counted>" fail to parse and stay None.
        let Ok(value) = value.parse::<u64>() else {
            continue;
        };
        // Events may come back as "cycles:u" or "cpu_core/cycles/".
        let event = event.split(':').next().unwrap_or(event);
        let event = event
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(event);
        let slot = match event {
            "cycles" => &mut counters.cycles,
            "instructions" => &mut counters.instructions,
            "cache-references" => &mut counters.cache_references,
            "cache-misses" => &mut counters.cache_misses,
            "branches" => &mut counters.branches,
            "branch-misses" => &mut counters.branch_misses,
            "L1-dcache-loads" => &mut counters.l1d_loads,
            "L1-dcache-load-misses" => &mut counters.l1d_misses,
            _ => continue,
        };
        *slot = Some(value);
    }
    counters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_perf_stat_csv() {
        let output = "\
1234567,,cycles:u,1000000,100.00,,
2345678,,instructions:u,1000000,100.00,1.90,insn per cycle
<not supported>,,cache-references,0,100.00,,
42,,cpu_core/cache-misses/,1000000,100.00,,
10,,branch-misses,1000000,100.00,,
some unrelated line
";
        let counters = parse(output);
        assert_eq!(counters.cycles, Some(1_234_567));
        assert_eq!(counters.instructions, Some(2_345_678));
        assert_eq!(counters.cache_references, None);
        assert_eq!(counters.cache_misses, Some(42));
        assert_eq!(counters.branch_misses, Some(10));
        assert_eq!(counters.l1d_loads, None);
    }
}
