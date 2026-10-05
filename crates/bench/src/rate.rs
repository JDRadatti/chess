//! `bench rate`: plays our agents against each other and against Stockfish at fixed Elo
//! settings using fastchess, then fits ratings on the scale Stockfish's settings are
//! calibrated to.
//!
//! Every opening is played twice with colors swapped. Each such pair is its own fastchess
//! process, and a pool of workers runs them so all cores stay busy until the end.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use game::engine::Level;

use crate::elo;
use crate::ratings::{AgentRating, Pairing, RatingRun};

pub(crate) const DEFAULT_CLOCK: &str = "10+0.1";
/// Roomy enough that the levels' own time per move always governs.
const LEVEL_CLOCK: &str = "300+3";
const QUICK_GAMES: u32 = 6;
const STANDARD_GAMES: u32 = 30;
/// Each agent plays Stockfish set this far above and below its expected rating.
const ANCHOR_SPREAD: f64 = 200.0;
const FULL: &str = "Full";
/// A sample of `noob_3moves.epd` from github.com/official-stockfish/books: positions three moves
/// into the game, roughly balanced.
const BOOK: &str = "openings/noob_3moves.epd";

pub(crate) struct Options {
    pub quick: bool,
    pub games: Option<u32>,
    pub concurrency: Option<usize>,
    pub clock: String,
    pub stockfish: Option<PathBuf>,
    pub save: bool,
}

/// What `run` measured, plus where the games were saved.
pub(crate) struct Outcome {
    pub run: RatingRun,
    pub pgn: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Agent {
    /// A level name, or `FULL`.
    Ours(&'static str),
    Stockfish(u32),
}

impl Agent {
    fn name(self) -> String {
        match self {
            Agent::Ours(name) => name.to_string(),
            Agent::Stockfish(elo) => format!("Stockfish {elo}"),
        }
    }

    /// The name fastchess writes into the PGN; no spaces, to keep its arguments simple.
    fn tag(self) -> String {
        match self {
            Agent::Ours(name) => name.to_string(),
            Agent::Stockfish(elo) => format!("SF{elo}"),
        }
    }

    /// Rough relative cost of its games, so the slowest pairings start first.
    fn cost(self) -> u32 {
        match self {
            Agent::Ours("Hard") => 10,
            Agent::Ours("Medium") => 4,
            Agent::Ours(FULL) => 4,
            Agent::Ours(_) => 1,
            Agent::Stockfish(_) => 2,
        }
    }

    fn engine_args(self, tools: &Tools, clock: &str) -> Vec<String> {
        let mut args = vec!["-engine".to_string(), format!("name={}", self.tag())];
        match self {
            Agent::Ours(name) => {
                args.push(format!("cmd={}", tools.uci.display()));
                args.push(format!("option.Level={name}"));
                let tc = if name == FULL { clock } else { LEVEL_CLOCK };
                args.push(format!("tc={tc}"));
            }
            Agent::Stockfish(elo) => {
                args.push(format!("cmd={}", tools.stockfish.display()));
                args.extend([
                    "option.Threads=1".to_string(),
                    "option.Hash=16".to_string(),
                    "option.UCI_LimitStrength=true".to_string(),
                    format!("option.UCI_Elo={elo}"),
                    format!("tc={clock}"),
                ]);
            }
        }
        args
    }
}

fn our_agents() -> Vec<Agent> {
    Level::ALL
        .iter()
        .map(|level| Agent::Ours(level.name()))
        .chain([Agent::Ours(FULL)])
        .collect()
}

/// Where to put an agent's anchors before it has a rating.
fn first_guess(name: &str) -> f64 {
    match name {
        "Beginner" => 700.0,
        "Easy" => 1100.0,
        "Medium" => 1400.0,
        "Hard" => 1600.0,
        _ => 1500.0,
    }
}

struct Tools {
    fastchess: PathBuf,
    stockfish: PathBuf,
    stockfish_name: String,
    /// Range of Stockfish's `UCI_Elo` option.
    elo_range: (u32, u32),
    uci: PathBuf,
    book: PathBuf,
}

/// Games between two agents, from `a`'s point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tally {
    wins: u32,
    draws: u32,
    losses: u32,
}

struct Job {
    a: Agent,
    b: Agent,
    /// 1-based line in the book.
    opening: u32,
}

/// One finished game from a PGN.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PlayedGame {
    white: String,
    black: String,
    result: String,
    termination: String,
}

