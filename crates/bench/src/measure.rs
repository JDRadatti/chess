//! Speed and memory measurements of move generation and search.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use game::engine::{Limits, search};
use game::{Game, Position, perft};

use crate::alloc;
use crate::history::Workload;
use crate::suite::{PERFT_CASES, SEARCH_DEPTH, SEARCH_POSITIONS, TIMED_MILLIS, TIMED_POSITIONS};

/// Counts the leaves of every perft case. Panics if a count is wrong: speed doesn't matter
/// when move generation is broken.
pub(crate) fn perft_pass() -> u64 {
    PERFT_CASES
        .iter()
        .map(|&(fen, depth, expected)| {
            let position: Position = fen.parse().expect("suite FENs are valid");
            let leaves = perft(&position, depth);
            assert_eq!(leaves, expected, "perft({depth}) is wrong for {fen}");
            leaves
        })
        .sum()
}

/// Searches every suite position to [`SEARCH_DEPTH`] and returns the total nodes visited.
pub(crate) fn search_pass() -> u64 {
    SEARCH_POSITIONS
        .iter()
        .map(|fen| {
            let limits = Limits {
                max_depth: SEARCH_DEPTH,
                time: None,
            };
            search_nodes(fen, &limits).1
        })
        .sum()
}

/// Deepest completed depth and nodes visited when searching `fen` within `limits`.
fn search_nodes(fen: &str, limits: &Limits) -> (u8, u64) {
    let game = Game::from_position(fen.parse().expect("suite FENs are valid"));
    let mut last = (0, 0);
    search(&game, limits, &AtomicBool::new(false), |info| {
        last = (info.depth, info.nodes);
    });
    last
}

/// Runs `pass` once counting allocations, then `repeats` more times on the clock, and reports
/// the median time. Every pass must visit the same number of nodes.
pub(crate) fn workload(pass: fn() -> u64, repeats: usize) -> Workload {
    let (nodes, allocs) = alloc::measure(pass);
    let mut times: Vec<f64> = (0..repeats.max(1))
        .map(|_| {
            let start = Instant::now();
            let timed_nodes = pass();
            let seconds = start.elapsed().as_secs_f64();
            assert_eq!(timed_nodes, nodes, "node count changed between runs");
            seconds
        })
        .collect();
    let seconds = median(&mut times);
    Workload {
        nodes,
        seconds,
        nodes_per_second: nodes as f64 / seconds,
        allocations_per_node: allocs.allocations as f64 / nodes as f64,
        bytes_per_node: allocs.bytes as f64 / nodes as f64,
        peak_heap_bytes: allocs.peak_bytes,
        counters: None,
    }
}

/// Average depth completed across [`TIMED_POSITIONS`] for each of [`TIMED_MILLIS`].
pub(crate) fn timed_depths() -> Vec<(u64, f64)> {
    TIMED_MILLIS
        .iter()
        .map(|&millis| {
            let limits = Limits {
                max_depth: 64,
                time: Some(Duration::from_millis(millis)),
            };
            let total: u32 = TIMED_POSITIONS
                .iter()
                .map(|fen| u32::from(search_nodes(fen, &limits).0))
                .sum();
            (millis, f64::from(total) / TIMED_POSITIONS.len() as f64)
        })
        .collect()
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        f64::midpoint(values[mid - 1], values[mid])
    } else {
        values[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::float_cmp, reason = "medians of small integers are exact")]
    fn median_of_odd_and_even_counts() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&mut [5.0]), 5.0);
    }
}
