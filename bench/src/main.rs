//! Measures the chess library's speed and memory use and keeps a history of the results.

mod alloc;
mod counters;
mod elo;
mod history;
mod measure;
mod rate;
mod ratings;
mod report;
mod suite;
mod system;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use game::{Move, Position};

use history::{Memory, Record, TimedDepth};
use ratings::RatingRun;

#[global_allocator]
static ALLOCATOR: alloc::CountingAllocator = alloc::CountingAllocator;

const USAGE: &str = "\
usage: cargo run --release -- [command] [options]

commands:
  run       measure, append the result to history.jsonl, and compare (default)
  report    compare the latest result with the previous one and the best
  history   one line per recorded result
  rate      play our agents against each other and Stockfish with fastchess, fit Elo
            ratings, and append them to ratings.jsonl (about an hour)
  ratings   the latest ratings and one line per rating run

run options:
  --quick     one timed repeat and no timed searches: faster but noisier
  --sudo      run perf under sudo, for hardware counters when the kernel blocks them
  --no-save   don't append the result to history.jsonl

rate options:
  --quick             6 games per pairing instead of 30 (about 15 minutes, wide error bars)
  --games N           games per pairing
  --concurrency N     games at a time (default: physical cores minus one)
  --clock TC          fastchess time control for Full and Stockfish (default: 10+0.1)
  --stockfish PATH    Stockfish binary (default: `stockfish` on PATH)
  --no-save           don't append the result to ratings.jsonl
";

