//! Tables comparing runs.

use std::io::IsTerminal;

use crate::history::{Counters, Record, Workload};
use crate::system::format_timestamp;

/// Changes smaller than this fraction are shown as noise.
const NOISE: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Better {
    Higher,
    Lower,
    /// A fingerprint, such as the number of nodes searched: changes mean the search behaves
    /// differently, which is neither good nor bad by itself.
    Neither,
}

/// Reads one metric from a record; `None` when that run didn't measure it.
type Extract = Box<dyn Fn(&Record) -> Option<f64>>;

struct Metric {
    name: &'static str,
    better: Better,
    decimals: usize,
    value: Extract,
}

impl Metric {
    fn new(
        name: &'static str,
        better: Better,
        decimals: usize,
        value: impl Fn(&Record) -> Option<f64> + 'static,
    ) -> Self {
        Self {
            name,
            better,
            decimals,
            value: Box::new(value),
        }
    }
}

fn perft(r: &Record) -> Option<&Workload> {
    r.perft.as_ref()
}

fn search(r: &Record) -> Option<&Workload> {
    r.search.as_ref()
}

fn ratio(numerator: Option<u64>, denominator: Option<u64>) -> Option<f64> {
    match (numerator?, denominator?) {
        (_, 0) => None,
        (n, d) => Some(n as f64 / d as f64),
    }
}

