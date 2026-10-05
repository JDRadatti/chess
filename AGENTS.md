# AGENTS.md

chess is a chess library, engine, and ratatui TUI, with a UCI front end and a benchmark harness.

## Writing style

- Be concise. Short, descriptive sentences.

## Layout

- `crates/game`: rules, FEN, SAN, move generation, and the engine. A library with no I/O.
- `crates/tui`: the ratatui front end. Human vs engine.
- `crates/uci`: the engine as a UCI program, for GUIs and fastchess.
- `crates/bench`: speed, memory, and Elo measurements. Results in `history.jsonl` and `ratings.jsonl`.
- `TODO.md`: features and their subtasks.
- `docs/`: high-level docs per major component (mdBook, `just docs` to view).
- `docs/planning/`: feature plans. Start from `docs/planning/TEMPLATE.md`.
- `.githooks/`: never bypass hooks with `--no-verify`.

## Commands

- `just setup`: install pinned dev tools and enable the git hooks.
- `just docs`: serve docs with live reload.
- `just docs-build`: build docs to `target/book`.
- `just check`: format check, clippy, build.
- `just test [filter]`: run tests, optionally filtered by name.
- `just run`: play in the TUI.
- `just uci`: run the UCI engine on stdin and stdout.
- `just bench [args]`: run the benchmark harness. `just bench help` lists commands.

## Planning a feature

1. If the feature is big or if prompted, write `docs/planning/<feature>.md` from the template. Otherwise, propose the plan in chat.
2. Ask for review. Revise until the plan is agreed.
3. After agreement, add the feature and its subtasks to `TODO.md`.

## Implementing a feature

1. Read the feature in `TODO.md` and its plan in `docs/planning/`.
2. Work on a branch: `feat/<short-name>`.
3. Implement test-first (see Tests). No comments. Keep `just test` passing.
4. Stop and ask for acceptance. Explain exactly how to test the change.
5. The user writes the comments (see Comments below).
6. Review the user's comments. Do not commit until they pass review.
7. After acceptance and comment review:
   - Commit in small steps on the branch.
   - Tick completed subtasks in `TODO.md` (`[x]`) and set `finished:` to the commit subject.
   - Squash-merge the branch into `main` with one commit in the format below.
8. Update `docs/` if a component's behavior changed.

## Tests

Use red/green TDD for features and bug fixes:

1. Write a test for the behavior. For a bug, the test reproduces it.
2. Run it with `just test <name>`. Confirm it fails for the expected reason. Show the failure.
3. Write the smallest code that makes it pass.
4. Run `just test`. Show the output.

- Never edit a test to make it pass. If the test is wrong, say why and ask.
- Test through public APIs, not internals.
- Move generation is checked by perft against published counts. Never change an expected count.
- Notation is checked against the real games in `crates/game/tests/pgn/`.
- UI tests render to ratatui's `TestBackend` and assert on the buffer.
- UCI tests drive the protocol through `crates/uci/tests/`.
- Refactors add no tests. Existing tests pass before and after, unchanged.

## Measuring

- Changes to move generation, search, or evaluation run `just bench` before and after. Report the comparison.
- Strength changes run `just bench rate --quick` at least. A full `just bench rate` takes about an hour; ask first.
- After a squash-merge that changes speed or strength, record the results on `main` in a `chore: record bench results for <hash>` commit.

## Comments

The user writes all comments. The agent writes code without them, then reviews.

What needs a comment:
- A `///` doc comment on every public item (fn, struct, enum, trait, module).
- A `//` comment on logic that is not obvious from the code.
- A `// SAFETY:` comment on every `unsafe` block.

`missing_docs = "warn"` is set in `[workspace.lints]` in the root `Cargo.toml`. Every crate's `Cargo.toml` has `[lints] workspace = true`. `just check` fails on warnings, so missing docs block the commit.

Review checks:
- The comment is accurate. A wrong comment means a misunderstanding or confusing code. Fix whichever it is.
- The comment says why or what it guarantees, not a restatement of the code.
- Important edge cases, panics, and errors are mentioned.
- Report findings as a list. Suggest fixes; the user edits the comments.

## TODO.md format

```
## FEATURE NAME

[ ] subtask
[ ] subtask
    [ ] sub-subtask
finished:
```

## Commit format

```
type: short description

update
- update 1

add
- add 1

delete
- delete 1

testing
- testing 1
```

- Subject: `type: description`. Lowercase, imperative, under 60 characters including the type.
- Types: feat, fix, refactor, perf, docs, test, chore (Conventional Commits). A squash commit uses the type of its main change.
- Omit sections that are empty.
- Other sections (e.g. `perf`, `notes`) are allowed when useful.

## Code

- Keep rules and engine logic in `game` so it can be tested headless. Front ends only translate input and output.
- `game` does no I/O: no printing, files, or environment.
- The TUI owns stdout. Never print from `tui`.
- `uci` owns stdout for the protocol. Write only through its `send` helper.
- Lints are suppressed with `#[expect(..., reason = "...")]`, never `#[allow]`.