pub(crate) fn run(
    options: &Options,
    repo: &Path,
    previous: &[RatingRun],
    (commit, dirty, subject): (String, bool, String),
) -> Result<Outcome, String> {
    let started = Instant::now();
    let run_dir = repo.join("target/rate").join(unix_now().to_string());
    fs::create_dir_all(&run_dir).map_err(|e| format!("can't create {}: {e}", run_dir.display()))?;
    let tools = find_tools(options, repo, &run_dir)?;

    let latest: BTreeMap<String, f64> = previous
        .iter()
        .rev()
        .find(|run| run.clock == options.clock)
        .map(|run| run.agents.iter().map(|a| (a.name.clone(), a.elo)).collect())
        .unwrap_or_default();
    let pairings = schedule(&latest, tools.elo_range);
    let games_per_pairing = options
        .games
        .unwrap_or(if options.quick {
            QUICK_GAMES
        } else {
            STANDARD_GAMES
        })
        .max(2)
        / 2
        * 2;
    let mut jobs: Vec<Job> = pairings
        .iter()
        .flat_map(|&(a, b)| (1..=games_per_pairing / 2).map(move |opening| Job { a, b, opening }))
        .collect();
    jobs.sort_by_key(|job| std::cmp::Reverse(job.a.cost() + job.b.cost()));
    let total_games = jobs.len() * 2;
    let concurrency = options
        .concurrency
        .unwrap_or_else(default_concurrency)
        .max(1);

    eprintln!(
        "playing {total_games} games ({games_per_pairing} per pairing, {concurrency} at a time) \
         against {}",
        tools.stockfish_name
    );
    for (a, b) in &pairings {
        eprintln!("  {} vs {}", a.name(), b.name());
    }

    let games = play_all(jobs, &tools, &options.clock, &run_dir, concurrency, started)?;

    let pgn = run_dir.join("games.pgn");
    let mut all_pgn = String::new();
    let mut entries: Vec<_> = fs::read_dir(&run_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("job-"))
        })
        .collect();
    entries.sort();
    for path in entries {
        all_pgn.push_str(&fs::read_to_string(&path).unwrap_or_default());
        let _ = fs::remove_file(path);
    }
    let _ = fs::write(&pgn, all_pgn);
    let _ = fs::remove_file(run_dir.join("config.json"));
    let _ = fs::remove_file(&tools.uci);

    let mut run = rate(&pairings, &games)?;
    run.timestamp = unix_now();
    run.commit = commit;
    run.dirty = dirty;
    run.subject = subject;
    run.quick = options.quick;
    run.clock = options.clock.clone();
    run.level_clock = LEVEL_CLOCK.to_string();
    run.book = BOOK.to_string();
    run.stockfish = tools.stockfish_name.clone();
    run.cpu = crate::system::machine().cpu;
    run.concurrency = concurrency;
    run.minutes = started.elapsed().as_secs_f64() / 60.0;
    Ok(Outcome { run, pgn })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One game per physical core, keeping one core for fastchess and the rest of the system.
/// Only one engine in a game thinks at a time.
fn default_concurrency() -> usize {
    let threads = thread::available_parallelism().map_or(2, std::num::NonZero::get);
    (threads / 2).saturating_sub(1).max(1)
}