/// Hardware counter metrics for the workload picked by `workload`, named: instructions per
/// node, instructions per cycle, cache misses per node, L1d miss rate, branch miss rate.
fn counter_metrics(
    names: [&'static str; 5],
    workload: fn(&Record) -> Option<&Workload>,
) -> [Metric; 5] {
    let counters = move |r: &Record| workload(r).and_then(|w| w.counters.clone());
    let per_node = move |r: &Record, f: fn(&Counters) -> Option<u64>| {
        let w = workload(r)?;
        ratio(f(w.counters.as_ref()?), Some(w.nodes))
    };
    [
        Metric::new(names[0], Better::Lower, 0, move |r| {
            per_node(r, |c| c.instructions)
        }),
        Metric::new(names[1], Better::Higher, 2, move |r| {
            let c = counters(r)?;
            ratio(c.instructions, c.cycles)
        }),
        Metric::new(names[2], Better::Lower, 3, move |r| {
            per_node(r, |c| c.cache_misses)
        }),
        Metric::new(names[3], Better::Lower, 2, move |r| {
            let c = counters(r)?;
            ratio(c.l1d_misses, c.l1d_loads).map(|x| x * 100.0)
        }),
        Metric::new(names[4], Better::Lower, 2, move |r| {
            let c = counters(r)?;
            ratio(c.branch_misses, c.branches).map(|x| x * 100.0)
        }),
    ]
}

fn timed_depth(r: &Record, millis: u64) -> Option<f64> {
    r.timed_depths
        .iter()
        .find(|t| t.millis == millis)
        .map(|t| t.average_depth)
}

fn metrics() -> Vec<Metric> {
    use Better::{Higher, Lower, Neither};
    let mut metrics = vec![
        Metric::new("perft speed (M nodes/s)", Higher, 2, |r| {
            perft(r).map(|w| w.nodes_per_second / 1e6)
        }),
        Metric::new("search speed (k nodes/s)", Higher, 0, |r| {
            search(r).map(|w| w.nodes_per_second / 1e3)
        }),
        Metric::new("search tree size (nodes)", Neither, 0, |r| {
            search(r).map(|w| w.nodes as f64)
        }),
        Metric::new("depth reached in 0.5 s", Higher, 2, |r| timed_depth(r, 500)),
        Metric::new("depth reached in 2 s", Higher, 2, |r| timed_depth(r, 2000)),
        Metric::new("perft allocations / node", Lower, 2, |r| {
            perft(r).map(|w| w.allocations_per_node)
        }),
        Metric::new("search allocations / node", Lower, 2, |r| {
            search(r).map(|w| w.allocations_per_node)
        }),
        Metric::new("search bytes allocated / node", Lower, 0, |r| {
            search(r).map(|w| w.bytes_per_node)
        }),
        Metric::new("search peak heap (KiB)", Lower, 1, |r| {
            search(r).map(|w| w.peak_heap_bytes as f64 / 1024.0)
        }),
        Metric::new("peak memory (MiB)", Lower, 1, |r| {
            Some(r.memory.peak_rss_kib as f64 / 1024.0).filter(|&m| m > 0.0)
        }),
    ];
    metrics.extend(counter_metrics(
        [
            "perft instructions / node",
            "perft instructions / cycle",
            "perft cache misses / node",
            "perft L1d miss rate %",
            "perft branch miss rate %",
        ],
        perft,
    ));
    metrics.extend(counter_metrics(
        [
            "search instructions / node",
            "search instructions / cycle",
            "search cache misses / node",
            "search L1d miss rate %",
            "search branch miss rate %",
        ],
        search,
    ));
    metrics
}

/// Metrics shown in the one-line-per-run history.
const HISTORY_COLUMNS: [(&str, &str); 6] = [
    ("perft speed (M nodes/s)", "perft Mn/s"),
    ("search speed (k nodes/s)", "search kn/s"),
    ("depth reached in 2 s", "depth 2s"),
    ("search allocations / node", "allocs/node"),
    ("search instructions / cycle", "IPC"),
    ("search cache misses / node", "cmiss/node"),
];

struct Style {
    color: bool,
}

impl Style {
    fn paint(&self, text: &str, code: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

fn describe(r: &Record) -> String {
    let dirty = if r.dirty {
        " (uncommitted changes)"
    } else {
        ""
    };
    format!(
        "{}{dirty}  {}  {}{}",
        r.commit,
        format_timestamp(r.timestamp),
        r.subject,
        if r.quick { "  [quick]" } else { "" }
    )
}

fn format_value(value: Option<f64>, decimals: usize) -> String {
    match value {
        Some(v) => format!("{v:.decimals$}"),
        None => "-".to_string(),
    }
}

/// The latest run against the previous one and the best so far.
pub(crate) fn compare(records: &[Record]) -> String {
    let style = Style {
        color: std::io::stdout().is_terminal(),
    };
    let Some(latest) = records.last() else {
        return "No results yet. Run `cargo run --release` in bench/ to record one.\n".into();
    };
    // Only runs over the same positions and depth can be compared.
    let comparable: Vec<&Record> = records
        .iter()
        .filter(|r| {
            r.suite_version == latest.suite_version && r.search_depth == latest.search_depth
        })
        .collect();
    let previous = comparable.iter().rev().nth(1).copied();

    let mut out = String::new();
    out += &format!("latest    {}\n", describe(latest));
    match previous {
        Some(p) => {
            out += &format!("previous  {}\n", describe(p));
            if p.machine.cpu != latest.machine.cpu {
                out += &style.paint("warning: different CPU; speeds are not comparable\n", "33");
            }
        }
        None => out += "previous  none with the same suite\n",
    }
    out += "\n";
    out += &format!(
        "{:<32}{:>12}{:>12}{:>10}{:>12}\n",
        "metric", "previous", "latest", "change", "best"
    );

    for metric in metrics() {
        let now = (metric.value)(latest);
        let before = previous.and_then(|p| (metric.value)(p));
        if now.is_none() && before.is_none() {
            continue;
        }
        let values = comparable.iter().filter_map(|r| (metric.value)(r));
        let best = match metric.better {
            Better::Higher => values.reduce(f64::max),
            Better::Lower => values.reduce(f64::min),
            Better::Neither => None,
        };
        let change = match (before, now) {
            (Some(b), Some(n)) if b != 0.0 => {
                let fraction = (n - b) / b.abs();
                let text = format!("{:+.1}%", fraction * 100.0);
                let improved = match metric.better {
                    Better::Higher => fraction > 0.0,
                    Better::Lower => fraction < 0.0,
                    Better::Neither => false,
                };
                let code = if fraction.abs() < NOISE {
                    "2"
                } else if metric.better == Better::Neither {
                    "33"
                } else if improved {
                    "32"
                } else {
                    "31"
                };
                // Pad before painting: escape codes would throw off the alignment.
                style.paint(&format!("{text:>10}"), code)
            }
            _ => format!("{:>10}", ""),
        };
        out += &format!(
            "{:<32}{:>12}{:>12}{change}{:>12}\n",
            metric.name,
            format_value(before, metric.decimals),
            format_value(now, metric.decimals),
            format_value(best, metric.decimals),
        );
    }
    out
}

/// One line per run, oldest first.
pub(crate) fn history(records: &[Record]) -> String {
    let metrics = metrics();
    let mut out = format!("{:<22}{:<10}", "date", "commit");
    for (_, short) in HISTORY_COLUMNS {
        out += &format!("{short:>13}");
    }
    out += "\n";
    for r in records {
        let commit = format!("{}{}", r.commit, if r.dirty { "+" } else { "" });
        out += &format!("{:<22}{commit:<10}", format_timestamp(r.timestamp));
        for (name, _) in HISTORY_COLUMNS {
            let metric = metrics
                .iter()
                .find(|m| m.name == name)
                .expect("known metric");
            out += &format!("{:>13}", format_value((metric.value)(r), metric.decimals));
        }
        out += "\n";
    }
    out += "(+ = uncommitted changes)\n";
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::TimedDepth;

    fn record(commit: &str, perft_nps: f64, allocs: f64) -> Record {
        Record {
            commit: commit.into(),
            suite_version: 1,
            search_depth: 4,
            perft: Some(Workload {
                nodes: 1000,
                nodes_per_second: perft_nps,
                allocations_per_node: allocs,
                counters: Some(Counters {
                    cycles: Some(2000),
                    instructions: Some(5000),
                    ..Counters::default()
                }),
                ..Workload::default()
            }),
            timed_depths: vec![TimedDepth {
                millis: 500,
                average_depth: 4.5,
            }],
            ..Record::default()
        }
    }

    fn row<'a>(table: &'a str, name: &str) -> &'a str {
        table
            .lines()
            .find(|l| l.starts_with(name))
            .unwrap_or_else(|| panic!("{name} in\n{table}"))
    }

    #[test]
    fn compares_latest_with_previous_and_best() {
        let records = [
            record("aaa", 3_000_000.0, 10.0),
            record("bbb", 2_000_000.0, 8.0),
            record("ccc", 2_200_000.0, 4.0),
        ];
        let table = compare(&records);
        assert!(table.contains("latest    ccc"), "{table}");
        assert!(table.contains("previous  bbb"), "{table}");
        let speed = row(&table, "perft speed");
        assert!(speed.contains("2.00") && speed.contains("2.20") && speed.contains("+10.0%"));
        assert!(
            speed.trim_end().ends_with("3.00"),
            "best is the fastest: {speed}"
        );
        let allocs = row(&table, "perft allocations");
        assert!(allocs.contains("-50.0%") && allocs.trim_end().ends_with("4.00"));
        let ipc = row(&table, "perft instructions / cycle");
        assert!(ipc.contains("2.50"));
        assert!(
            !table.contains("search speed"),
            "metrics nobody measured are hidden"
        );
    }

    #[test]
    fn other_suites_are_not_compared() {
        let mut old = record("aaa", 9_000_000.0, 1.0);
        old.suite_version = 0;
        let table = compare(&[old, record("bbb", 2_000_000.0, 8.0)]);
        assert!(table.contains("previous  none"), "{table}");
    }

    #[test]
    fn history_has_a_line_per_run() {
        let records = [
            record("aaa", 3_000_000.0, 10.0),
            record("bbb", 2_000_000.0, 8.0),
        ];
        let table = history(&records);
        assert_eq!(table.lines().count(), 4);
        assert!(table.lines().nth(2).unwrap().contains("bbb"));
    }

    #[test]
    fn empty_history() {
        assert!(compare(&[]).contains("No results yet"));
    }
}
