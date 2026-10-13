//! Et Tu — graphical brute forcer.
//!
//! Solves Latin-square shape puzzles like `et tu problem.png`:
//! each of the N symbols must appear exactly once per row and column.
//!
//! Usage:
//! * GUI (default): `cargo run`
//! * Headless: `cargo run -- --headless [--puzzle file] [--mrv]`
//!   or `--solve`, `--count N`.

use eframe::egui;
use et_tu::{solve_all, solve_one, symbol_name, BruteForcer, Order, Puzzle, StepEvent};

mod paint;
use paint::{color_word, paint_symbol};

// ---------------------------------------------------------------------------
// Headless mode
// ---------------------------------------------------------------------------

fn print_usage() {
    eprintln!("Et Tu — graphical brute forcer");
    eprintln!("GUI:      et_tu");
    eprintln!("Headless: et_tu --headless [--puzzle FILE] [--mrv] [--count LIMIT]");
    eprintln!("  --headless, --solve   solve without opening a window");
    eprintln!("  --puzzle FILE         load puzzle text file (default: built-in example)");
    eprintln!("  --mrv                 (headless uses MRV fast solver either way)");
    eprintln!("  --count LIMIT         print up to LIMIT solutions (default 1)");
    eprintln!();
    eprintln!("Puzzle text format (see puzzles/et_tu_problem.txt):");
    eprintln!("  * = star, T = triangle, o = circle, + = plus, S = square");
    eprintln!("  . = empty, ? = query cell");
}

