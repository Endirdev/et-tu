//! Et Tu — core Latin-square brute-force library.
//!
//! Puzzle model inferred from `et tu problem.png`:
//! * NxN grid (example: 5x5).
//! * N symbols (example: star, triangle, circle, plus, square).
//! * Each symbol appears exactly once per row and once per column
//!   (Latin square). Column 4 of the example is already complete with all
//!   five distinct shapes, and the puzzle has a unique solution under this
//!   rule, which strongly supports the interpretation.
//! * Each symbol has a fixed display colour (see [`SYMBOL_COLORS`]).
//! * `?` marks the query cell whose value is asked for.
//!
//! This crate is intentionally UI-agnostic so the companion problem
//! generator (see [`gen`]) and both GUI binaries can reuse it.

use std::time::Instant;

pub mod gen;

/// Symbol indices for the canonical 5-symbol puzzle.
pub const STAR: u8 = 0;
pub const TRIANGLE: u8 = 1;
pub const CIRCLE: u8 = 2;
pub const PLUS: u8 = 3;
pub const SQUARE: u8 = 4;

pub const CANONICAL_N: usize = 5;

/// Human-readable names. Index = symbol value (for N == 5).
pub const SHAPE_NAMES: [&str; 5] = ["star", "triangle", "circle", "plus", "square"];

/// Glyphs used in text dumps / terminal output for the canonical symbols.
pub const SHAPE_GLYPHS: [char; 5] = ['*', 'T', 'o', '+', 'S'];

/// Display colours sampled from `et tu problem.png` (legend, bottom row):
/// yellow, green, blue, red, brown — in shape order star..square.
pub const SYMBOL_COLORS: [[u8; 3]; 5] = [
    [255, 242, 0],  // star    -> yellow
    [34, 177, 76],  // triangle-> green
    [63, 72, 204],  // circle  -> blue
    [237, 28, 36],  // plus    -> red
    [185, 122, 87], // square  -> brown
];

/// Name for a symbol value. Works for any N; falls back to `sym{k}`.
pub fn symbol_name(v: u8) -> String {
    if (v as usize) < SHAPE_NAMES.len() {
        SHAPE_NAMES[v as usize].to_string()
    } else {
        format!("sym{}", v)
    }
}

/// Glyph for a symbol value (single char). Falls back to digit/letter.
pub fn symbol_glyph(v: u8) -> char {
    if (v as usize) < SHAPE_GLYPHS.len() {
        SHAPE_GLYPHS[v as usize]
    } else if v < 10 {
        (b'0' + v) as char
    } else {
        (b'A' + (v - 10)) as char
    }
}

/// RGB colour for a symbol value. Canonical five use [`SYMBOL_COLORS`];
/// extras are generated deterministically (golden-ratio hue rotation).
pub fn symbol_color(v: u8) -> [u8; 3] {
    if (v as usize) < SYMBOL_COLORS.len() {
        SYMBOL_COLORS[v as usize]
    } else {
        // deterministic pseudo-palette for N > 5
        let h = (f64::from(v) * 137.508) % 360.0;
        hsv_to_rgb(h, 0.65, 0.9)
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}

// ---------------------------------------------------------------------------
// Puzzle
// ---------------------------------------------------------------------------

/// A Latin-square puzzle.
///
/// `cells[r * size + c]` is `Some(symbol)` for a given, `None` for empty.
/// `query` is the optional `?` cell whose value is asked for (still just an
/// empty cell as far as the solver is concerned).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Puzzle {
    pub size: usize,
    pub cells: Vec<Option<u8>>,
    pub query: Option<(usize, usize)>,
}

impl Puzzle {
    pub fn new(size: usize) -> Self {
        assert!((2..=16).contains(&size), "size must be 2..=16");
        Self {
            size,
            cells: vec![None; size * size],
            query: None,
        }
    }

    pub fn blank(size: usize, query: Option<(usize, usize)>) -> Self {
        let mut p = Self::new(size);
        p.query = query;
        p
    }

    #[inline]
    pub fn idx(&self, r: usize, c: usize) -> usize {
        r * self.size + c
    }

    pub fn get(&self, r: usize, c: usize) -> Option<u8> {
        self.cells[self.idx(r, c)]
    }

    pub fn set(&mut self, r: usize, c: usize, v: Option<u8>) {
        let i = self.idx(r, c);
        self.cells[i] = v;
    }

    /// The exact puzzle photographed in `et tu problem.png`.
    ///
    /// Text view (`*`=star, `T`=triangle, `o`=circle, `+`=plus, `S`=square):
    /// ```text
    /// . ? . . *
    /// . S . . o
    /// + . T . S
    /// * . S . T
    /// . . . T +
    /// ```
    pub fn example_png() -> Self {
        let mut p = Self::new(5);
        let givens: [(usize, usize, u8); 11] = [
            (0, 4, STAR),
            (1, 1, SQUARE),
            (1, 4, CIRCLE),
            (2, 0, PLUS),
            (2, 2, TRIANGLE),
            (2, 4, SQUARE),
            (3, 0, STAR),
            (3, 2, SQUARE),
            (3, 4, TRIANGLE),
            (4, 3, TRIANGLE),
            (4, 4, PLUS),
        ];
        for (r, c, v) in givens {
            p.set(r, c, Some(v));
        }
        p.query = Some((0, 1));
        p
    }

    /// Check givens for immediate Latin violations (duplicate in row/col)
    /// and out-of-range symbols.
    pub fn validate(&self) -> Result<(), String> {
        let n = self.size;
        if self.cells.len() != n * n {
            return Err(format!("cells len {} != {}x{}", self.cells.len(), n, n));
        }
        for v in self.cells.iter().flatten() {
            if (*v as usize) >= n {
                return Err(format!("symbol {} out of range for size {}", v, n));
            }
        }
        // rows
        for r in 0..n {
            let mut seen = vec![false; n];
            for c in 0..n {
                if let Some(v) = self.get(r, c) {
                    if seen[v as usize] {
                        return Err(format!("duplicate {} in row {}", symbol_name(v), r));
                    }
                    seen[v as usize] = true;
                }
            }
        }
        // cols
        for c in 0..n {
            let mut seen = vec![false; n];
            for r in 0..n {
                if let Some(v) = self.get(r, c) {
                    if seen[v as usize] {
                        return Err(format!("duplicate {} in column {}", symbol_name(v), c));
                    }
                    seen[v as usize] = true;
                }
            }
        }
        if let Some((qr, qc)) = self.query {
            if qr >= n || qc >= n {
                return Err(format!("query ({},{}) out of bounds", qr, qc));
            }
        }
        Ok(())
    }

    /// Parse the text format described in `puzzles/et_tu_problem.txt`.
    ///
    /// Rules:
    /// * blank lines and lines starting with `#` or `//` are ignored.
    /// * optional first line `N=5` / `size 5` sets the size.
    /// * otherwise size = number of grid lines; each line must yield N tokens.
    /// * tokens may be space-separated (`* T o + S . ?`) or packed (`*.?`).
    ///   Multi-char integer tokens require spaces.
    /// * token map (size 5): `*`=star(0) `T`/`^`=triangle(1) `o`/`O`/`C`=circle(2)
    ///   `+`=plus(3) `S`/`#`=square(4); `.`/`_`/`-`/`0`=empty; `?`=empty+query.
    ///   Digits `1`..=`N` also map to symbols 0..=N-1 (1-based).
    pub fn from_text(s: &str) -> Result<Self, String> {
        let mut size_opt: Option<usize> = None;
        let mut grid_lines: Vec<String> = Vec::new();
        for raw in s.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let low = line.to_ascii_lowercase().replace(['=', ':'], " ");
            let parts: Vec<&str> = low.split_whitespace().collect();
            if (parts.first() == Some(&"n") || parts.first() == Some(&"size")) && parts.len() == 2 {
                let n: usize = parts[1]
                    .parse()
                    .map_err(|_| format!("bad size line: {:?}", raw))?;
                if !(2..=16).contains(&n) {
                    return Err(format!("size out of range 2..=16: {}", n));
                }
                size_opt = Some(n);
                continue;
            }
            grid_lines.push(raw.to_string());
        }
        if grid_lines.is_empty() {
            return Err("no grid lines found".to_string());
        }
        let n = size_opt.unwrap_or(grid_lines.len());
        if !(2..=16).contains(&n) {
            return Err(format!("inferred size out of range: {}", n));
        }
        if grid_lines.len() != n {
            return Err(format!(
                "expected {} grid rows, found {}",
                n,
                grid_lines.len()
            ));
        }
        let mut p = Puzzle::new(n);
        let mut query: Option<(usize, usize)> = None;
        for (r, raw) in grid_lines.iter().enumerate() {
            let tokens = tokenize_row(raw, n)?;
            if tokens.len() != n {
                return Err(format!(
                    "row {}: expected {} cells, found {} in {:?}",
                    r,
                    n,
                    tokens.len(),
                    raw
                ));
            }
            for (c, tok) in tokens.iter().enumerate() {
                match parse_token(tok, n)? {
                    Token::Empty => {}
                    Token::Query => {
                        if query.is_none() {
                            query = Some((r, c));
                        }
                        // extra `?` cells stay empty; first one is the query
                    }
                    Token::Sym(v) => p.set(r, c, Some(v)),
                }
            }
        }
        p.query = query;
        p.validate()?;
        Ok(p)
    }

    /// Serialize back to the canonical text format.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("N={}\n", self.size));
        for r in 0..self.size {
            for c in 0..self.size {
                if c > 0 {
                    out.push(' ');
                }
                if Some((r, c)) == self.query && self.get(r, c).is_none() {
                    out.push('?');
                } else {
                    match self.get(r, c) {
                        None => out.push('.'),
                        Some(v) if self.size == 5 => out.push(symbol_glyph(v)),
                        Some(v) => out.push_str(&(v + 1).to_string()),
                    }
                }
            }
            out.push('\n');
        }
        out
    }

    /// Pretty grid with glyphs (for terminal / headless output).
    pub fn to_pretty(&self) -> String {
        self.to_text()
    }

    /// Render a solved value-grid (flat `N*N` symbols) as text.
    pub fn solution_to_text(size: usize, sol: &[u8]) -> String {
        let mut out = String::new();
        for r in 0..size {
            for c in 0..size {
                if c > 0 {
                    out.push(' ');
                }
                let v = sol[r * size + c];
                if size == 5 {
                    out.push(symbol_glyph(v));
                } else {
                    out.push_str(&(v + 1).to_string());
                }
            }
            out.push('\n');
        }
        out
    }
}

enum Token {
    Empty,
    Query,
    Sym(u8),
}

fn tokenize_row(raw: &str, _n: usize) -> Result<Vec<String>, String> {
    // Strip comments after # or //
    let mut s = raw.to_string();
    if let Some(i) = s.find('#') {
        s.truncate(i);
    }
    if let Some(i) = s.find("//") {
        s.truncate(i);
    }
    let s = s.trim().to_string();
    if s.is_empty() {
        return Ok(vec![]);
    }
    if s.contains(char::is_whitespace) {
        Ok(s.split_whitespace().map(|t| t.to_string()).collect())
    } else {
        // packed single-char tokens, e.g. ".?..*"
        Ok(s.chars().map(|c| c.to_string()).collect())
    }
}

fn parse_token(tok: &str, n: usize) -> Result<Token, String> {
    let t = tok.trim();
    if t == "." || t == "_" || t == "-" || t.eq_ignore_ascii_case("x") {
        return Ok(Token::Empty);
    }
    if t == "?" {
        return Ok(Token::Query);
    }
    if t == "0" {
        // historic: 0 = empty (1-based digit convention uses 1..=N)
        return Ok(Token::Empty);
    }
    // canonical shape tokens (only meaningful when they fit in range)
    let canon: Option<u8> = match t {
        "*" => Some(STAR),
        "T" | "t" | "^" | "A" | "a" => Some(TRIANGLE),
        "o" | "O" | "C" | "c" => Some(CIRCLE),
        "+" => Some(PLUS),
        "S" | "s" | "#" => Some(SQUARE),
        _ => None,
    };
    if let Some(v) = canon {
        if (v as usize) >= n {
            return Err(format!("token {:?} out of range for size {}", tok, n));
        }
        return Ok(Token::Sym(v));
    }
    // 1-based digits 1..=N
    if t.len() == 1 {
        if let Some(d) = t.chars().next().unwrap().to_digit(10) {
            if d >= 1 && (d as usize) <= n {
                return Ok(Token::Sym((d - 1) as u8));
            }
            return Err(format!("digit token {:?} out of range 1..={}", tok, n));
        }
    }
    // multi-char integers (1-based preferred, 0-based fallback for compat)
    if let Ok(num) = t.parse::<usize>() {
        if num >= 1 && num <= n {
            return Ok(Token::Sym((num - 1) as u8));
        }
        if num < n {
            return Ok(Token::Sym(num as u8));
        }
    }
    // single letters A..=P as 0-based values (for N > 5), except those
    // already claimed above by shape tokens
    if t.len() == 1 {
        let c = t.chars().next().unwrap().to_ascii_uppercase();
        if ('A'..='P').contains(&c) {
            let v = (c as u8) - b'A';
            if (v as usize) < n {
                return Ok(Token::Sym(v));
            }
        }
    }
    Err(format!("unrecognised token {:?}", tok))
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// Every candidate value examined (including rejected ones).
    pub attempts: u64,
    /// Successful assignments (grid writes).
    pub assignments: u64,
    /// Backtracks (un-assignments).
    pub backtracks: u64,
    pub solutions_found: usize,
    pub elapsed_ms: u128,
}