fn find_tools(options: &Options, repo: &Path, run_dir: &Path) -> Result<Tools, String> {
    let fastchess = find_on_path("fastchess").ok_or(
        "fastchess isn't on your PATH; build it from github.com/Disservin/fastchess and copy \
         the binary to ~/.local/bin",
    )?;
    let stockfish = match &options.stockfish {
        Some(path) => path.clone(),
        None => find_on_path("stockfish")
            .ok_or("stockfish isn't on your PATH; install it with `sudo apt install stockfish`")?,
    };
    let (stockfish_name, elo_range) = probe_stockfish(&stockfish)?;

    eprintln!("building the UCI engine ...");
    let status = Command::new("cargo")
        .args(["build", "--release", "--quiet", "-p", "uci"])
        .current_dir(repo)
        .status()
        .map_err(|e| format!("can't run cargo: {e}"))?;
    if !status.success() {
        return Err("building the UCI engine failed".to_string());
    }
    // A copy, so rebuilding during a long run doesn't change the engine mid-run.
    let uci = run_dir.join("uci");
    fs::copy(repo.join("target/release/uci"), &uci)
        .map_err(|e| format!("can't copy the UCI engine: {e}"))?;

    let book = repo.join("crates/bench").join(BOOK);
    if !book.is_file() {
        return Err(format!("missing opening book {}", book.display()));
    }
    Ok(Tools {
        fastchess,
        stockfish,
        stockfish_name,
        elo_range,
        uci,
        book,
    })
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Stockfish's name and the range of its `UCI_Elo` option.
fn probe_stockfish(path: &Path) -> Result<(String, (u32, u32)), String> {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("can't start {}: {e}", path.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"uci\nquit\n");
    }
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    let _ = child.wait();
    parse_stockfish_uci(&output)
        .ok_or_else(|| format!("{} doesn't look like Stockfish", path.display()))
}

fn parse_stockfish_uci(output: &str) -> Option<(String, (u32, u32))> {
    let name = output
        .lines()
        .find_map(|line| line.strip_prefix("id name "))?
        .trim()
        .to_string();
    let option = output
        .lines()
        .find(|line| line.starts_with("option name UCI_Elo "))?;
    let words: Vec<&str> = option.split_whitespace().collect();
    let after = |key: &str| -> Option<u32> {
        let i = words.iter().position(|&w| w == key)?;
        words.get(i + 1)?.parse().ok()
    };
    Some((name, (after("min")?, after("max")?)))
}

/// Neighbouring agents play each other, and each agent plays Stockfish just above and below
/// where it's expected to be, so every rating is tied to Stockfish's scale.
fn schedule(latest: &BTreeMap<String, f64>, (min, max): (u32, u32)) -> Vec<(Agent, Agent)> {
    let ours = our_agents();
    let mut pairings: Vec<(Agent, Agent)> = ours.windows(2).map(|w| (w[0], w[1])).collect();
    for &agent in &ours {
        let name = agent.name();
        let guess = latest
            .get(&name)
            .copied()
            .unwrap_or_else(|| first_guess(&name));
        let mut anchors: Vec<u32> = [guess - ANCHOR_SPREAD, guess + ANCHOR_SPREAD]
            .iter()
            .map(|elo| ((elo / 50.0).round() * 50.0).clamp(f64::from(min), f64::from(max)) as u32)
            .collect();
        anchors.dedup();
        pairings.extend(
            anchors
                .into_iter()
                .map(|elo| (agent, Agent::Stockfish(elo))),
        );
    }
    pairings
}

