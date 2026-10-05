//! The engine as a UCI program, so GUIs and match runners such as fastchess can play it.
//!
//! Reads commands on stdin and answers on stdout. Searches run on a background thread so
//! `stop` and `isready` are answered while thinking.

mod protocol;

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use game::Game;
use game::engine::think;
use protocol::{Go, Strength};

const DEFAULT_OVERHEAD_MS: u64 = 30;
const MAX_OVERHEAD_MS: u64 = 5000;

fn main() {
    let mut engine = Engine::new();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if !engine.handle(&line) {
            break;
        }
    }
    engine.stop();
}

/// Writes one line and flushes, so the GUI sees it immediately.
fn send(line: &str) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

struct Engine {
    strength: Strength,
    overhead: Duration,
    game: Game,
    search: Option<Search>,
}

struct Search {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Engine {
    fn new() -> Self {
        Self {
            strength: Strength::Full,
            overhead: Duration::from_millis(DEFAULT_OVERHEAD_MS),
            game: Game::new(),
            search: None,
        }
    }

    /// Handles one command. Returns false on `quit`.
    fn handle(&mut self, line: &str) -> bool {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&command, args)) = words.split_first() else {
            return true;
        };
        match command {
            "uci" => Self::identify(),
            "isready" => send("readyok"),
            "setoption" => self.set_option(args),
            "ucinewgame" => {
                self.stop();
                self.game = Game::new();
            }
            "position" => {
                self.stop();
                match protocol::parse_position(args) {
                    Ok(game) => self.game = game,
                    Err(e) => send(&format!("info string {e}")),
                }
            }
            "go" => self.go(&Go::parse(args)),
            "stop" => self.stop(),
            "quit" => return false,
            "debug" | "ponderhit" => {}
            _ => send(&format!("info string unknown command `{command}`")),
        }
        true
    }

    fn identify() {
        send(concat!("id name chess ", env!("CARGO_PKG_VERSION")));
        send("id author Justin");
        let levels: Vec<String> = Strength::ALL
            .iter()
            .map(|s| format!("var {}", s.name()))
            .collect();
        send(&format!(
            "option name Level type combo default {} {}",
            Strength::Full.name(),
            levels.join(" ")
        ));
        send(&format!(
            "option name Move Overhead type spin default {DEFAULT_OVERHEAD_MS} min 0 max {MAX_OVERHEAD_MS}"
        ));
        send("uciok");
    }

    fn set_option(&mut self, args: &[&str]) {
        let Some((name, value)) = protocol::parse_setoption(args) else {
            send("info string expected `setoption name <name> value <value>`");
            return;
        };
        if name.eq_ignore_ascii_case("Level") {
            match Strength::from_name(&value) {
                Some(strength) => self.strength = strength,
                None => send(&format!("info string unknown level `{value}`")),
            }
        } else if name.eq_ignore_ascii_case("Move Overhead") {
            match value.parse::<u64>() {
                Ok(ms) => self.overhead = Duration::from_millis(ms.min(MAX_OVERHEAD_MS)),
                Err(_) => send(&format!("info string bad Move Overhead `{value}`")),
            }
        } else {
            send(&format!("info string unknown option `{name}`"));
        }
    }

    fn go(&mut self, go: &Go) {
        self.stop();
        let game = self.game.clone();
        let side = game.position().side_to_move();
        let settings = protocol::settings(self.strength, go, side, self.overhead);
        let seed = RandomState::new().build_hasher().finish();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::spawn({
            let stop = Arc::clone(&stop);
            move || {
                let best = think(&game, &settings, &stop, seed, |info| {
                    send(&protocol::format_info(info, side));
                });
                let best = best.map_or_else(|| "0000".to_string(), protocol::format_move);
                send(&format!("bestmove {best}"));
            }
        });
        self.search = Some(Search { stop, thread });
    }

    /// Ends the current search, if any, once it has sent its `bestmove`.
    fn stop(&mut self) {
        if let Some(search) = self.search.take() {
            search.stop.store(true, Ordering::Relaxed);
            let _ = search.thread.join();
        }
    }
}