fn run_headless(args: &[String]) -> i32 {
    let mut puzzle_path: Option<String> = None;
    let mut limit: usize = 1;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--puzzle" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--puzzle needs a file argument");
                    return 2;
                }
                puzzle_path = Some(args[i].clone());
            }
            "--count" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--count needs a number");
                    return 2;
                }
                limit = args[i].parse().unwrap_or(1).max(1);
            }
            "--mrv" | "--headless" | "--solve" => {}
            "--help" | "-h" => {
                print_usage();
                return 0;
            }
            other => {
                eprintln!("unknown arg: {}", other);
                print_usage();
                return 2;
            }
        }
        i += 1;
    }

    let puzzle = match puzzle_path {
        Some(p) => match std::fs::read_to_string(&p) {
            Ok(txt) => match Puzzle::from_text(&txt) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("failed to parse {}: {}", p, e);
                    return 1;
                }
            },
            Err(e) => {
                eprintln!("failed to read {}: {}", p, e);
                return 1;
            }
        },
        None => Puzzle::example_png(),
    };

    if limit == 1 {
        let (sol, stats) = solve_one(&puzzle);
        match sol {
            Some(s) => {
                println!("solved (size {}):", puzzle.size);
                print!("{}", Puzzle::solution_to_text(puzzle.size, &s));
                if let Some((qr, qc)) = puzzle.query {
                    let v = s[qr * puzzle.size + qc];
                    println!(
                        "query ({},{}) = {} ({})",
                        qr,
                        qc,
                        symbol_name(v),
                        color_word(v)
                    );
                }
                println!(
                    "stats: attempts={} assignments={} backtracks={} elapsed_ms={}",
                    stats.attempts, stats.assignments, stats.backtracks, stats.elapsed_ms
                );
                0
            }
            None => {
                eprintln!("no solution");
                eprintln!(
                    "stats: attempts={} assignments={} backtracks={}",
                    stats.attempts, stats.assignments, stats.backtracks
                );
                1
            }
        }
    } else {
        let (sols, stats) = solve_all(&puzzle, limit);
        println!("found {} solution(s) (limit {}):", sols.len(), limit);
        for (k, s) in sols.iter().enumerate() {
            println!("--- solution {} ---", k + 1);
            print!("{}", Puzzle::solution_to_text(puzzle.size, s));
        }
        println!(
            "stats: attempts={} assignments={} backtracks={} elapsed_ms={}",
            stats.attempts, stats.assignments, stats.backtracks, stats.elapsed_ms
        );
        if sols.is_empty() {
            1
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditTool {
    Place(u8),
    Erase,
    Query,
}

#[derive(Clone, PartialEq, Eq)]
enum RunState {
    Idle,
    Solving,
    Paused,
    Solved,
    NoSolution(String),
}

struct EtTuApp {
    puzzle: Puzzle,
    forcer: BruteForcer,
    order: Order,
    run_state: RunState,
    steps_per_frame: usize,
    show_colors: bool,
    edit_tool: EditTool,
    status: String,
    solution: Option<Vec<u8>>,
    puzzle_text: String,
}

impl EtTuApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let puzzle = Puzzle::example_png();
        let text = puzzle.to_text();
        let forcer = BruteForcer::new(&puzzle, Order::RowMajor);
        Self {
            puzzle,
            forcer,
            order: Order::RowMajor,
            run_state: RunState::Idle,
            steps_per_frame: 12,
            show_colors: true,
            edit_tool: EditTool::Query,
            status: "Loaded example from et tu problem.png. Press Solve.".to_string(),
            solution: None,
            puzzle_text: text,
        }
    }

    fn set_puzzle(&mut self, p: Puzzle, note: String) {
        self.puzzle_text = p.to_text();
        self.forcer = BruteForcer::new(&p, self.order);
        self.puzzle = p;
        self.solution = None;
        self.run_state = RunState::Idle;
        self.status = note;
    }

    fn start_solving(&mut self) {
        self.forcer = BruteForcer::new(&self.puzzle, self.order);
        if let Some(e) = self.forcer.init_error() {
            self.run_state = RunState::NoSolution(e.to_string());
            self.status = format!("Invalid puzzle: {}", e);
            return;
        }
        self.solution = None;
        self.run_state = RunState::Solving;
        self.status = format!(
            "Brute-forcing ({} order)…",
            if self.order == Order::RowMajor {
                "row-major"
            } else {
                "MRV"
            }
        );
    }

    fn do_step(&mut self) {
        if self.forcer.is_finished() {
            return;
        }
        match self.forcer.step() {
            Some(StepEvent::Assigned { .. }) | Some(StepEvent::Backtracked { .. }) => {
                self.status = format!(
                    "searching… attempts={} backtracks={}",
                    self.forcer.stats.attempts, self.forcer.stats.backtracks
                );
            }
            Some(StepEvent::Solved) => {
                self.solution = self.forcer.solution().map(|s| s.to_vec());
                self.run_state = RunState::Solved;
                self.status = format!(
                    "solved! attempts={} assignments={} backtracks={}",
                    self.forcer.stats.attempts,
                    self.forcer.stats.assignments,
                    self.forcer.stats.backtracks
                );
            }
            Some(StepEvent::NoSolution(msg)) => {
                self.run_state = RunState::NoSolution(msg.clone());
                self.status = format!("no solution ({})", msg);
            }
            None => {}
        }
    }

    fn fast_solve(&mut self) {
        let (sol, stats) = solve_one(&self.puzzle);
        match sol {
            Some(s) => {
                self.solution = Some(s);
                self.run_state = RunState::Solved;
                self.status = format!(
                    "solved instantly (MRV): attempts={} backtracks={} elapsed={}ms",
                    stats.attempts, stats.backtracks, stats.elapsed_ms
                );
            }
            None => {
                self.solution = None;
                self.run_state = RunState::NoSolution("unsatisfiable".to_string());
                self.status = "no solution".to_string();
            }
        }
    }

    fn reset_search(&mut self) {
        self.forcer = BruteForcer::new(&self.puzzle, self.order);
        self.solution = None;
        self.run_state = RunState::Idle;
        self.status = "Reset. Press Solve.".to_string();
    }

    /// Grid to display: solved solution if available and solved, else live
    /// brute-force grid.
    fn display_grid(&self) -> Vec<Option<u8>> {
        if self.run_state == RunState::Solved {
            if let Some(s) = &self.solution {
                return s.iter().map(|v| Some(*v)).collect();
            }
        }
        self.forcer.grid().to_vec()
    }

    fn apply_click(&mut self, r: usize, c: usize) {
        match self.edit_tool {
            EditTool::Place(v) => {
                if v as usize >= self.puzzle.size {
                    return;
                }
                self.puzzle.set(r, c, Some(v));
            }
            EditTool::Erase => {
                self.puzzle.set(r, c, None);
                if self.puzzle.query == Some((r, c)) {
                    self.puzzle.query = None;
                }
            }
            EditTool::Query => {
                self.puzzle.query = Some((r, c));
                if self.puzzle.get(r, c).is_some() {
                    // query must be empty: clear any given there
                    self.puzzle.set(r, c, None);
                }
            }
        }
        match self.puzzle.validate() {
            Ok(()) => {
                self.puzzle_text = self.puzzle.to_text();
                self.forcer = BruteForcer::new(&self.puzzle, self.order);
                self.solution = None;
                self.run_state = RunState::Idle;
                self.status = "Edited. Press Solve.".to_string();
            }
            Err(e) => {
                self.status = format!("edit rejected ({}); reverting", e);
                // revert by rebuilding puzzle from last good text? simplest:
                // keep the edit for visibility but forcer will report error.
                self.puzzle_text = self.puzzle.to_text();
                self.forcer = BruteForcer::new(&self.puzzle, self.order);
                self.solution = None;
                self.run_state = RunState::Idle;
            }
        }
    }
}