fn play_all(
    jobs: Vec<Job>,
    tools: &Tools,
    clock: &str,
    run_dir: &Path,
    concurrency: usize,
    started: Instant,
) -> Result<Vec<PlayedGame>, String> {
    let total = jobs.len() * 2;
    let queue = Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let (sender, receiver) = mpsc::channel::<Result<Vec<PlayedGame>, String>>();
    let mut games = Vec::new();
    let mut errors = Vec::new();
    thread::scope(|scope| {
        for _ in 0..concurrency {
            let sender = sender.clone();
            let queue = &queue;
            scope.spawn(move || {
                loop {
                    let next = queue.lock().unwrap().pop();
                    let Some((id, job)) = next else { break };
                    let _ = sender.send(play(&job, id, tools, clock, run_dir));
                }
            });
        }
        drop(sender);
        let live = std::io::stderr().is_terminal();
        let mut last_tenth = 0;
        for result in receiver {
            match result {
                Ok(played) => games.extend(played),
                Err(e) => errors.push(e),
            }
            let done = games.len();
            let elapsed = started.elapsed().as_secs();
            let line = format!(
                "  {done}/{total} games, {}:{:02} elapsed",
                elapsed / 60,
                elapsed % 60
            );
            if live {
                eprint!("\r{line}  ");
            } else if done * 10 / total.max(1) > last_tenth {
                last_tenth = done * 10 / total.max(1);
                eprintln!("{line}");
            }
        }
        if live {
            eprintln!();
        }
    });
    // Jobs were popped from the back; any order works for rating.
    if games.is_empty() {
        return Err(format!(
            "no games finished: {}",
            errors.first().map_or("unknown error", String::as_str)
        ));
    }
    for e in errors.iter().take(3) {
        eprintln!("warning: {e}");
    }
    Ok(games)
}

/// Plays one opening twice with colors swapped.
fn play(
    job: &Job,
    id: usize,
    tools: &Tools,
    clock: &str,
    run_dir: &Path,
) -> Result<Vec<PlayedGame>, String> {
    let pgn = run_dir.join(format!("job-{id:05}.pgn"));
    let output = Command::new(&tools.fastchess)
        .current_dir(run_dir)
        .args(job.a.engine_args(tools, clock))
        .args(job.b.engine_args(tools, clock))
        .args(["-rounds", "1", "-games", "2", "-concurrency", "1"])
        .arg("-openings")
        .arg(format!("file={}", tools.book.display()))
        .args(["format=epd", "order=sequential"])
        .arg(format!("start={}", job.opening))
        .arg("-pgnout")
        .arg(format!("file={}", pgn.display()))
        .args(["-resign", "movecount=3", "score=1000", "twosided=true"])
        .args(["-maxmoves", "200"])
        .args(["-autosaveinterval", "0", "-ratinginterval", "0"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("can't run fastchess: {e}"))?;
    let games = fs::read_to_string(&pgn)
        .map(|text| parse_pgn(&text))
        .unwrap_or_default();
    if !output.status.success() || games.is_empty() {
        let log = String::from_utf8_lossy(&output.stdout);
        let tail: Vec<&str> = log.lines().rev().take(5).collect();
        return Err(format!(
            "{} vs {} (opening {}): fastchess failed: {}",
            job.a.name(),
            job.b.name(),
            job.opening,
            tail.into_iter().rev().collect::<Vec<_>>().join(" / ")
        ));
    }
    Ok(games)
}

fn parse_pgn(text: &str) -> Vec<PlayedGame> {
    let mut games = Vec::new();
    let mut current: Option<PlayedGame> = None;
    for line in text.lines() {
        let Some(tag) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) else {
            continue;
        };
        let Some((key, value)) = tag.split_once(' ') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        if key == "Event" {
            games.extend(current.take());
            current = Some(PlayedGame::default());
        }
        let Some(game) = current.as_mut() else {
            continue;
        };
        match key {
            "White" => game.white = value,
            "Black" => game.black = value,
            "Result" => game.result = value,
            "Termination" => game.termination = value,
            _ => {}
        }
    }
    games.extend(current);
    games
}