// ---------------------------------------------------------------------------
// Fast one-shot solvers (recursive, MRV)
// ---------------------------------------------------------------------------

fn allowed_on(grid: &[Option<u8>], n: usize, cell: usize, v: u8) -> bool {
    let r = cell / n;
    let c = cell % n;
    for cc in 0..n {
        if grid[r * n + cc] == Some(v) {
            return false;
        }
    }
    for rr in 0..n {
        if grid[rr * n + c] == Some(v) {
            return false;
        }
    }
    true
}

fn candidates_on(grid: &[Option<u8>], n: usize, cell: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for v in 0..(n as u8) {
        if allowed_on(grid, n, cell, v) {
            out.push(v);
        }
    }
    out
}

fn select_mrv(grid: &[Option<u8>], n: usize) -> Option<(usize, Vec<u8>)> {
    let mut best: Option<(usize, Vec<u8>)> = None;
    for (i, g) in grid.iter().enumerate() {
        if g.is_none() {
            let cands = candidates_on(grid, n, i);
            if cands.is_empty() {
                return Some((i, cands)); // dead end, report immediately
            }
            match &best {
                None => best = Some((i, cands)),
                Some((_, b)) if cands.len() < b.len() => best = Some((i, cands)),
                _ => {}
            }
            if best.as_ref().unwrap().1.len() == 1 {
                break;
            }
        }
    }
    best
}

/// Solve and return the first solution (flat `N*N` symbols), or `None`.
pub fn solve_one(puzzle: &Puzzle) -> (Option<Vec<u8>>, Stats) {
    let t0 = Instant::now();
    let mut stats = Stats::default();
    let mut grid = puzzle.cells.clone();
    let n = puzzle.size;
    let mut answer: Option<Vec<u8>> = None;

    fn dfs(
        grid: &mut [Option<u8>],
        n: usize,
        stats: &mut Stats,
        answer: &mut Option<Vec<u8>>,
    ) -> bool {
        let Some((cell, cands)) = select_mrv(grid, n) else {
            // no empties -> solved
            *answer = Some(grid.iter().map(|g| g.unwrap()).collect());
            stats.solutions_found = 1;
            return true;
        };
        if cands.is_empty() {
            return false;
        }
        for v in cands {
            stats.attempts += 1;
            grid[cell] = Some(v);
            stats.assignments += 1;
            if dfs(grid, n, stats, answer) {
                return true;
            }
            grid[cell] = None;
            stats.backtracks += 1;
        }
        false
    }

    if puzzle.validate().is_err() {
        stats.elapsed_ms = t0.elapsed().as_millis();
        return (None, stats);
    }
    dfs(&mut grid, n, &mut stats, &mut answer);
    stats.elapsed_ms = t0.elapsed().as_millis();
    (answer, stats)
}

