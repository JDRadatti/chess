//! The ratings file: one JSON record per `bench rate` run, oldest first, and its report.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::system::format_timestamp;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RatingRun {
    /// Seconds since the Unix epoch.
    pub timestamp: u64,
    pub commit: String,
    pub dirty: bool,
    pub subject: String,
    pub quick: bool,
    /// fastchess time control for clock-managed engines: Full and Stockfish. Ratings are only
    /// comparable between runs with the same clock.
    pub clock: String,
    /// fastchess time control for the levels, roomy enough that their own pace governs.
    pub level_clock: String,
    pub book: String,
    /// Stockfish's `id name`.
    pub stockfish: String,
    pub cpu: String,
    pub concurrency: usize,
    pub minutes: f64,
    /// Our agents, strongest first.
    pub agents: Vec<AgentRating>,
    pub pairings: Vec<Pairing>,
    /// Games that ended abnormally, such as on time or by an illegal move.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentRating {
    pub name: String,
    pub elo: f64,
    /// Half-width of the 95% interval.
    pub error: f64,
    pub games: u32,
    /// Fraction of points scored, counting draws as half.
    pub score: f64,
}

/// Games between two agents, from `a`'s point of view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Pairing {
    pub a: String,
    pub b: String,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

/// The latest run in detail, then one line per run.
pub(crate) fn report(runs: &[RatingRun]) -> String {
    let Some(latest) = runs.last() else {
        return "no ratings yet; run `cargo run --release -- rate`\n".to_string();
    };
    let mut out = details(latest);
    if runs.len() > 1 {
        out.push('\n');
        out.push_str(&history(runs));
    }
    out
}

fn details(run: &RatingRun) -> String {
    let mut out = String::new();
    let games: u32 = run
        .pairings
        .iter()
        .map(|p| p.wins + p.draws + p.losses)
        .sum();
    let _ = writeln!(
        out,
        "Ratings at {}{} ({}): {games} games in {:.0} min{}",
        run.commit,
        if run.dirty {
            " (uncommitted changes)"
        } else {
            ""
        },
        format_timestamp(run.timestamp),
        run.minutes,
        if run.quick { ", quick" } else { "" },
    );
    let _ = writeln!(
        out,
        "Full and {} on a {} clock; the levels at their own pace.",
        run.stockfish, run.clock
    );
    let _ = writeln!(
        out,
        "Elo is on the CCRL scale Stockfish's limited strength is calibrated to, not FIDE's or chess.com's."
    );
    out.push('\n');
    let _ = writeln!(
        out,
        "{:<10} {:>6} {:>6} {:>6} {:>6}",
        "agent", "Elo", "±95%", "games", "score"
    );
    for agent in &run.agents {
        let _ = writeln!(
            out,
            "{:<10} {:>6.0} {:>6} {:>6} {:>5.0}%",
            agent.name,
            agent.elo,
            format!("±{:.0}", agent.error),
            agent.games,
            agent.score * 100.0
        );
    }
    out.push('\n');
    let _ = writeln!(out, "pairings (wins-draws-losses for the first):");
    let width = run.pairings.iter().map(|p| p.a.len()).max().unwrap_or(0);
    for p in &run.pairings {
        let _ = writeln!(
            out,
            "  {:<width$} vs {:<15} {}-{}-{}",
            p.a, p.b, p.wins, p.draws, p.losses
        );
    }
    if !run.problems.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "problems:");
        for problem in &run.problems {
            let _ = writeln!(out, "  {problem}");
        }
    }
    out
}

fn history(runs: &[RatingRun]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for run in runs {
        for agent in &run.agents {
            if !names.contains(&agent.name.as_str()) {
                names.push(&agent.name);
            }
        }
    }
    let mut out = String::new();
    let _ = write!(out, "{:<17} {:<8} {:<8}", "date", "commit", "clock");
    for name in &names {
        let _ = write!(out, " {name:>9}");
    }
    out.push('\n');
    for run in runs {
        let commit = format!("{}{}", run.commit, if run.dirty { "+" } else { "" });
        let date = format_timestamp(run.timestamp);
        let _ = write!(
            out,
            "{:<17} {commit:<8} {:<8}",
            date.trim_end_matches(" UTC"),
            run.clock
        );
        for name in &names {
            let elo = run.agents.iter().find(|a| a.name == *name);
            let cell = elo.map_or("-".to_string(), |a| format!("{:.0}", a.elo));
            let _ = write!(out, " {cell:>9}");
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(commit: &str, hard: f64) -> RatingRun {
        RatingRun {
            commit: commit.into(),
            clock: "10+0.1".into(),
            stockfish: "Stockfish 16".into(),
            agents: vec![AgentRating {
                name: "Hard".into(),
                elo: hard,
                error: 50.0,
                games: 30,
                score: 0.5,
            }],
            pairings: vec![Pairing {
                a: "Hard".into(),
                b: "Stockfish 1500".into(),
                wins: 10,
                draws: 10,
                losses: 10,
            }],
            ..RatingRun::default()
        }
    }

    #[test]
    fn reports_the_latest_run_and_the_history() {
        let report = report(&[run("aaa1111", 1450.0), run("bbb2222", 1520.0)]);
        assert!(report.contains("Ratings at bbb2222"));
        assert!(
            report.contains("Hard         1520    ±50     30    50%"),
            "{report}"
        );
        assert!(
            report.contains("Hard vs Stockfish 1500  10-10-10"),
            "{report}"
        );
        assert!(report.contains("aaa1111"));
        assert!(
            report
                .lines()
                .any(|l| l.starts_with("1970-01-01") && l.ends_with("1450"))
        );
    }

    #[test]
    fn empty_history_explains_how_to_start() {
        assert!(report(&[]).contains("rate"));
    }
}
