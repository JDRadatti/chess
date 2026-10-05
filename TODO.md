# TODO

## Workflow migration

[x] cargo workspace under crates/
[x] AGENTS.md, justfile, git hooks, pinned toolchain, clippy.toml
[x] TODO.md tracked, in the AGENTS.md format
[x] workspace lints on every crate
    [x] formatting and mechanical clippy fixes
    [x] unreachable_pub: pub(crate) for unreachable items
    [x] numeric casts: fixed outside game, expected in game
    [x] prints: expected in bench, debug print removed from game
    [x] doc lints expected per crate until Doc comments is done
[x] docs/ as an mdBook site, one page per crate
[x] git hooks enabled with just setup
finished: chore: migrate to the birdie workflow

## Doc comments

[ ] game: doc comments on public items, `# Errors` and `# Panics` sections, tests/san_integration.rs crate doc
[ ] tui: doc comments on public items, `# Errors` and `# Panics` sections
[ ] bench: `// SAFETY:` on the unsafe impl and blocks in alloc.rs
[ ] remove each crate's doc-lint `#![expect(...)]` once it is done
finished:

## Fix move execution

[x] move the piece from `src`, not a type taken from `dst`
[x] promotion places the promoted piece
[x] en passant removes the captured pawn behind `dst`
[x] castling moves the rook
[x] castling rights cleared when the king or a rook moves, or a rook is captured
[x] en passant target set on a double push, cleared otherwise
[x] side to move toggles
[x] halfmove clock and fullmove counter
[x] tests: promotion, en passant, both castlings, rights lost on rook capture, halfmove reset
finished: Fix move execution: promotion, en passant, castling, and state updates

## Legal move generation

[x] king-safety filter on pseudo-legal moves
[x] castling goes through the same filter
[x] tests: pins, king into check, pinned en passant pawn
finished: Add legal move generation with king-safety filter

## FEN I/O

[x] parse placement, side, castling, en passant, counters
[x] validate field count, kings, back-rank pawns, side not to move in check
[x] `to_fen` serialization
[x] tests: startpos, mid-game, round-trip, malformed input
finished: Add FEN parsing and serialization

## SAN

[x] parse: piece, disambiguation, capture, promotion, check and mate suffixes, castling
[x] reject unknown pieces, missing destination, ambiguity, no legal match
[x] format: minimal disambiguation, `+`/`#`, castling
[x] tests: round-trip every legal move in several positions
finished: Add SAN move parsing and formatting with tests

## Public API

[x] `Square` public
[x] `Position::apply_move`
[x] `Position::legal_moves_from`
[x] `Move` accessors; coordinate `FromStr`, SAN through `parse_san`/`to_san`
[x] public re-exports in lib.rs, everything else crate-private
finished: Add public API: apply_move, moves_from_square, make Square public

## Validation

[x] perft: startpos, Kiwipete, positions 3 and 4
[x] fix perft mismatches
[x] full test run
finished: Fix move generation bugs and add perft and SAN integration tests

## Play against the computer

[x] `Player` trait, `Match`, `RandomPlayer`
[x] ratatui front end: board, mouse, promotion picker, popups, captured pieces, image pieces
[x] engine: material and piece-square eval, iterative-deepening alpha-beta, quiescence, move ordering, time limit, draws
[x] `EnginePlayer` on a background thread, four levels, depth and eval in the panel
finished: Add alpha-beta engine with difficulty levels

## Measuring

[x] bench: speed, allocations, peak memory, hardware counters, history and report
[x] faster move generation and search
[x] uci: `Level` option incl. clock-managed `Full`, Move Overhead; passes `fastchess --compliance`
[x] bench rate: fastchess games vs Stockfish anchors, maximum-likelihood Elo, ratings.jsonl
finished: Add bench rate: Elo ratings against Stockfish via fastchess

## Ideas

Not planned yet. Plan one before adding it as a feature.

[ ] Stockfish as a TUI opponent (`UciPlayer` speaking UCI to any engine)
[ ] takeback (`u`) and hint (`?`)
[ ] game review: step through past positions, flip board
[ ] chess clocks: time controls, clocks in the panel, loss on time
[ ] stronger engine: Zobrist hashing, transposition table, killer/history ordering
[ ] PGN export and starting from a pasted FEN
finished:
