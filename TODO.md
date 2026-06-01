# TODO — future enhancements

Planned improvements, roughly ordered by value-to-effort. These fill in classic
Minesweeper features the current build is missing. Difficulty, board size, and
mine count are the consts `ROWS` / `COLS` / `MINES` in `src/main.rs`.

## Gameplay features

- [ ] **Chord with both mouse buttons** — support the traditional left+right
      simultaneous click to chord, in addition to the current left-click-on-number.
- [ ] **Flag-count guard** — optionally prevent placing more flags than there are
      mines.
- [ ] **First-click opening guarantee** — optionally regenerate the board until
      the first click opens a zero-region (currently the 3×3 safe zone makes this
      likely but not guaranteed).
- [ ] **Lose animation / reveal** — show which flags were wrong (✗) and which
      mines were missed, like classic Minesweeper, instead of just exposing mines.
- [x] **Difficulty selector** — Beginner (9×9 / 10), Intermediate (16×16 / 40),
      Expert (30×16 / 99), plus a custom width/height/mines dialog. Board
      dimensions are now runtime fields (`rows`/`cols`/`mines`) on `Minesweeper`;
      a combo box switches presets and the window resizes to fit, and a Custom
      dialog takes width/height/mines.
- [x] **Question marks (`?`)** — the classic third right-click state
      (Hidden → Flagged → Question → Hidden), as a toggleable option (the `?`
      checkbox in the header). `?` cells are reminders only — not protected, so
      they still reveal and flood like hidden cells. Persisted in `Settings`.

## Persistence & meta

- [ ] **Best times / high scores** per difficulty, persisted to disk
      (e.g. via `eframe`'s storage or a small JSON file in the config dir).
- [ ] **Settings persistence** — remember last difficulty, question-mark toggle,
      and window position between runs.
- [ ] **Statistics** — games played, win %, win/loss streaks.

## UX & polish

- [ ] **Pause / safe-resign** and an explicit "Restart" vs "New board".
- [ ] **Keyboard support** — arrow-key/`hjkl` cursor, space to reveal, `f` to
      flag; useful for accessibility.
- [ ] **Theming** — light/dark toggle and a classic Win95 skin; honor the system
      theme via egui.
- [ ] **Sound effects** — reveal, flag, explosion, win jingle (would add an audio
      crate such as `rodio` — weigh against the keep-deps-minimal philosophy).
- [ ] **Resizable / scalable board** — let the window resize and scale cell size,
      or support high-DPI zoom controls.
- [ ] **Timer formatting** — show mm:ss past 99 seconds rather than capping the
      display.

## Engineering

- [ ] **Unit tests** for the pure game logic (see CLAUDE.md "Testing philosophy"):
      mine count, first-click safe-zone invariant, adjacency correctness,
      flood-fill reveal counts, chord behavior, win/lose transitions.
- [ ] **Split into modules** once the logic grows — e.g. `game.rs` (rules) and
      `app.rs` (rendering) — if `main.rs` stops being comfortable to read in one
      sitting.
- [ ] **CI** — GitHub Actions matrix (Windows/macOS/Linux) running
      `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
- [ ] **Web build** — egui compiles to WASM; add a `trunk`/`wasm` target so the
      game can run in a browser from the same codebase.
- [ ] **Seeded games** — expose the RNG seed so a specific board can be shared or
      replayed (helps testing and "daily puzzle" ideas).