impl eframe::App for EtTuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // advance animation
        if self.run_state == RunState::Solving {
            for _ in 0..self.steps_per_frame {
                if self.run_state != RunState::Solving {
                    break;
                }
                self.do_step();
                if self.forcer.is_finished() {
                    break;
                }
            }
            ui.ctx().request_repaint();
        }

        ui.horizontal(|ui| {
            ui.heading("Et Tu — graphical brute forcer");
            ui.label("(Latin-square shapes: each symbol once per row & column)");
        });
        ui.separator();

        ui.horizontal(|ui| {
            // ---- left: grid ----
            ui.vertical(|ui| {
                ui.label(format!(
                    "Grid ({}x{}) — click a cell to apply the edit tool",
                    self.puzzle.size, self.puzzle.size
                ));
                let n = self.puzzle.size;
                let cell = (64.0_f32).min(360.0 / n as f32).max(30.0);
                let grid = self.display_grid();
                let query = self.puzzle.query;
                let last_a = self.forcer.last_assigned;
                let last_b = self.forcer.last_backtracked;
                for r in 0..n {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::splat(4.0);
                        for c in 0..n {
                            let flat = r * n + c;
                            let is_given = self.puzzle.cells[flat].is_some();
                            let is_query = query == Some((r, c));
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::Vec2::splat(cell), egui::Sense::click());
                            if resp.clicked() {
                                self.apply_click(r, c);
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let painter = ui.painter();
                            // background tint
                            let bg = if Some(flat) == last_a {
                                egui::Color32::from_rgb(220, 255, 220)
                            } else if Some(flat) == last_b {
                                egui::Color32::from_rgb(255, 220, 220)
                            } else if is_query {
                                egui::Color32::from_rgb(255, 252, 220)
                            } else if is_given {
                                egui::Color32::from_rgb(238, 238, 240)
                            } else {
                                egui::Color32::WHITE
                            };
                            painter.rect_filled(rect, 0.0, bg);
                            // border
                            let border = if is_query {
                                egui::Stroke::new(2.5, egui::Color32::from_rgb(230, 140, 0))
                            } else if is_given {
                                egui::Stroke::new(2.0, egui::Color32::BLACK)
                            } else {
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 205))
                            };
                            painter.rect_stroke(rect, 0.0, border, egui::StrokeKind::Inside);
                            // content
                            match grid[flat] {
                                Some(v) => {
                                    paint_symbol(painter, rect.shrink(5.0), v, self.show_colors, n)
                                }
                                None if is_query && self.run_state != RunState::Solved => {
                                    painter.text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "?",
                                        egui::FontId::proportional(cell * 0.55),
                                        egui::Color32::from_rgb(90, 90, 90),
                                    );
                                }
                                None => {}
                            }
                            // given tick: small corner dot for deduced vs given?
                            if grid[flat].is_some()
                                && !is_given
                                && self.run_state == RunState::Solved
                            {
                                // deduced cells get a tiny grey corner mark
                                let d = rect.left_top() + egui::Vec2::splat(3.0);
                                painter.circle_filled(
                                    d,
                                    2.0,
                                    egui::Color32::from_rgb(150, 150, 155),
                                );
                            }
                        }
                    });
                }
                // query answer line
                if let Some((qr, qc)) = self.puzzle.query {
                    if let Some(sol) = &self.solution {
                        let v = sol[qr * n + qc];
                        ui.label(format!(
                            "? at ({},{}) = {} ({})",
                            qr,
                            qc,
                            symbol_name(v),
                            color_word(v)
                        ));
                    } else {
                        ui.label(format!("? at ({},{}) — unsolved", qr, qc));
                    }
                }
                ui.label(format!("status: {}", self.status));
                ui.label(format!(
                    "attempts={} assignments={} backtracks={}",
                    self.forcer.stats.attempts,
                    self.forcer.stats.assignments,
                    self.forcer.stats.backtracks
                ));
            });

            ui.separator();

            // ---- right: legend + controls ----
            ui.vertical(|ui| {
                ui.heading("Legend (shape → colour)");
                let m = self.puzzle.size.min(5);
                for v in 0..m {
                    ui.horizontal(|ui| {
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::click());
                        if resp.clicked() {
                            self.edit_tool = EditTool::Place(v as u8);
                        }
                        paint_symbol(ui.painter(), rect, v as u8, true, 5);
                        let selected = self.edit_tool == EditTool::Place(v as u8);
                        let name = if self.puzzle.size == 5 {
                            format!("{} ({})", symbol_name(v as u8), color_word(v as u8))
                        } else {
                            format!("symbol {}", v + 1)
                        };
                        if ui.selectable_label(selected, name).clicked() {
                            self.edit_tool = EditTool::Place(v as u8);
                        }
                    });
                }
                ui.separator();
                ui.label("Edit tool (applies on grid click):");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.edit_tool, EditTool::Query, "query ?");
                    ui.radio_value(&mut self.edit_tool, EditTool::Erase, "erase");
                });
                ui.label("Place:");
                ui.horizontal(|ui| {
                    for v in 0..m {
                        ui.radio_value(
                            &mut self.edit_tool,
                            EditTool::Place(v as u8),
                            symbol_name(v as u8),
                        );
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let solve_btn = ui.button("▶ Solve (animate)");
                    if solve_btn.clicked() {
                        self.start_solving();
                    }
                    let pause_label = if self.run_state == RunState::Solving {
                        "⏸ Pause"
                    } else {
                        "▶ Resume"
                    };
                    if ui.button(pause_label).clicked() {
                        if self.run_state == RunState::Solving {
                            self.run_state = RunState::Paused;
                            self.status = "Paused.".to_string();
                        } else if self.run_state == RunState::Paused {
                            self.run_state = RunState::Solving;
                        }
                    }
                    if ui.button("Step").clicked() {
                        if self.run_state == RunState::Idle || self.run_state == RunState::Paused {
                            if self.forcer.init_error().is_some() {
                                self.start_solving();
                            } else {
                                self.run_state = RunState::Paused;
                                self.do_step();
                            }
                        } else if self.run_state == RunState::Solving {
                            self.do_step();
                        }
                    }
                    if ui.button("Reset").clicked() {
                        self.reset_search();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("⚡ Fast solve").clicked() {
                        self.fast_solve();
                    }
                    if ui.button("Load example").clicked() {
                        self.set_puzzle(
                            Puzzle::example_png(),
                            "Loaded example from et tu problem.png.".to_string(),
                        );
                    }
                    if ui.button("Clear").clicked() {
                        let n = self.puzzle.size;
                        self.set_puzzle(Puzzle::new(n), "Cleared.".to_string());
                    }
                });
                ui.add(
                    egui::Slider::new(&mut self.steps_per_frame, 1..=500)
                        .text("Steps / frame")
                        .logarithmic(true),
                );
                ui.horizontal(|ui| {
                    ui.label("Order:");
                    let mut ord = self.order;
                    ui.radio_value(&mut ord, Order::RowMajor, "row-major");
                    ui.radio_value(&mut ord, Order::Mrv, "smart MRV");
                    if ord != self.order {
                        self.order = ord;
                        self.forcer = BruteForcer::new(&self.puzzle, self.order);
                        self.solution = None;
                        if self.run_state == RunState::Solved {
                            self.run_state = RunState::Idle;
                        }
                    }
                });
                ui.checkbox(
                    &mut self.show_colors,
                    "Show colours (uncheck = black silhouettes)",
                );
                ui.separator();
                ui.label("Puzzle text (editable):");
                ui.add(
                    egui::TextEdit::multiline(&mut self.puzzle_text)
                        .desired_rows(7)
                        .desired_width(260.0)
                        .font(egui::TextStyle::Monospace),
                );
                ui.horizontal(|ui| {
                    if ui.button("Load text").clicked() {
                        match Puzzle::from_text(&self.puzzle_text) {
                            Ok(p) => self.set_puzzle(p, "Loaded from text.".to_string()),
                            Err(e) => self.status = format!("parse error: {}", e),
                        }
                    }
                    if ui.button("Revert text").clicked() {
                        self.puzzle_text = self.puzzle.to_text();
                    }
                });
                ui.separator();
                ui.label("Rules: each symbol exactly once per row and column.");
            });
        });
    }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| {
        a == "--headless"
            || a == "--solve"
            || a == "--count"
            || a == "--puzzle"
            || a == "--help"
            || a == "-h"
    }) {
        std::process::exit(run_headless(&args));
    }
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1040.0, 700.0]),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "Et Tu — graphical brute forcer",
        native_options,
        Box::new(|cc: &eframe::CreationContext| {
            Ok(Box::new(EtTuApp::new(cc)) as Box<dyn eframe::App>)
        }),
    ) {
        eprintln!("GUI failed to start: {:?}. Try --headless.", e);
        std::process::exit(1);
    }
}
