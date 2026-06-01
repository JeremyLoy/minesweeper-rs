// Minesweeper — a cross-platform GUI game built with egui/eframe.
//
// Classic rules: a 9x9 grid with 10 hidden mines.
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

const ROWS: usize = 9;
const COLS: usize = 9;
const MINES: usize = 10;
const CELL: f32 = 34.0; // pixel size of a single cell

fn main() -> eframe::Result {
    let board_w = COLS as f32 * CELL;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([board_w + 24.0, COLS as f32 * CELL + 110.0])
            .with_resizable(false)
            .with_title("Minesweeper"),
        ..Default::default()
    };
    eframe::run_native(
        "Minesweeper",
        options,
        Box::new(|_cc| Ok(Box::new(Minesweeper::new()))),
    )
}

/// A tiny xorshift64 PRNG so we don't need an external crate just to scatter
/// ten mines. Seeded from the system clock.
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

#[derive(Clone, Copy, PartialEq)]
enum CellState {
    Hidden,
    Revealed,
    Flagged,
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

#[derive(PartialEq, Clone, Copy)]
enum Status {
    Playing,
    Won,
    Lost,
}

struct Minesweeper {
    grid: Vec<Vec<Cell>>,
    status: Status,
    mines_placed: bool,
    revealed_safe: usize, // count of revealed non-mine cells
    flags: i32,
    start: Option<Instant>,
    final_time: Option<f64>,
    /// The cell that ended the game, drawn with a red background.
    exploded: Option<(usize, usize)>,
}

impl Minesweeper {
    fn new() -> Self {
        Minesweeper {
            grid: vec![vec![Cell::default(); COLS]; ROWS],
            status: Status::Playing,
            mines_placed: false,
            revealed_safe: 0,
            flags: 0,
            start: None,
            final_time: None,
            exploded: None,
        }
    }

    fn reset(&mut self) {
        *self = Minesweeper::new();
    }

    /// Place mines after the first click, keeping the clicked cell and its
    /// neighbors mine-free so the opening move reveals an area.
    fn place_mines(&mut self, safe_r: usize, safe_c: usize) {
        let mut candidates: Vec<(usize, usize)> = Vec::new();
        for r in 0..ROWS {
            for c in 0..COLS {
                let near = r.abs_diff(safe_r) <= 1 && c.abs_diff(safe_c) <= 1;
                if !near {
                    candidates.push((r, c));
                }
            }
        }
        // With 9x9/10 there is always room, but guard anyway: if excluding the
        // whole 3x3 safe zone leaves too few cells, fall back to only the click.
        if candidates.len() < MINES {
            candidates.clear();
            for r in 0..ROWS {
                for c in 0..COLS {
                    if (r, c) != (safe_r, safe_c) {
                        candidates.push((r, c));
                    }
                }
            }
        }

        // Partial Fisher–Yates: pick the first MINES after shuffling.
        let mut rng = Rng::new();
        for i in 0..MINES {
            let j = i + rng.below(candidates.len() - i);
            candidates.swap(i, j);
            let (r, c) = candidates[i];
            self.grid[r][c].mine = true;
        }

        // Compute adjacency counts.
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.grid[r][c].mine {
                    continue;
                }
                let mut count = 0;
                for (nr, nc) in neighbors(r, c) {
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
            CellState::Hidden => {}
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
            if cell.state != CellState::Hidden || cell.mine {
                continue;
            }
            cell.state = CellState::Revealed;
            self.revealed_safe += 1;
            if cell.adjacent == 0 {
                for (nr, nc) in neighbors(r, c) {
                    if self.grid[nr][nc].state == CellState::Hidden {
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
        let flagged = neighbors(r, c)
            .filter(|&(nr, nc)| self.grid[nr][nc].state == CellState::Flagged)
            .count() as u8;
        if flagged != cell.adjacent {
            return;
        }
        for (nr, nc) in neighbors(r, c) {
            if self.grid[nr][nc].state == CellState::Hidden {
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
        match self.grid[r][c].state {
            CellState::Hidden => {
                self.grid[r][c].state = CellState::Flagged;
                self.flags += 1;
            }
            CellState::Flagged => {
                self.grid[r][c].state = CellState::Hidden;
                self.flags -= 1;
            }
            CellState::Revealed => {}
        }
    }

    fn lose(&mut self) {
        self.status = Status::Lost;
        self.freeze_time();
        // Expose every mine.
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.grid[r][c].mine && self.grid[r][c].state != CellState::Flagged {
                    self.grid[r][c].state = CellState::Revealed;
                }
            }
        }
    }

    fn check_win(&mut self) {
        if self.revealed_safe == ROWS * COLS - MINES {
            self.status = Status::Won;
            self.freeze_time();
            // Auto-flag the remaining mines for a tidy finish.
            for r in 0..ROWS {
                for c in 0..COLS {
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
}

/// Yields the in-bounds neighbors of `(r, c)`.
fn neighbors(r: usize, c: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut out = Vec::with_capacity(8);
    for dr in -1i32..=1 {
        for dc in -1i32..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr >= 0 && nr < ROWS as i32 && nc >= 0 && nc < COLS as i32 {
                out.push((nr as usize, nc as usize));
            }
        }
    }
    out.into_iter()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

impl eframe::App for Minesweeper {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Keep the timer ticking while the game is live.
        if self.status == Status::Playing && self.mines_placed {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Panel::top("header").show_inside(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                // Mines remaining (mines minus flags placed).
                let remaining = MINES as i32 - self.flags;
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
            ui.vertical_centered(|ui| {
                let face = match self.status {
                    Status::Playing => ":)",
                    Status::Won => "B)",
                    Status::Lost => ":(",
                };
                if ui
                    .add_sized(
                        [44.0, 34.0],
                        egui::Button::new(egui::RichText::new(face).size(20.0).strong()),
                    )
                    .on_hover_text("New game")
                    .clicked()
                {
                    self.reset();
                }
                match self.status {
                    Status::Won => {
                        ui.label(egui::RichText::new("You win!").strong());
                    }
                    Status::Lost => {
                        ui.label(egui::RichText::new("Boom! Try again.").strong());
                    }
                    Status::Playing => {
                        ui.label("Left-click reveal · Right-click flag");
                    }
                }
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut left_click: Option<(usize, usize)> = None;
            let mut right_click: Option<(usize, usize)> = None;

            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for r in 0..ROWS {
                ui.horizontal(|ui| {
                    for c in 0..COLS {
                        let resp = self.draw_cell(ui, r, c);
                        if resp.clicked() {
                            left_click = Some((r, c));
                        }
                        if resp.secondary_clicked() {
                            right_click = Some((r, c));
                        }
                    }
                });
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
            CellState::Flagged => draw_flag(painter, rect),
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
