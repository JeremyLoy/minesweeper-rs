# TODO — future enhancements

Planned improvements, roughly ordered by value-to-effort. These fill in classic
Minesweeper features the current build is missing. Difficulty, board size, and
mine count are the consts `ROWS` / `COLS` / `MINES` in `src/main.rs`.

## Gameplay features

- [x] **Difficulty selector** — Beginner (9×9 / 10), Intermediate (16×16 / 40),
      Expert (30×16 / 99), plus a custom width/height/mines dialog. Board
      dimensions are now runtime fields (`rows`/`cols`/`mines`) on `Minesweeper`;
      a combo box switches presets and the window resizes to fit, and a Custom
      dialog takes width/height/mines.
- [x] **Question marks (`?`)** — the classic third right-click state
      (Hidden → Flagged → Question → Hidden), as a toggleable option (the `?`
      checkbox in the header). `?` cells are reminders only — not protected, so
      they still reveal and flood like hidden cells. Persisted in `Settings`.
- [x] **Chord with both mouse buttons** — holding left+right over a revealed
      number and releasing chords it, alongside the existing left-click-on-number.
      The gesture suppresses the individual clicks (so the right-button release
      doesn't drop a flag) and fires the chord on release.
- [x] **Flag-count guard** — optional `flag_guard` setting (in the new Options
      menu) that refuses to place more flags than there are mines.
- [x] **First-click opening guarantee** — `open_guarantee` setting (default on).
      On, the whole 3×3 around the first click is spared, so the clicked cell has
      zero adjacent mines and always floods open a region; off, only the clicked
      cell is spared (safe, but the first reveal may be a bare number).
- [x] **Lose animation / reveal** — on a loss, missed mines are exposed, the mine
      you hit is drawn on a red background, and wrong flags (a flag on a non-mine)
      are crossed out with a red ✗ (`draw_cross`), classic-style.

## Persistence & meta

- [ ] **Settings persistence** — remember last difficulty, question-mark toggle,
      and window position between runs.
- [ ] **Statistics** — games played, win %, win/loss streaks.

- [x] **Best times / high scores** per difficulty, persisted to disk. A tiny,
      dependency-free `key=value` save file (`SaveData`) in the platform config
      dir holds the best completion seconds per preset; a win updates it and the
      header shows the current preset's record (and "New best!" on a record).

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
