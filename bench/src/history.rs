//! The results file: one JSON record per line, oldest first.
//!
//! Every field has a default, so records written before a field existed still load.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    /// Seconds since the Unix epoch.
    pub timestamp: u64,
    /// Short hash of the commit that was measured.
    pub commit: String,
    /// Whether there were uncommitted changes, so `commit` doesn't fully describe the code.
    pub dirty: bool,
    /// First line of the commit message.
    pub subject: String,
    /// Fewer repeats and no timed searches; speed numbers are noisier.
    pub quick: bool,
    /// Results are only comparable between runs with the same suite.
    pub suite_version: u32,
    pub search_depth: u8,
    pub machine: Machine,
    pub perft: Option<Workload>,
    pub search: Option<Workload>,
    pub timed_depths: Vec<TimedDepth>,
    pub memory: Memory,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Machine {
    pub cpu: String,
    pub rustc: String,
    /// CPU frequency governor and energy preference, which affect speed.
    pub governor: String,
    pub energy_preference: String,
    pub on_ac_power: Option<bool>,
}

/// One timed workload: perft or the fixed-depth search.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workload {
    pub nodes: u64,
    /// Median wall time of the timed runs.
    pub seconds: f64,
    pub nodes_per_second: f64,
    pub allocations_per_node: f64,
    pub bytes_per_node: f64,
    /// Most heap memory held at once during the run.
    pub peak_heap_bytes: u64,
    /// From `perf stat`, when hardware counters were available.
    pub counters: Option<Counters>,
}

/// Hardware performance counters for one run of a workload. Any may be missing if the CPU
/// or kernel doesn't support it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counters {
    pub cycles: Option<u64>,
    pub instructions: Option<u64>,
    pub cache_references: Option<u64>,
    pub cache_misses: Option<u64>,
    pub branches: Option<u64>,
    pub branch_misses: Option<u64>,
    pub l1d_loads: Option<u64>,
    pub l1d_misses: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimedDepth {
    pub millis: u64,
    /// Deepest completed depth, averaged over the timed positions.
    pub average_depth: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    /// Most resident memory the bench process used, in KiB.
    pub peak_rss_kib: u64,
    pub position_bytes: usize,
    pub move_bytes: usize,
}

/// All records in `path`, oldest first. A missing file is an empty history.
pub fn load(path: &Path) -> io::Result<Vec<Record>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}:{}: {e}", path.display(), i + 1),
                )
            })
        })
        .collect()
}

pub fn append(path: &Path, record: &Record) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(record)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_through_a_file() {
        let path = std::env::temp_dir().join(format!("bench-history-{}.jsonl", std::process::id()));
        let _ = fs::remove_file(&path);
        assert_eq!(load(&path).unwrap(), vec![]);

        let record = Record {
            commit: "abc1234".into(),
            perft: Some(Workload {
                nodes: 10,
                counters: Some(Counters {
                    cycles: Some(5),
                    ..Counters::default()
                }),
                ..Workload::default()
            }),
            ..Record::default()
        };
        append(&path, &record).unwrap();
        append(&path, &record).unwrap();
        assert_eq!(load(&path).unwrap(), vec![record.clone(), record]);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn old_records_missing_fields_still_load() {
        let record: Record = serde_json::from_str(r#"{"commit":"abc1234"}"#).unwrap();
        assert_eq!(record.commit, "abc1234");
        assert_eq!(record.perft, None);
    }
}