/// Tallies the games and fits ratings with Stockfish's settings as fixed points.
fn rate(pairings: &[(Agent, Agent)], games: &[PlayedGame]) -> Result<RatingRun, String> {
    let mut tallies: Vec<Tally> = vec![
        Tally {
            wins: 0,
            draws: 0,
            losses: 0
        };
        pairings.len()
    ];
    let mut problems: BTreeMap<String, u32> = BTreeMap::new();
    for game in games {
        let Some((index, flipped)) = pairings.iter().enumerate().find_map(|(i, (a, b))| {
            if (a.tag(), b.tag()) == (game.white.clone(), game.black.clone()) {
                Some((i, false))
            } else if (b.tag(), a.tag()) == (game.white.clone(), game.black.clone()) {
                Some((i, true))
            } else {
                None
            }
        }) else {
            continue;
        };
        let white_points = match game.result.as_str() {
            "1-0" => 2,
            "1/2-1/2" => 1,
            "0-1" => 0,
            _ => {
                *problems
                    .entry(format!("{} vs {}: unfinished game", game.white, game.black))
                    .or_default() += 1;
                continue;
            }
        };
        let a_points = if flipped {
            2 - white_points
        } else {
            white_points
        };
        let tally = &mut tallies[index];
        match a_points {
            2 => tally.wins += 1,
            1 => tally.draws += 1,
            _ => tally.losses += 1,
        }
        if let Some(problem) = abnormal_ending(game) {
            *problems.entry(problem).or_default() += 1;
        }
    }

    let mut players: Vec<Agent> = Vec::new();
    for &(a, b) in pairings {
        for agent in [a, b] {
            if !players.contains(&agent) {
                players.push(agent);
            }
        }
    }
    let index_of = |agent: Agent| players.iter().position(|&p| p == agent).unwrap();
    let fixed: Vec<Option<f64>> = players
        .iter()
        .map(|p| match p {
            Agent::Stockfish(elo) => Some(f64::from(*elo)),
            Agent::Ours(_) => None,
        })
        .collect();
    let results: Vec<elo::Results> = pairings
        .iter()
        .zip(&tallies)
        .filter(|(_, t)| t.wins + t.draws + t.losses > 0)
        .map(|(&(a, b), t)| elo::Results {
            a: index_of(a),
            b: index_of(b),
            wins: t.wins,
            draws: t.draws,
            losses: t.losses,
        })
        .collect();
    let fit = elo::fit(&fixed, &results)
        .ok_or("some agent has no games connecting it to Stockfish, so it can't be rated")?;

    let mut agents: Vec<AgentRating> = players
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p, Agent::Ours(_)))
        .map(|(i, &agent)| {
            let (mut games, mut points) = (0, 0.0);
            for (&(a, b), t) in pairings.iter().zip(&tallies) {
                let n = t.wins + t.draws + t.losses;
                let won = f64::from(t.wins) + f64::from(t.draws) / 2.0;
                if a == agent {
                    games += n;
                    points += won;
                } else if b == agent {
                    games += n;
                    points += f64::from(n) - won;
                }
            }
            AgentRating {
                name: agent.name(),
                elo: fit.ratings[i],
                error: fit.errors[i],
                games,
                score: if games > 0 {
                    points / f64::from(games)
                } else {
                    0.0
                },
            }
        })
        .collect();
    agents.sort_by(|x, y| y.elo.total_cmp(&x.elo));

    Ok(RatingRun {
        agents,
        pairings: pairings
            .iter()
            .zip(&tallies)
            .map(|(&(a, b), t)| Pairing {
                a: a.name(),
                b: b.name(),
                wins: t.wins,
                draws: t.draws,
                losses: t.losses,
            })
            .collect(),
        problems: problems
            .into_iter()
            .map(|(problem, n)| {
                if n > 1 {
                    format!("{problem} ({n} games)")
                } else {
                    problem
                }
            })
            .collect(),
        ..RatingRun::default()
    })
}