struct RunOptions {
    quick: bool,
    sudo: bool,
    save: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["workload", name] => workload(name),
        ["report"] => show(report::compare),
        ["history"] => show(report::history),
        ["ratings"] => match history::load(&ratings_file()) {
            Ok(runs) => {
                print!("{}", ratings::report(&runs));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("couldn't read the ratings: {e}");
                ExitCode::FAILURE
            }
        },
        ["rate", flags @ ..] => match parse_rate_options(flags) {
            Ok(options) => rate(&options),
            Err(e) => {
                eprint!("{e}\n\n{USAGE}");
                ExitCode::FAILURE
            }
        },
        ["help" | "--help" | "-h"] => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        rest => {
            let flags = rest.strip_prefix(&["run"]).unwrap_or(rest);
            let mut options = RunOptions {
                quick: false,
                sudo: false,
                save: true,
            };
            for flag in flags {
                match *flag {
                    "--quick" => options.quick = true,
                    "--sudo" => options.sudo = true,
                    "--no-save" => options.save = false,
                    _ => {
                        eprint!("unknown argument `{flag}`\n\n{USAGE}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            run(&options)
        }
    }
}

fn history_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("history.jsonl")
}

fn ratings_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ratings.jsonl")
}

fn git_state() -> (String, bool, String) {
    system::git_state(&repo_root(), &[history_file(), ratings_file()])
}

fn repo_root() -> PathBuf {
    let bench_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    bench_dir.parent().unwrap_or(bench_dir).to_path_buf()
}

/// One pass of a workload, for `perf stat` to measure. Prints the node count.
fn workload(name: &str) -> ExitCode {
    let nodes = match name {
        "perft" => measure::perft_pass(),
        "search" => measure::search_pass(),
        _ => {
            eprintln!("unknown workload `{name}`");
            return ExitCode::FAILURE;
        }
    };
    println!("{nodes}");
    ExitCode::SUCCESS
}

fn show(format: fn(&[Record]) -> String) -> ExitCode {
    match history::load(&history_file()) {
        Ok(records) => {
            print!("{}", format(&records));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("couldn't read the history: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: &RunOptions) -> ExitCode {
    if cfg!(debug_assertions) {
        eprintln!("warning: unoptimized build; use `cargo run --release` for real numbers");
    }
    let history_file = history_file();
    let (commit, dirty, subject) = git_state();
    let machine = system::machine();
    if machine.on_ac_power == Some(false) {
        eprintln!("warning: running on battery; speeds will be lower and noisier");
    }
    eprintln!(
        "measuring {commit}{}; this takes {}",
        if dirty {
            " with uncommitted changes"
        } else {
            ""
        },
        if options.quick {
            "a few seconds"
        } else {
            "about 20 seconds"
        }
    );

    let mut sudo = options.sudo;
    if sudo {
        eprintln!("perf needs sudo for hardware counters:");
        if let Err(reason) = counters::authorize_sudo() {
            eprintln!("warning: {reason}; continuing without sudo");
            sudo = false;
        }
    }

    let repeats = if options.quick { 1 } else { 3 };
    eprint!("  perft ...");
    let mut perft = measure::workload(measure::perft_pass, repeats);
    eprintln!(" {:.2} M nodes/s", perft.nodes_per_second / 1e6);
    eprint!("  search ...");
    let mut search = measure::workload(measure::search_pass, repeats);
    eprintln!(" {:.0} k nodes/s", search.nodes_per_second / 1e3);

    let timed_depths = if options.quick {
        vec![]
    } else {
        eprint!("  timed searches ...");
        let depths: Vec<TimedDepth> = measure::timed_depths()
            .into_iter()
            .map(|(millis, average_depth)| TimedDepth {
                millis,
                average_depth,
            })
            .collect();
        eprintln!(
            " {}",
            depths
                .iter()
                .map(|t| format!("depth {:.2} in {} ms", t.average_depth, t.millis))
                .collect::<Vec<_>>()
                .join(", ")
        );
        depths
    };

    eprint!("  hardware counters ...");
    for (name, workload) in [("perft", &mut perft), ("search", &mut search)] {
        match counters::measure(name, sudo) {
            Ok((counters, nodes)) => {
                if nodes != workload.nodes {
                    eprintln!(" skipped: {name} visited {nodes} nodes under perf");
                    break;
                }
                workload.counters = Some(counters);
            }
            Err(reason) => {
                eprint!(" skipped: {reason}");
                break;
            }
        }
    }
    eprintln!();

    let record = Record {
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        commit,
        dirty,
        subject,
        quick: options.quick,
        suite_version: suite::SUITE_VERSION,
        search_depth: suite::SEARCH_DEPTH,
        machine,
        perft: Some(perft),
        search: Some(search),
        timed_depths,
        memory: Memory {
            peak_rss_kib: system::peak_rss_kib(),
            position_bytes: size_of::<Position>(),
            move_bytes: size_of::<Move>(),
        },
    };

    let mut records = match history::load(&history_file) {
        Ok(records) => records,
        Err(e) => {
            eprintln!("couldn't read the history: {e}");
            return ExitCode::FAILURE;
        }
    };
    if options.save {
        if let Err(e) = history::append(&history_file, &record) {
            eprintln!("couldn't save the result: {e}");
            return ExitCode::FAILURE;
        }
        eprintln!("saved to {}", history_file.display());
    }
    records.push(record);
    println!();
    print!("{}", report::compare(&records));
    ExitCode::SUCCESS
}

fn parse_rate_options(flags: &[&str]) -> Result<rate::Options, String> {
    let mut options = rate::Options {
        quick: false,
        games: None,
        concurrency: None,
        clock: rate::DEFAULT_CLOCK.to_string(),
        stockfish: None,
        save: true,
    };
    let mut flags = flags.iter();
    while let Some(&flag) = flags.next() {
        let mut value = || {
            flags
                .next()
                .copied()
                .ok_or_else(|| format!("`{flag}` needs a value"))
        };
        let number = |text: &str| {
            text.parse::<u32>()
                .map_err(|_| format!("`{flag}` needs a number, not `{text}`"))
        };
        match flag {
            "--quick" => options.quick = true,
            "--no-save" => options.save = false,
            "--games" => options.games = Some(number(value()?)?),
            "--concurrency" => options.concurrency = Some(number(value()?)? as usize),
            "--clock" => options.clock = value()?.to_string(),
            "--stockfish" => options.stockfish = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown argument `{flag}`")),
        }
    }
    Ok(options)
}

fn rate(options: &rate::Options) -> ExitCode {
    if cfg!(debug_assertions) {
        eprintln!("warning: unoptimized build; use `cargo run --release`");
    }
    let ratings_file = ratings_file();
    let mut runs: Vec<RatingRun> = match history::load(&ratings_file) {
        Ok(runs) => runs,
        Err(e) => {
            eprintln!("couldn't read the ratings: {e}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = match rate::run(options, &repo_root(), &runs, git_state()) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("games saved to {}", outcome.pgn.display());
    if options.save {
        if let Err(e) = history::append(&ratings_file, &outcome.run) {
            eprintln!("couldn't save the ratings: {e}");
            return ExitCode::FAILURE;
        }
        eprintln!("saved to {}", ratings_file.display());
    }
    runs.push(outcome.run);
    println!();
    print!("{}", ratings::report(&runs));
    ExitCode::SUCCESS
}