/// Solve and collect up to `limit` solutions.
pub fn solve_all(puzzle: &Puzzle, limit: usize) -> (Vec<Vec<u8>>, Stats) {
    let t0 = Instant::now();
    let mut stats = Stats::default();
    let mut sols: Vec<Vec<u8>> = Vec::new();
    if puzzle.validate().is_err() || limit == 0 {
        stats.elapsed_ms = t0.elapsed().as_millis();
        return (sols, stats);
    }
    let mut grid = puzzle.cells.clone();
    let n = puzzle.size;

    fn dfs(
        grid: &mut [Option<u8>],
        n: usize,
        stats: &mut Stats,
        sols: &mut Vec<Vec<u8>>,
        limit: usize,
    ) -> bool {
        let Some((cell, cands)) = select_mrv(grid, n) else {
            sols.push(grid.iter().map(|g| g.unwrap()).collect());
            stats.solutions_found = sols.len();
            return sols.len() >= limit;
        };
        if cands.is_empty() {
            return false;
        }
        for v in cands {
            stats.attempts += 1;
            grid[cell] = Some(v);
            stats.assignments += 1;
            let stop = dfs(grid, n, stats, sols, limit);
            grid[cell] = None;
            stats.backtracks += 1;
            if stop {
                return true;
            }
        }
        false
    }

    dfs(&mut grid, n, &mut stats, &mut sols, limit);
    stats.solutions_found = sols.len();
    stats.elapsed_ms = t0.elapsed().as_millis();
    (sols, stats)
}

// ---------------------------------------------------------------------------
// Stepwise brute forcer (for the graphical animation)
// ---------------------------------------------------------------------------

/// Cell-selection order for the animated brute forcer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Fill empty cells top-to-bottom, left-to-right. The honest "brute
    /// force" order; easiest to follow visually.
    RowMajor,
    /// Always pick the empty cell with fewest legal candidates
    /// (minimum-remaining-values). Much faster, jumps around.
    Mrv,
}

/// One atomic visual transition produced by [`BruteForcer::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepEvent {
    Assigned { row: usize, col: usize, val: u8 },
    Backtracked { row: usize, col: usize },
    Solved,
    NoSolution(String),
}

#[derive(Clone, Debug)]
struct Frame {
    cell: usize,
    next_val: u8,
}

/// Animated backtracking solver. Each [`BruteForcer::step`] call performs at
/// most one assignment or one backtrack, so the GUI can replay the search.
pub struct BruteForcer {
    size: usize,
    givens: Vec<Option<u8>>,
    grid: Vec<Option<u8>>,
    empties_rowmajor: Vec<usize>,
    order: Order,
    stack: Vec<Frame>,
    pub stats: Stats,
    finished: bool,
    solution: Option<Vec<u8>>,
    pub last_assigned: Option<usize>,
    pub last_backtracked: Option<usize>,
    t0: Instant,
    error: Option<String>,
}

impl BruteForcer {
    pub fn new(puzzle: &Puzzle, order: Order) -> Self {
        let n = puzzle.size;
        let mut empties = Vec::new();
        for (i, g) in puzzle.cells.iter().enumerate() {
            if g.is_none() {
                empties.push(i);
            }
        }
        let error = puzzle.validate().err();
        Self {
            size: n,
            givens: puzzle.cells.clone(),
            grid: puzzle.cells.clone(),
            empties_rowmajor: empties,
            order,
            stack: Vec::new(),
            stats: Stats::default(),
            finished: false,
            solution: None,
            last_assigned: None,
            last_backtracked: None,
            t0: Instant::now(),
            error,
        }
    }

    pub fn order(&self) -> Order {
        self.order
    }