/// Describes a game lost on time, by an illegal move, or by a crash; `None` if it ended
/// normally or by adjudication.
fn abnormal_ending(game: &PlayedGame) -> Option<String> {
    let termination = game.termination.to_lowercase();
    if termination.is_empty()
        || termination.contains("normal")
        || termination.contains("adjudication")
    {
        return None;
    }
    let loser = match game.result.as_str() {
        "1-0" => &game.black,
        "0-1" => &game.white,
        _ => {
            return Some(format!(
                "{} vs {}: {}",
                game.white, game.black, game.termination
            ));
        }
    };
    let opponent = if loser == &game.white {
        &game.black
    } else {
        &game.white
    };
    Some(format!(
        "{loser} lost against {opponent}: {}",
        game.termination
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_a_chain_and_anchors() {
        let pairings = schedule(&BTreeMap::new(), (1320, 3190));
        let names: Vec<(String, String)> =
            pairings.iter().map(|(a, b)| (a.name(), b.name())).collect();
        let has = |a: &str, b: &str| names.contains(&(a.to_string(), b.to_string()));
        assert!(has("Beginner", "Easy"));
        assert!(has("Hard", "Full"));
        // Below Stockfish's floor, both anchors collapse into one.
        assert!(has("Beginner", "Stockfish 1320"));
        assert_eq!(names.iter().filter(|(a, _)| a == "Beginner").count(), 2);
        assert!(has("Hard", "Stockfish 1400") && has("Hard", "Stockfish 1800"));
    }

    #[test]
    fn previous_ratings_move_the_anchors() {
        let latest = BTreeMap::from([("Full".to_string(), 2010.0)]);
        let pairings = schedule(&latest, (1320, 3190));
        assert!(pairings.contains(&(Agent::Ours(FULL), Agent::Stockfish(1800))));
        assert!(pairings.contains(&(Agent::Ours(FULL), Agent::Stockfish(2200))));
    }

    #[test]
    fn reads_stockfish_options() {
        let output = "Stockfish 16 by the Stockfish developers (see AUTHORS file)\n\
            id name Stockfish 16\n\
            option name UCI_LimitStrength type check default false\n\
            option name UCI_Elo type spin default 1320 min 1320 max 3190\n\
            uciok\n";
        assert_eq!(
            parse_stockfish_uci(output),
            Some(("Stockfish 16".to_string(), (1320, 3190)))
        );
        assert_eq!(parse_stockfish_uci("id name Something\n"), None);
    }

    #[test]
    fn parses_pgn_headers() {
        let text = "[Event \"Fastchess Tournament\"]\n[White \"Hard\"]\n[Black \"SF1500\"]\n\
            [Result \"0-1\"]\n[Termination \"time forfeit\"]\n\n1. e4 e5 0-1\n\n\
            [Event \"Fastchess Tournament\"]\n[White \"SF1500\"]\n[Black \"Hard\"]\n\
            [Result \"1/2-1/2\"]\n\n1. d4 d5 1/2-1/2\n";
        let games = parse_pgn(text);
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].termination, "time forfeit");
        assert_eq!(games[1].white, "SF1500");
        assert_eq!(
            abnormal_ending(&games[0]),
            Some("Hard lost against SF1500: time forfeit".to_string())
        );
        assert_eq!(abnormal_ending(&games[1]), None);
    }

    #[test]
    fn rates_from_games() {
        let pairings = vec![
            (Agent::Ours("Hard"), Agent::Stockfish(1500)),
            (Agent::Ours("Hard"), Agent::Ours(FULL)),
        ];
        let game = |white: &str, black: &str, result: &str| PlayedGame {
            white: white.into(),
            black: black.into(),
            result: result.into(),
            termination: String::new(),
        };
        let mut games = Vec::new();
        for _ in 0..10 {
            games.push(game("Hard", "SF1500", "1-0"));
            games.push(game("SF1500", "Hard", "1/2-1/2"));
            games.push(game("Full", "Hard", "1-0"));
            games.push(game("Hard", "Full", "1/2-1/2"));
        }
        let run = rate(&pairings, &games).unwrap();
        assert_eq!(run.agents[0].name, "Full");
        assert_eq!(run.agents[1].name, "Hard");
        assert!(run.agents[1].elo > 1600.0 && run.agents[1].elo < 1800.0);
        assert_eq!(run.agents[1].games, 40);
        assert_eq!(run.agents[1].score, 0.5);
        assert_eq!(
            run.pairings[0],
            Pairing {
                a: "Hard".into(),
                b: "Stockfish 1500".into(),
                wins: 10,
                draws: 10,
                losses: 0
            }
        );
    }
}
