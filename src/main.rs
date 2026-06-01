// Minesweeper — a cross-platform GUI game built with egui/eframe.
//
// Classic rules. Board size and mine count are chosen at runtime via the
// difficulty selector (Beginner 9x9/10, Intermediate 16x16/40, Expert
// 30x16/99, or a Custom board).
//   * Left-click   reveals a cell. Revealing a mine ends the game.
//   * Right-click  toggles a flag on a hidden cell.
//   * The very first click is always safe — mines are placed *after* it,
//     avoiding the clicked cell and its neighbors so the first move opens
//     up an area.
//   * Left-clicking an already-revealed number whose adjacent flags match
//     that number "chords": it reveals the remaining neighbors at once.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window in release

use eframe::egui;
use std::time::Instant;

const CELL: f32 = 34.0; // pixel size of a single cell

/// Padding added around the board to size the OS window: horizontal margin and
/// the header height above the grid.
const WIN_PAD_W: f32 = 24.0;
const WIN_PAD_H: f32 = 110.0;

fn main() -> eframe::Result {
    let (rows, cols, _mines) = Difficulty::Beginner.dims().unwrap();
    let size = window_size(rows, cols);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_resizable(false)
            .with_title("Minesweeper"),
        ..Default::default()
    };
    eframe::run_native(
        "Minesweeper",
        options,
        Box::new(|_cc| Ok(Box::new(Minesweeper::new(Difficulty::Beginner)))),
    )
}

/// OS-window inner size needed to show a `rows`x`cols` board.
fn window_size(rows: usize, cols: usize) -> egui::Vec2 {
    egui::vec2(
        cols as f32 * CELL + WIN_PAD_W,
        rows as f32 * CELL + WIN_PAD_H,
    )
}

/// The selectable difficulty presets, plus a free-form Custom board.
#[derive(Clone, Copy, PartialEq)]
enum Difficulty {
    Beginner,
    Intermediate,
    Expert,
    Custom,
}

impl Difficulty {
    /// Preset board dimensions as `(rows, cols, mines)`. `Custom` has no fixed
    /// dimensions (the player supplies them), so it returns `None`.
    fn dims(self) -> Option<(usize, usize, usize)> {
        match self {
            Difficulty::Beginner => Some((9, 9, 10)),
            Difficulty::Intermediate => Some((16, 16, 40)),
            // Expert is the classic 30-wide by 16-tall board: 16 rows, 30 cols.
            Difficulty::Expert => Some((16, 30, 99)),
            Difficulty::Custom => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Difficulty::Beginner => "Beginner",
            Difficulty::Intermediate => "Intermediate",
            Difficulty::Expert => "Expert",
            Difficulty::Custom => "Custom",
        }
    }
}