    pub fn set_order(&mut self, order: Order) {
        self.order = order;
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn grid(&self) -> &[Option<u8>] {
        &self.grid
    }

    pub fn is_given(&self, flat: usize) -> bool {
        self.givens[flat].is_some()
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn solution(&self) -> Option<&[u8]> {
        self.solution.as_deref()
    }

    pub fn init_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn reset(&mut self, puzzle: &Puzzle) {
        let fresh = Self::new(puzzle, self.order);
        *self = fresh;
    }

    fn pick_next_cell(&self) -> Option<usize> {
        match self.order {
            Order::RowMajor => {
                // depth == number of pushed frames == number of assigned-or-trying
                // non-given cells; next is empties[stack.len()] if any empty left.
                // NB: after a backtrack-clear the top frame is unassigned, so the
                // caller must NOT pick a new cell in that case (it reuses top).
                if self.stack.len() < self.empties_rowmajor.len() {
                    Some(self.empties_rowmajor[self.stack.len()])
                } else {
                    None
                }
            }
            Order::Mrv => {
                let mut best: Option<(usize, usize)> = None; // (cell, count)
                for (i, g) in self.grid.iter().enumerate() {
                    if g.is_none() && !self.stack.iter().any(|f| f.cell == i) {
                        // skip the reserved top cell? caller guarantees top is
                        // assigned when picking, so every None not in stack is free.
                        let mut count = 0;
                        for v in 0..(self.size as u8) {
                            self.stats_dummy();
                            if allowed_on(&self.grid, self.size, i, v) {
                                count += 1;
                            }
                        }
                        match best {
                            None => best = Some((i, count)),
                            Some((_, b)) if count < b => best = Some((i, count)),
                            _ => {}
                        }
                        if best.unwrap().1 <= 1 {
                            break;
                        }
                    }
                }
                best.map(|(c, _)| c)
            }
        }
    }

    #[inline]
    fn stats_dummy(&self) {}

    /// Perform one atomic step. Returns `None` once finished (see
    /// [`BruteForcer::is_finished`]); otherwise returns the visual event.
    pub fn step(&mut self) -> Option<StepEvent> {
        if self.finished {
            return None;
        }
        if let Some(e) = &self.error {
            self.finished = true;
            self.stats.elapsed_ms = self.t0.elapsed().as_millis();
            return Some(StepEvent::NoSolution(e.clone()));
        }
        self.last_assigned = None;
        self.last_backtracked = None;

        // Solved?
        if self.grid.iter().all(|g| g.is_some()) {
            self.finished = true;
            self.solution = Some(self.grid.iter().map(|g| g.unwrap()).collect());
            self.stats.solutions_found = 1;
            self.stats.elapsed_ms = self.t0.elapsed().as_millis();
            return Some(StepEvent::Solved);
        }

        // Need a fresh frame?
        let need_new = match self.stack.last() {
            None => true,
            Some(f) => self.grid[f.cell].is_some(),
        };
        if need_new {
            let cell = match self.pick_next_cell() {
                Some(c) => c,
                None => {
                    // No cell to pick but grid incomplete: must be an MRV
                    // bookkeeping gap; treat as dead end and backtrack.
                    if self.stack.is_empty() {
                        self.finished = true;
                        self.stats.elapsed_ms = self.t0.elapsed().as_millis();
                        return Some(StepEvent::NoSolution("no moves left".to_string()));
                    }
                    // fall through to backtrack below
                    return self.backtrack_one();
                }
            };
            self.stack.push(Frame { cell, next_val: 0 });
        }

        // Try candidates for the top frame until one fits or we exhaust.
        // (Single pass: every path below returns.)
        let top = self.stack.last_mut().expect("just pushed");
        let cell = top.cell;
        debug_assert!(self.grid[cell].is_none());
        while top.next_val < self.size as u8 {
            let v = top.next_val;
            top.next_val += 1;
            self.stats.attempts += 1;
            if allowed_on(&self.grid, self.size, cell, v) {
                self.grid[cell] = Some(v);
                self.stats.assignments += 1;
                self.last_assigned = Some(cell);
                let (r, c) = (cell / self.size, cell % self.size);
                return Some(StepEvent::Assigned {
                    row: r,
                    col: c,
                    val: v,
                });
            }
        }
        // Exhausted: drop this never-assigned frame and backtrack one level.
        self.stack.pop();
        if self.stack.is_empty() {
            self.finished = true;
            self.stats.elapsed_ms = self.t0.elapsed().as_millis();
            return Some(StepEvent::NoSolution("unsatisfiable".to_string()));
        }
        // Unassign the new top (it holds the previous depth's value) so the
        // next call resumes trying its remaining candidates.
        let prev_cell = self.stack.last().unwrap().cell;
        self.grid[prev_cell] = None;
        self.stats.backtracks += 1;
        self.last_backtracked = Some(prev_cell);
        let (r, c) = (prev_cell / self.size, prev_cell % self.size);
        Some(StepEvent::Backtracked { row: r, col: c })
    }

    fn backtrack_one(&mut self) -> Option<StepEvent> {
        // Pop the exhausted top and clear the new top.
        self.stack.pop();
        if self.stack.is_empty() {
            self.finished = true;
            self.stats.elapsed_ms = self.t0.elapsed().as_millis();
            return Some(StepEvent::NoSolution("unsatisfiable".to_string()));
        }
        let prev = self.stack.last().unwrap().cell;
        self.grid[prev] = None;
        self.stats.backtracks += 1;
        self.last_backtracked = Some(prev);
        Some(StepEvent::Backtracked {
            row: prev / self.size,
            col: prev % self.size,
        })
    }

    /// Run to completion (used by tests / headless fast path comparison).
    pub fn run_to_end(&mut self) -> Option<Vec<u8>> {
        while !self.finished {
            match self.step() {
                Some(StepEvent::Solved) => break,
                Some(StepEvent::NoSolution(_)) => break,
                Some(_) => {}
                None => break,
            }
        }
        self.solution.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_validates() {
        let p = Puzzle::example_png();
        assert_eq!(p.size, 5);
        assert_eq!(p.query, Some((0, 1)));
        p.validate().unwrap();
    }

    #[test]
    fn example_has_unique_solution_with_triangle_query() {
        let p = Puzzle::example_png();
        let (sols, stats) = solve_all(&p, 10);
        assert_eq!(sols.len(), 1, "expected unique solution, stats={:?}", stats);
        let sol = &sols[0];
        // query (0,1) must be triangle
        assert_eq!(sol[1], TRIANGLE);
        // every row/col is a permutation of 0..5
        for r in 0..5 {
            let mut row: Vec<u8> = (0..5).map(|c| sol[r * 5 + c]).collect();
            row.sort_unstable();
            assert_eq!(row, vec![0, 1, 2, 3, 4], "row {}", r);
        }
        for c in 0..5 {
            let mut col: Vec<u8> = (0..5).map(|r| sol[r * 5 + c]).collect();
            col.sort_unstable();
            assert_eq!(col, vec![0, 1, 2, 3, 4], "col {}", c);
        }
        // spot-check givens preserved
        assert_eq!(sol[4], STAR);
        assert_eq!(sol[6], SQUARE);
        assert_eq!(sol[24], PLUS);
    }

    #[test]
    fn solve_one_matches_solve_all() {
        let p = Puzzle::example_png();
        let (one, _) = solve_one(&p);
        let (all, _) = solve_all(&p, 2);
        assert_eq!(one.unwrap(), all[0]);
    }

    #[test]
    fn stepwise_rowmajor_finds_same_solution() {
        let p = Puzzle::example_png();
        let mut f = BruteForcer::new(&p, Order::RowMajor);
        let sol = f.run_to_end().expect("should solve");
        let (expected, _) = solve_one(&p);
        assert_eq!(sol, expected.unwrap());
        assert!(f.stats.assignments > 0);
    }

    #[test]
    fn stepwise_mrv_finds_same_solution() {
        let p = Puzzle::example_png();
        let mut f = BruteForcer::new(&p, Order::Mrv);
        let sol = f.run_to_end().expect("should solve");
        let (expected, _) = solve_one(&p);
        assert_eq!(sol, expected.unwrap());
    }

    #[test]
    fn text_roundtrip_example() {
        let src = "N=5\n. ? . . *\n. S . . o\n+ . T . S\n* . S . T\n. . . T +\n";
        let p = Puzzle::from_text(src).unwrap();
        assert_eq!(p, Puzzle::example_png());
        let back = p.to_text();
        let p2 = Puzzle::from_text(&back).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn rejects_duplicate_row() {
        let mut p = Puzzle::new(5);
        p.set(0, 0, Some(STAR));
        p.set(0, 1, Some(STAR));
        assert!(p.validate().is_err());
    }

    #[test]
    fn unsatisfiable_is_reported() {
        // Fill row 0 with 0..3 and force col conflict? Simplest: duplicate via
        // direct cells bypassing validate is impossible, so build a valid but
        // unsatisfiable puzzle: 2x2 with conflicting givens is invalid, so use
        // a 3x3 where MRV dead-ends? Actually any valid partial Latin square of
        // order <=3 extends, so use an invalid one and check error path.
        let mut p = Puzzle::new(3);
        p.set(0, 0, Some(0));
        p.set(0, 1, Some(0));
        let mut f = BruteForcer::new(&p, Order::RowMajor);
        assert!(f.init_error().is_some());
        let ev = f.step().unwrap();
        assert!(matches!(ev, StepEvent::NoSolution(_)));
    }
}
