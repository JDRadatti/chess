//! Talks to the built binary the way a GUI would.

use std::io::{BufRead, BufReader, Lines, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl Session {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_uci"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the uci binary starts");
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").expect("the engine reads stdin");
    }

    /// Lines up to and including the first one starting with `prefix`.
    fn read_until(&mut self, prefix: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for line in &mut self.stdout {
            let line = line.expect("the engine writes UTF-8");
            let done = line.starts_with(prefix);
            lines.push(line);
            if done {
                return lines;
            }
        }
        panic!("output ended before `{prefix}`: {lines:?}");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "quit");
        let _ = self.child.wait();
    }
}

#[test]
fn handshake_lists_options() {
    let mut uci = Session::start();
    uci.send("uci");
    let lines = uci.read_until("uciok");
    assert!(lines.iter().any(|l| l.starts_with("id name ")));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("option name Level type combo"))
    );
    uci.send("isready");
    assert_eq!(uci.read_until("readyok"), ["readyok"]);
}

#[test]
fn searches_a_position_to_a_fixed_depth() {
    let mut uci = Session::start();
    uci.send("position startpos moves e2e4 e7e5");
    uci.send("go depth 3");
    let lines = uci.read_until("bestmove");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("info depth 3 score cp "))
    );
    let best = lines.last().unwrap();
    assert_eq!(best.len(), "bestmove g1f3".len(), "{best}");
}

#[test]
fn finds_mate_in_one() {
    let mut uci = Session::start();
    uci.send("position fen 6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
    uci.send("go movetime 1000");
    let lines = uci.read_until("bestmove");
    assert_eq!(lines.last().unwrap(), "bestmove a1a8");
    assert!(lines.iter().any(|l| l.contains("score mate 1")));
}

#[test]
fn stop_ends_an_infinite_search() {
    let mut uci = Session::start();
    uci.send("position startpos");
    uci.send("go infinite");
    std::thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    uci.send("stop");
    let lines = uci.read_until("bestmove");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(lines.last().unwrap().len() > "bestmove ".len());
}

#[test]
fn respects_the_clock() {
    let mut uci = Session::start();
    uci.send("setoption name Level value Hard");
    uci.send("position startpos");
    let started = Instant::now();
    uci.send("go wtime 3000 btime 3000");
    uci.read_until("bestmove");
    // A hard level thinks 2 s per move, but 3 s on the clock only allows about 100 ms.
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn reports_no_move_when_the_game_is_over() {
    let mut uci = Session::start();
    uci.send("position startpos moves f2f3 e7e5 g2g4 d8h4");
    uci.send("go depth 2");
    assert_eq!(uci.read_until("bestmove").last().unwrap(), "bestmove 0000");
}