/// A tiny xorshift64 PRNG so we don't need an external crate just to scatter
/// mines. Seeded from the system clock.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1; // never zero
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform integer in `0..n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum CellState {
    Hidden,
    Revealed,
    Flagged,
    /// A "?" mark — a reminder that is *not* protected: it can still be revealed.
    Question,
}

/// Player-chosen options that persist across new games (they are not part of a
/// single board's state, so `reset`/`apply_board` carry them over).
#[derive(Clone, Copy)]
struct Settings {
    /// Whether right-click cycles through a "?" state (Hidden → Flag → ? → Hidden).
    question_marks: bool,
    /// Whether to forbid placing more flags than there are mines.
    flag_guard: bool,
    /// Whether the first click is guaranteed to open a region (spare the whole
    /// 3x3 around it) rather than merely being safe (spare only the click).
    open_guarantee: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            question_marks: true,
            flag_guard: false,
            open_guarantee: true,
        }
    }
}

#[derive(Clone, Copy)]
struct Cell {
    mine: bool,
    adjacent: u8,
    state: CellState,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            mine: false,
            adjacent: 0,
            state: CellState::Hidden,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Status {
    Playing,
    Won,
    Lost,
}

struct Minesweeper {
    rows: usize,
    cols: usize,
    mines: usize,
    difficulty: Difficulty,
    grid: Vec<Vec<Cell>>,
    status: Status,
    mines_placed: bool,
    revealed_safe: usize, // count of revealed non-mine cells
    flags: i32,
    start: Option<Instant>,
    final_time: Option<f64>,
    /// The cell that ended the game, drawn with a red background.
    exploded: Option<(usize, usize)>,
    /// Whether the Custom-board dialog is open, and its pending field values.
    show_custom: bool,
    custom_rows: usize,
    custom_cols: usize,
    custom_mines: usize,
    /// Set when the board changes; the next frame resizes the OS window to fit.
    pending_resize: bool,
    settings: Settings,
    /// Transient input state for the both-button chord gesture: the revealed
    /// cell currently under the cursor while both buttons are held, and whether
    /// such a gesture is in progress (so the eventual button release chords
    /// instead of revealing/flagging).
    armed_chord: Option<(usize, usize)>,
    chord_gesture: bool,
}

impl Minesweeper {
    /// Build a board for the given difficulty. For `Custom`, the dimensions come
    /// from the dialog fields (defaulted here to a Beginner-like board).
    fn new(difficulty: Difficulty) -> Self {
        let (rows, cols, mines) = difficulty.dims().unwrap_or((9, 9, 10));
        Self::with_dims(rows, cols, mines, difficulty)
    }

    fn with_dims(rows: usize, cols: usize, mines: usize, difficulty: Difficulty) -> Self {
        Minesweeper {
            rows,
            cols,
            mines,
            difficulty,
            grid: vec![vec![Cell::default(); cols]; rows],
            status: Status::Playing,
            mines_placed: false,
            revealed_safe: 0,
            flags: 0,
            start: None,
            final_time: None,
            exploded: None,
            show_custom: false,
            custom_rows: rows,
            custom_cols: cols,
            custom_mines: mines,
            pending_resize: false,
            settings: Settings::default(),
            armed_chord: None,
            chord_gesture: false,
        }
    }

    /// Carry the player's persistent choices (settings and last custom-dialog
    /// entries) from `self` onto a freshly built board.
    fn carry_over(&self, fresh: &mut Minesweeper) {
        fresh.settings = self.settings;
        fresh.custom_rows = self.custom_rows;
        fresh.custom_cols = self.custom_cols;
        fresh.custom_mines = self.custom_mines;
    }

    /// Rebuild the board with the same dimensions and difficulty (a fresh game).
    fn reset(&mut self) {
        let mut fresh = Self::with_dims(self.rows, self.cols, self.mines, self.difficulty);
        self.carry_over(&mut fresh);
        *self = fresh;
    }

    /// Switch to a new board (different size and/or mine count) and start fresh,
    /// requesting an OS-window resize on the next frame.
    fn apply_board(&mut self, rows: usize, cols: usize, mines: usize, difficulty: Difficulty) {
        let mut fresh = Self::with_dims(rows, cols, mines, difficulty);
        self.carry_over(&mut fresh);
        *self = fresh;
        self.pending_resize = true;
    }

    /// Cells eligible to hold a mine: everything outside the Chebyshev-distance
    /// `radius` square centered on the first click. `radius == 1` spares the 3x3
    /// safe zone; `radius == 0` spares only the clicked cell.
    fn safe_candidates(&self, safe_r: usize, safe_c: usize, radius: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for r in 0..self.rows {
            for c in 0..self.cols {
                let near = r.abs_diff(safe_r) <= radius && c.abs_diff(safe_c) <= radius;
                if !near {
                    out.push((r, c));
                }
            }
        }
        out
    }

    /// Place mines after the first click. With `open_guarantee` the whole 3x3
    /// around the click is spared, so the clicked cell has zero adjacent mines
    /// and the opening reveal always floods into a region; otherwise only the
    /// clicked cell is spared (still safe, but the first reveal may be a number).
    fn place_mines(&mut self, safe_r: usize, safe_c: usize) {
        let radius = if self.settings.open_guarantee { 1 } else { 0 };
        let mut candidates = self.safe_candidates(safe_r, safe_c, radius);
        // If the board is too dense for the requested safe zone, spare only the
        // click so there is always room for every mine.
        if candidates.len() < self.mines {
            candidates = self.safe_candidates(safe_r, safe_c, 0);
        }

        // Partial Fisher–Yates: pick the first `mines` after shuffling.
        let mut rng = Rng::new();
        let n = self.mines.min(candidates.len());
        for i in 0..n {
            let j = i + rng.below(candidates.len() - i);
            candidates.swap(i, j);
            let (r, c) = candidates[i];
            self.grid[r][c].mine = true;
        }

        // Compute adjacency counts.
        for r in 0..self.rows {
            for c in 0..self.cols {
                if self.grid[r][c].mine {
                    continue;
                }
                let mut count = 0;
                for (nr, nc) in self.neighbors(r, c) {
                    if self.grid[nr][nc].mine {
                        count += 1;
                    }
                }
                self.grid[r][c].adjacent = count;
            }
        }
    }

    fn reveal(&mut self, r: usize, c: usize) {
        if self.status != Status::Playing {
            return;
        }

        if !self.mines_placed {
            self.place_mines(r, c);
            self.mines_placed = true;
            self.start = Some(Instant::now());
        }

        match self.grid[r][c].state {
            CellState::Flagged => return,
            // Clicking an already-revealed number attempts a chord.
            CellState::Revealed => {
                self.chord(r, c);
                return;
            }
            // Hidden and "?" cells are both revealable.
            CellState::Hidden | CellState::Question => {}
        }

        if self.grid[r][c].mine {
            self.grid[r][c].state = CellState::Revealed;
            self.exploded = Some((r, c));
            self.lose();
            return;
        }

        self.flood_reveal(r, c);
        self.check_win();
    }

    /// Iterative flood fill: reveal the cell; if it has no adjacent mines,
    /// spread to its neighbors.
    fn flood_reveal(&mut self, r: usize, c: usize) {
        let mut stack = vec![(r, c)];
        while let Some((r, c)) = stack.pop() {
            let cell = &mut self.grid[r][c];
            // Only Hidden/"?" cells are revealable; never spread through mines.
            if matches!(cell.state, CellState::Revealed | CellState::Flagged) || cell.mine {
                continue;
            }
            cell.state = CellState::Revealed;
            self.revealed_safe += 1;
            if cell.adjacent == 0 {
                for (nr, nc) in self.neighbors(r, c) {
                    if matches!(
                        self.grid[nr][nc].state,
                        CellState::Hidden | CellState::Question
                    ) {
                        stack.push((nr, nc));
                    }
                }
            }
        }
    }

    /// If a revealed number has exactly as many flags around it as its count,
    /// reveal all the remaining hidden neighbors at once.
    fn chord(&mut self, r: usize, c: usize) {
        let cell = self.grid[r][c];
        if cell.adjacent == 0 {
            return;
        }
        let flagged = self
            .neighbors(r, c)
            .filter(|&(nr, nc)| self.grid[nr][nc].state == CellState::Flagged)
            .count() as u8;
        if flagged != cell.adjacent {
            return;
        }
        for (nr, nc) in self.neighbors(r, c) {
            if matches!(
                self.grid[nr][nc].state,
                CellState::Hidden | CellState::Question
            ) {
                if self.grid[nr][nc].mine {
                    self.grid[nr][nc].state = CellState::Revealed;
                    self.exploded = Some((nr, nc));
                    self.lose();
                    return;
                }
                self.flood_reveal(nr, nc);
            }
        }
        self.check_win();
    }

    fn toggle_flag(&mut self, r: usize, c: usize) {
        if self.status != Status::Playing {
            return;
        }
        // Right-click cycles Hidden → Flagged → (? if enabled) → Hidden.
        match self.grid[r][c].state {
            CellState::Hidden => {
                // Optional guard: never place more flags than there are mines.
                if self.settings.flag_guard && self.flags >= self.mines as i32 {
                    return;
                }
                self.grid[r][c].state = CellState::Flagged;
                self.flags += 1;
            }
            CellState::Flagged => {
                self.flags -= 1;
                self.grid[r][c].state = if self.settings.question_marks {
                    CellState::Question
                } else {
                    CellState::Hidden
                };
            }
            CellState::Question => {
                self.grid[r][c].state = CellState::Hidden;
            }
            CellState::Revealed => {}
        }
    }

    fn lose(&mut self) {
        self.status = Status::Lost;
        self.freeze_time();
        // Expose every mine.
        for r in 0..self.rows {
            for c in 0..self.cols {
                if self.grid[r][c].mine && self.grid[r][c].state != CellState::Flagged {
                    self.grid[r][c].state = CellState::Revealed;
                }
            }
        }
    }

    fn check_win(&mut self) {
        if self.revealed_safe == self.rows * self.cols - self.mines {
            self.status = Status::Won;
            self.freeze_time();
            // Auto-flag the remaining mines for a tidy finish.
            for r in 0..self.rows {
                for c in 0..self.cols {
                    if self.grid[r][c].mine && self.grid[r][c].state != CellState::Flagged {
                        self.grid[r][c].state = CellState::Flagged;
                        self.flags += 1;
                    }
                }
            }
        }
    }

    fn freeze_time(&mut self) {
        if let Some(start) = self.start {
            self.final_time = Some(start.elapsed().as_secs_f64());
        }
    }

    fn elapsed_secs(&self) -> u64 {
        let secs = match (self.status, self.final_time, self.start) {
            (Status::Playing, _, Some(start)) => start.elapsed().as_secs_f64(),
            (_, Some(t), _) => t,
            _ => 0.0,
        };
        (secs as u64).min(999)
    }

    /// Yields the in-bounds neighbors of `(r, c)`. Returns an owned iterator
    /// (not borrowing `self`) so callers can keep mutating `self.grid` while
    /// iterating.
    fn neighbors(&self, r: usize, c: usize) -> std::vec::IntoIter<(usize, usize)> {
        let (rows, cols) = (self.rows, self.cols);
        let mut out = Vec::with_capacity(8);
        for dr in -1i32..=1 {
            for dc in -1i32..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr >= 0 && nr < rows as i32 && nc >= 0 && nc < cols as i32 {
                    out.push((nr as usize, nc as usize));
                }
            }
        }
        out.into_iter()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

impl eframe::App for Minesweeper {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Resize the OS window to fit a freshly chosen board.
        if self.pending_resize {
            self.pending_resize = false;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(window_size(
                    self.rows, self.cols,
                )));
        }

        // Keep the timer ticking while the game is live.
        if self.status == Status::Playing && self.mines_placed {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Panel::top("header").show_inside(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                // Mines remaining (mines minus flags placed).
                let remaining = self.mines as i32 - self.flags;
                ui.label(
                    egui::RichText::new(format!("Mines {remaining:>3}"))
                        .monospace()
                        .size(18.0)
                        .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Time {:>3}", self.elapsed_secs()))
                            .monospace()
                            .size(18.0)
                            .strong(),
                    );
                });
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Difficulty selector.
                let mut chosen: Option<Difficulty> = None;
                egui::ComboBox::from_id_salt("difficulty")
                    .selected_text(self.difficulty.label())
                    .show_ui(ui, |ui| {
                        for d in [
                            Difficulty::Beginner,
                            Difficulty::Intermediate,
                            Difficulty::Expert,
                            Difficulty::Custom,
                        ] {
                            if ui
                                .selectable_label(self.difficulty == d, d.label())
                                .clicked()
                            {
                                chosen = Some(d);
                            }
                        }
                    });
                if let Some(d) = chosen {
                    match d.dims() {
                        Some((rows, cols, mines)) => self.apply_board(rows, cols, mines, d),
                        // Custom: open the dialog instead of switching immediately.
                        None => self.show_custom = true,
                    }
                }

                ui.menu_button("Options", |ui| {
                    ui.checkbox(&mut self.settings.question_marks, "Question marks (?)")
                        .on_hover_text("Right-click cycles a question mark after the flag");
                    ui.checkbox(&mut self.settings.flag_guard, "Flag guard")
                        .on_hover_text("Don't allow more flags than there are mines");
                    ui.checkbox(&mut self.settings.open_guarantee, "Guaranteed opening")
                        .on_hover_text(
                            "First click always opens an empty region, not a bare number",
                        );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let face = match self.status {
                        Status::Playing => ":)",
                        Status::Won => "B)",
                        Status::Lost => ":(",
                    };
                    if ui
                        .add_sized(
                            [44.0, 30.0],
                            egui::Button::new(egui::RichText::new(face).size(20.0).strong()),
                        )
                        .on_hover_text("New game")
                        .clicked()
                    {
                        self.reset();
                    }
                });
            });

            ui.add_space(4.0);
            ui.vertical_centered(|ui| match self.status {
                Status::Won => {
                    ui.label(egui::RichText::new("You win!").strong());
                }
                Status::Lost => {
                    ui.label(egui::RichText::new("Boom! Try again.").strong());
                }
                Status::Playing => {
                    ui.label("Left-click reveal · Right-click flag");
                }
            });
            ui.add_space(6.0);
        });

        self.custom_dialog(ui.ctx());

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut left_click: Option<(usize, usize)> = None;
            let mut right_click: Option<(usize, usize)> = None;

            // Both-button chord: while both mouse buttons are held, remember the
            // revealed cell under the cursor; chord it once both are released.
            let (primary_down, secondary_down) =
                ui.input(|i| (i.pointer.primary_down(), i.pointer.secondary_down()));
            let both_down = primary_down && secondary_down;
            let any_down = primary_down || secondary_down;
            if both_down {
                self.chord_gesture = true;
            }

            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for r in 0..self.rows {
                ui.horizontal(|ui| {
                    for c in 0..self.cols {
                        let resp = self.draw_cell(ui, r, c);
                        if resp.clicked() {
                            left_click = Some((r, c));
                        }
                        if resp.secondary_clicked() {
                            right_click = Some((r, c));
                        }
                        if both_down && resp.hovered() {
                            self.armed_chord = Some((r, c));
                        }
                    }
                });
            }

            // A chord gesture swallows the individual clicks its buttons would
            // otherwise produce (notably the right-button "flag" on release).
            if self.chord_gesture {
                left_click = None;
                right_click = None;
                if !any_down {
                    if let Some((r, c)) = self.armed_chord {
                        // reveal() routes a click on a revealed cell to chord().
                        self.reveal(r, c);
                    }
                    self.chord_gesture = false;
                    self.armed_chord = None;
                }
            }

            if let Some((r, c)) = left_click {
                self.reveal(r, c);
            }
            if let Some((r, c)) = right_click {
                self.toggle_flag(r, c);
            }
        });
    }
}

impl Minesweeper {
    /// The Custom-board dialog: pick width, height, and mine count, then start.
    fn custom_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_custom {
            return;
        }
        let mut open = true;
        let mut start = false;
        egui::Window::new("Custom board")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("custom_fields").show(ui, |ui| {
                    ui.label("Width (cols)");
                    ui.add(egui::DragValue::new(&mut self.custom_cols).range(5..=40));
                    ui.end_row();
                    ui.label("Height (rows)");
                    ui.add(egui::DragValue::new(&mut self.custom_rows).range(5..=24));
                    ui.end_row();
                    // Cap mines so a 3x3 first-click safe zone always fits.
                    let max_mines = (self.custom_rows * self.custom_cols)
                        .saturating_sub(9)
                        .max(1);
                    self.custom_mines = self.custom_mines.clamp(1, max_mines);
                    ui.label("Mines");
                    ui.add(egui::DragValue::new(&mut self.custom_mines).range(1..=max_mines));
                    ui.end_row();
                });
                ui.add_space(4.0);
                if ui.button("Start").clicked() {
                    start = true;
                }
            });
        if start {
            let (r, c, m) = (self.custom_rows, self.custom_cols, self.custom_mines);
            self.apply_board(r, c, m, Difficulty::Custom);
            self.show_custom = false;
        } else if !open {
            self.show_custom = false;
        }
    }

    fn draw_cell(&self, ui: &mut egui::Ui, r: usize, c: usize) -> egui::Response {
        let cell = self.grid[r][c];
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(CELL, CELL), egui::Sense::click());
        let painter = ui.painter();
        let revealed = cell.state == CellState::Revealed;

        // Background.
        let bg = if revealed {
            if Some((r, c)) == self.exploded {
                egui::Color32::from_rgb(220, 60, 60) // the mine you hit
            } else {
                egui::Color32::from_gray(198)
            }
        } else if resp.hovered() && self.status == Status::Playing {
            egui::Color32::from_gray(210)
        } else {
            egui::Color32::from_gray(189)
        };
        painter.rect_filled(rect, 2.0, bg);

        if revealed {
            // Thin grid line so revealed cells read as a grid.
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(160)),
                egui::StrokeKind::Inside,
            );
        } else {
            // Raised bevel: light top/left, dark bottom/right.
            let light = egui::Color32::from_gray(232);
            let dark = egui::Color32::from_gray(140);
            let b = 3.0;
            let (min, max) = (rect.min, rect.max);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    min,
                    egui::pos2(max.x, min.y),
                    egui::pos2(max.x - b, min.y + b),
                    egui::pos2(min.x + b, min.y + b),
                    egui::pos2(min.x + b, max.y - b),
                    min,
                ],
                light,
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    max,
                    egui::pos2(min.x, max.y),
                    egui::pos2(min.x + b, max.y - b),
                    egui::pos2(max.x - b, max.y - b),
                    egui::pos2(max.x - b, min.y + b),
                    max,
                ],
                dark,
                egui::Stroke::NONE,
            ));
        }

        let center = rect.center();
        match cell.state {
            // After a loss, a flag on a non-mine is exposed as a wrong guess: the
            // (absent) mine drawn with a red cross through it, classic-style.
            CellState::Flagged if self.status == Status::Lost && !cell.mine => {
                draw_mine(painter, rect);
                draw_cross(painter, rect);
            }
            CellState::Flagged => draw_flag(painter, rect),
            // "?" is plain ASCII, so it's safe to draw as text on every platform.
            CellState::Question => {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "?",
                    egui::FontId::proportional(CELL * 0.6),
                    egui::Color32::from_gray(60),
                );
            }
            CellState::Revealed if cell.mine => draw_mine(painter, rect),
            CellState::Revealed if cell.adjacent > 0 => {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    cell.adjacent.to_string(),
                    egui::FontId::proportional(CELL * 0.6),
                    number_color(cell.adjacent),
                );
            }
            _ => {}
        }

        resp
    }
}

fn number_color(n: u8) -> egui::Color32 {
    match n {
        1 => egui::Color32::from_rgb(0, 0, 255),
        2 => egui::Color32::from_rgb(0, 130, 0),
        3 => egui::Color32::from_rgb(220, 0, 0),
        4 => egui::Color32::from_rgb(0, 0, 130),
        5 => egui::Color32::from_rgb(130, 0, 0),
        6 => egui::Color32::from_rgb(0, 130, 130),
        7 => egui::Color32::from_rgb(0, 0, 0),
        _ => egui::Color32::from_rgb(120, 120, 120),
    }
}

/// Draw a classic red flag on a black pole.
fn draw_flag(painter: &egui::Painter, rect: egui::Rect) {
    let w = rect.width();
    let p = rect.min;
    let pole_x = p.x + w * 0.62;
    let top = p.y + w * 0.22;
    let bottom = p.y + w * 0.74;
    // Flag triangle.
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(pole_x, top),
            egui::pos2(pole_x, top + w * 0.26),
            egui::pos2(pole_x - w * 0.34, top + w * 0.13),
        ],
        egui::Color32::from_rgb(210, 30, 30),
        egui::Stroke::NONE,
    ));
    // Pole.
    painter.line_segment(
        [egui::pos2(pole_x, top), egui::pos2(pole_x, bottom)],
        egui::Stroke::new(2.0, egui::Color32::BLACK),
    );
    // Base.
    painter.line_segment(
        [
            egui::pos2(pole_x - w * 0.18, bottom),
            egui::pos2(pole_x + w * 0.18, bottom),
        ],
        egui::Stroke::new(3.0, egui::Color32::BLACK),
    );
}

/// Draw a red cross — used to mark a wrong flag (a flag on a non-mine cell).
fn draw_cross(painter: &egui::Painter, rect: egui::Rect) {
    let m = rect.shrink(rect.width() * 0.2);
    let stroke = egui::Stroke::new(2.5, egui::Color32::from_rgb(200, 0, 0));
    painter.line_segment([m.left_top(), m.right_bottom()], stroke);
    painter.line_segment([m.right_top(), m.left_bottom()], stroke);
}

/// Draw a mine: a black circle with a few spikes.
fn draw_mine(painter: &egui::Painter, rect: egui::Rect) {
    let center = rect.center();
    let radius = rect.width() * 0.22;
    let stroke = egui::Stroke::new(2.0, egui::Color32::BLACK);
    // Spikes.
    for i in 0..4 {
        let a = std::f32::consts::FRAC_PI_4 * i as f32;
        let (s, co) = a.sin_cos();
        let dir = egui::vec2(co, s) * radius * 1.7;
        painter.line_segment([center - dir, center + dir], stroke);
    }
    painter.circle_filled(center, radius, egui::Color32::BLACK);
    // Little highlight.
    painter.circle_filled(
        center - egui::vec2(radius * 0.3, radius * 0.3),
        radius * 0.28,
        egui::Color32::from_gray(230),
    );
}

// ---------------------------------------------------------------------------
// Tests — pure game logic only (no egui). See CLAUDE.md "Testing philosophy".
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Count mines on the board.
    fn mine_count(g: &Minesweeper) -> usize {
        g.grid.iter().flatten().filter(|cell| cell.mine).count()
    }

    #[test]
    fn presets_have_expected_dims() {
        assert_eq!(Difficulty::Beginner.dims(), Some((9, 9, 10)));
        assert_eq!(Difficulty::Intermediate.dims(), Some((16, 16, 40)));
        assert_eq!(Difficulty::Expert.dims(), Some((16, 30, 99)));
        assert_eq!(Difficulty::Custom.dims(), None);
    }

    #[test]
    fn new_board_matches_difficulty() {
        for d in [
            Difficulty::Beginner,
            Difficulty::Intermediate,
            Difficulty::Expert,
        ] {
            let (rows, cols, mines) = d.dims().unwrap();
            let g = Minesweeper::new(d);
            assert_eq!(g.rows, rows);
            assert_eq!(g.cols, cols);
            assert_eq!(g.mines, mines);
            assert_eq!(g.grid.len(), rows);
            assert_eq!(g.grid[0].len(), cols);
        }
    }

    #[test]
    fn place_mines_lays_exact_count_for_each_preset() {
        for d in [
            Difficulty::Beginner,
            Difficulty::Intermediate,
            Difficulty::Expert,
        ] {
            let mut g = Minesweeper::new(d);
            g.place_mines(0, 0);
            assert_eq!(mine_count(&g), g.mines, "{} mine count", d.label());
        }
    }

    #[test]
    fn custom_board_has_requested_shape() {
        let mut g = Minesweeper::with_dims(12, 20, 30, Difficulty::Custom);
        assert_eq!((g.rows, g.cols, g.mines), (12, 20, 30));
        g.place_mines(5, 5);
        assert_eq!(mine_count(&g), 30);
    }

    #[test]
    fn right_click_cycles_through_question_when_enabled() {
        let mut g = Minesweeper::new(Difficulty::Beginner);
        g.settings.question_marks = true;
        assert_eq!(g.grid[0][0].state, CellState::Hidden);
        g.toggle_flag(0, 0);
        assert_eq!(g.grid[0][0].state, CellState::Flagged);
        assert_eq!(g.flags, 1);
        g.toggle_flag(0, 0);
        assert_eq!(g.grid[0][0].state, CellState::Question);
        assert_eq!(g.flags, 0, "a ? does not count as a flag");
        g.toggle_flag(0, 0);
        assert_eq!(g.grid[0][0].state, CellState::Hidden);
    }

    #[test]
    fn right_click_skips_question_when_disabled() {
        let mut g = Minesweeper::new(Difficulty::Beginner);
        g.settings.question_marks = false;
        g.toggle_flag(0, 0);
        assert_eq!(g.grid[0][0].state, CellState::Flagged);
        g.toggle_flag(0, 0);
        assert_eq!(g.grid[0][0].state, CellState::Hidden);
    }

    #[test]
    fn question_marked_cell_is_still_revealable() {
        let mut g = Minesweeper::with_dims(5, 5, 1, Difficulty::Custom);
        // Force a known board: single mine in the far corner, rest safe.
        g.grid[4][4].mine = true;
        g.mines_placed = true;
        g.grid[0][0].state = CellState::Question;
        g.reveal(0, 0);
        // A "?" cell reveals like a hidden one (it is not protected).
        assert_eq!(g.grid[0][0].state, CellState::Revealed);
    }

    /// Fill in adjacency counts for a hand-placed board (place_mines also does
    /// this, but tests that set mines manually need it on its own).
    fn recompute_adjacency(g: &mut Minesweeper) {
        for r in 0..g.rows {
            for c in 0..g.cols {
                if g.grid[r][c].mine {
                    continue;
                }
                let count = g
                    .neighbors(r, c)
                    .filter(|&(nr, nc)| g.grid[nr][nc].mine)
                    .count() as u8;
                g.grid[r][c].adjacent = count;
            }
        }
    }

    #[test]
    fn chord_reveals_neighbors_when_flags_match() {
        let mut g = Minesweeper::with_dims(3, 3, 1, Difficulty::Custom);
        g.grid[0][0].mine = true;
        g.mines_placed = true;
        recompute_adjacency(&mut g);

        // Reveal the center (adjacent == 1, so it reveals only itself).
        g.reveal(1, 1);
        assert_eq!(g.grid[1][1].state, CellState::Revealed);
        assert_eq!(g.grid[1][1].adjacent, 1);

        // Flagging the one mine satisfies the center; chording it opens the rest.
        g.toggle_flag(0, 0);
        g.reveal(1, 1); // a click on a revealed cell chords
        assert_eq!(g.grid[0][1].state, CellState::Revealed);
        assert_eq!(g.grid[1][0].state, CellState::Revealed);
        assert_eq!(g.grid[0][0].state, CellState::Flagged);
    }

    #[test]
    fn chord_is_a_noop_when_flags_do_not_match() {
        let mut g = Minesweeper::with_dims(3, 3, 1, Difficulty::Custom);
        g.grid[0][0].mine = true;
        g.mines_placed = true;
        recompute_adjacency(&mut g);

        g.reveal(1, 1);
        // No flags placed yet: chording the center must reveal nothing new.
        g.reveal(1, 1);
        assert_eq!(g.grid[0][1].state, CellState::Hidden);
        assert_eq!(g.grid[1][0].state, CellState::Hidden);
    }

    #[test]
    fn flag_guard_caps_flags_at_mine_count() {
        let mut g = Minesweeper::with_dims(5, 5, 2, Difficulty::Custom);
        g.settings.flag_guard = true;
        g.toggle_flag(0, 0);
        g.toggle_flag(0, 1);
        assert_eq!(g.flags, 2);
        // The third flag is refused — there are only 2 mines.
        g.toggle_flag(0, 2);
        assert_eq!(g.flags, 2);
        assert_eq!(g.grid[0][2].state, CellState::Hidden);
    }

    #[test]
    fn without_flag_guard_flags_may_exceed_mines() {
        let mut g = Minesweeper::with_dims(5, 5, 1, Difficulty::Custom);
        g.settings.flag_guard = false;
        g.toggle_flag(0, 0);
        g.toggle_flag(0, 1);
        assert_eq!(g.flags, 2);
    }

    #[test]
    fn open_guarantee_clears_the_whole_first_click_neighborhood() {
        // Run several times: the RNG-seeded placement must always spare the 3x3.
        for _ in 0..20 {
            let mut g = Minesweeper::new(Difficulty::Beginner);
            g.settings.open_guarantee = true;
            g.place_mines(4, 4);
            assert!(!g.grid[4][4].mine);
            assert_eq!(g.grid[4][4].adjacent, 0, "click must open a region");
            for (nr, nc) in g.neighbors(4, 4) {
                assert!(!g.grid[nr][nc].mine, "3x3 safe zone must be mine-free");
            }
            assert_eq!(mine_count(&g), g.mines);
        }
    }

    #[test]
    fn without_open_guarantee_only_the_click_is_spared() {
        for _ in 0..20 {
            let mut g = Minesweeper::new(Difficulty::Beginner);
            g.settings.open_guarantee = false;
            g.place_mines(4, 4);
            // The clicked cell itself is always safe...
            assert!(!g.grid[4][4].mine);
            // ...and the full mine count is still placed.
            assert_eq!(mine_count(&g), g.mines);
        }
    }

    #[test]
    fn losing_reveals_missed_mines_and_marks_the_hit() {
        let mut g = Minesweeper::with_dims(3, 3, 1, Difficulty::Custom);
        g.grid[0][0].mine = true;
        g.mines_placed = true;
        recompute_adjacency(&mut g);

        // A wrong flag on a safe cell stays flagged so the renderer can cross it.
        g.toggle_flag(2, 2);
        g.reveal(0, 0); // step on the mine

        assert_eq!(g.status, Status::Lost);
        assert_eq!(g.exploded, Some((0, 0)));
        assert_eq!(g.grid[0][0].state, CellState::Revealed);
        assert_eq!(g.grid[2][2].state, CellState::Flagged);
        assert!(!g.grid[2][2].mine, "the wrong flag is on a non-mine");
    }

    #[test]
    fn losing_preserves_correct_flags() {
        let mut g = Minesweeper::with_dims(3, 3, 2, Difficulty::Custom);
        g.grid[0][0].mine = true; // will be correctly flagged
        g.grid[2][2].mine = true; // will be stepped on
        g.mines_placed = true;
        recompute_adjacency(&mut g);

        g.toggle_flag(0, 0); // a correct flag
        g.reveal(2, 2); // lose on the other mine

        assert_eq!(g.status, Status::Lost);
        assert_eq!(g.grid[0][0].state, CellState::Flagged);
        assert_eq!(g.grid[2][2].state, CellState::Revealed);
    }

    #[test]
    fn settings_persist_across_new_game() {
        let mut g = Minesweeper::new(Difficulty::Beginner);
        g.settings.question_marks = false;
        g.reset();
        assert!(!g.settings.question_marks);
        g.apply_board(16, 16, 40, Difficulty::Intermediate);
        assert!(!g.settings.question_marks);
    }
}
