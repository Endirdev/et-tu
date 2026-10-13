//! Companion problem generator for Et Tu Latin-square shape puzzles.
//!
//! Pipeline:
//! 1. Draw a random complete Latin square (randomized MRV fill).
//! 2. Dig holes (remove givens) in random order while the puzzle keeps a
//!    unique solution (checked with [`crate::solve_all`] limited to 2).
//! 3. Pick the `?` query cell among the holes and score difficulty with a
//!    capped row-major [`crate::BruteForcer`] run.
//!
//! Everything is deterministic for a given [`GenOptions`] (including `seed`),
//! so puzzles are reproducible. No external RNG dependency: [`Rng`] is a
//! small SplitMix64 generator.

use std::time::Instant;

use crate::{solve_all, BruteForcer, Order, Puzzle};

// ---------------------------------------------------------------------------
// RNG
// ---------------------------------------------------------------------------

/// Deterministic SplitMix64 RNG (seedable, no dependencies).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish value in `0..bound` (panics if `bound == 0`).
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "Rng::below with bound 0");
        (self.next_u64() % bound as u64) as usize
    }

    /// Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------------------
// Options / output
// ---------------------------------------------------------------------------

/// What kind of puzzle to generate.
#[derive(Clone, Debug)]
pub struct GenOptions {
    /// Grid size (2..=9; larger sizes make uniqueness checks slow).
    pub size: usize,
    /// Desired number of givens. The digger removes clues while uniqueness
    /// holds and stops here; if uniqueness breaks first, the result keeps
    /// *more* givens than this target.
    pub givens: usize,
    /// Deterministic seed.
    pub seed: u64,
    /// Hole-digging passes over the cells (default 3).
    pub max_passes: usize,
    /// Forced query cell. It is pinned as a hole from the start; if no
    /// uniquely-solvable puzzle with that hole exists for the drawn solution,
    /// generation retries with a fresh solution (bounded).
    pub query: Option<(usize, usize)>,
}

impl Default for GenOptions {
    /// Mirrors the photographed puzzle: 5x5 with 11 givens.
    fn default() -> Self {
        Self {
            size: 5,
            givens: 11,
            seed: 0,
            max_passes: 3,
            query: None,
        }
    }
}

impl GenOptions {
    /// Preset matching the photographed puzzle (5x5, 11 givens).
    pub fn example_like(seed: u64) -> Self {
        Self {
            size: 5,
            givens: 11,
            seed,
            max_passes: 3,
            query: None,
        }
    }
}

/// A generated puzzle plus metadata.
#[derive(Clone, Debug)]
pub struct Generated {
    pub puzzle: Puzzle,
    /// The unique full solution (flat `size*size` symbols, row-major).
    pub solution: Vec<u8>,
    pub size: usize,
    pub givens: usize,
    pub seed: u64,
    /// How many `solve_all(limit 2)` uniqueness checks ran while digging.
    pub uniqueness_checks: usize,
    pub elapsed_ms: u128,
    /// Row-major brute-force effort to re-solve (capped; see
    /// `difficulty_capped`).
    pub difficulty_attempts: u64,
    pub difficulty_backtracks: u64,
    /// True if the difficulty probe hit its step cap (puzzle is hard).
    pub difficulty_capped: bool,
}

impl Generated {
    /// One-line human summary.
    pub fn summary(&self) -> String {
        let (qr, qc) = self.puzzle.query.unwrap_or((0, 0));
        let qv = self.solution[qr * self.size + qc];
        format!(
            "size={} givens={} seed={} query=({},{})={} difficulty(attempts={}, backtracks={}{}) checks={} elapsed={}ms",
            self.size,
            self.givens,
            self.seed,
            qr,
            qc,
            crate::symbol_name(qv),
            self.difficulty_attempts,
            self.difficulty_backtracks,
            if self.difficulty_capped { ", CAPPED" } else { "" },
            self.uniqueness_checks,
            self.elapsed_ms,
        )
    }
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Step cap for the row-major difficulty probe.
const DIFFICULTY_STEP_CAP: usize = 500_000;
/// Assignment budget for one random-fill attempt.
const FILL_BUDGET: usize = 100_000;
/// Random-fill restarts before giving up.
const FILL_RESTARTS: usize = 200;
/// Retries of the whole pipeline when a forced query hole won't stay unique.
const QUERY_RETRIES: usize = 50;

fn allowed(grid: &[Option<u8>], n: usize, cell: usize, v: u8) -> bool {
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

/// Randomized MRV fill of `grid` (in place). Returns true when complete.
fn fill_mrv(grid: &mut [Option<u8>], n: usize, rng: &mut Rng, budget: &mut usize) -> bool {
    let mut best_cell: Option<usize> = None;
    let mut best_cands: Vec<u8> = Vec::new();
    for (i, g) in grid.iter().enumerate() {
        if g.is_none() {
            let mut cands = Vec::with_capacity(n);
            for v in 0..n as u8 {
                if allowed(grid, n, i, v) {
                    cands.push(v);
                }
            }
            if cands.is_empty() {
                return false; // dead end
            }
            if best_cell.is_none() || cands.len() < best_cands.len() {
                best_cell = Some(i);
                best_cands = cands;
                if best_cands.len() == 1 {
                    break;
                }
            }
        }
    }
    let Some(cell) = best_cell else {
        return true; // no empties: complete
    };
    rng.shuffle(&mut best_cands);
    for v in best_cands {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        grid[cell] = Some(v);
        if fill_mrv(grid, n, rng, budget) {
            return true;
        }
        grid[cell] = None;
    }
    false
}

/// Draw a random complete Latin square (flat symbols, row-major).
fn random_complete_latin(rng: &mut Rng, n: usize) -> Result<Vec<u8>, String> {
    for _ in 0..FILL_RESTARTS {
        let mut grid = vec![None; n * n];
        let mut budget = FILL_BUDGET;
        if fill_mrv(&mut grid, n, rng, &mut budget) {
            return Ok(grid.into_iter().map(|g| g.unwrap()).collect());
        }
    }
    Err(format!(
        "could not draw a random {}x{} Latin square ({} restarts)",
        n, n, FILL_RESTARTS
    ))
}

fn count_givens(cells: &[Option<u8>]) -> usize {
    cells.iter().filter(|g| g.is_some()).count()
}

fn rowmajor_difficulty(puzzle: &Puzzle) -> (u64, u64, bool) {
    let mut f = BruteForcer::new(puzzle, Order::RowMajor);
    let mut steps = 0usize;
    while !f.is_finished() && steps < DIFFICULTY_STEP_CAP {
        match f.step() {
            Some(_) => steps += 1,
            None => break,
        }
    }
    let capped = !f.is_finished();
    (f.stats.attempts, f.stats.backtracks, capped)
}

/// Generate a uniquely-solvable puzzle per `opts`.
pub fn generate(opts: &GenOptions) -> Result<Generated, String> {
    let n = opts.size;
    if !(2..=9).contains(&n) {
        return Err(format!("size must be 2..=9 for generation, got {}", n));
    }
    if opts.max_passes == 0 {
        return Err("max_passes must be >= 1".to_string());
    }
    if let Some((qr, qc)) = opts.query {
        if qr >= n || qc >= n {
            return Err(format!(
                "query ({},{}) out of bounds for size {}",
                qr, qc, n
            ));
        }
    }
    // Need at least one hole for the query cell.
    let target = opts.givens.min(n * n - 1);

    let t0 = Instant::now();
    let mut rng = Rng::new(opts.seed);
    let mut uniqueness_checks = 0usize;

    for _attempt in 0..=QUERY_RETRIES {
        let solution = random_complete_latin(&mut rng, n)?;
        let mut puzzle = Puzzle::new(n);
        for (i, v) in solution.iter().enumerate() {
            puzzle.cells[i] = Some(*v);
        }

        // Pin the forced query hole, if any.
        if let Some((qr, qc)) = opts.query {
            let q = qr * n + qc;
            puzzle.cells[q] = None;
            let (sols, _) = solve_all(&puzzle, 2);
            uniqueness_checks += 1;
            if sols.len() != 1 {
                continue; // this solution can't host the forced hole: redraw
            }
        }

        // Dig holes while uniqueness holds.
        let mut order: Vec<usize> = (0..n * n).collect();
        // Never re-add the pinned query hole: keep it out of the dig order.
        if let Some((qr, qc)) = opts.query {
            order.retain(|&i| i != qr * n + qc);
        }
        for _ in 0..opts.max_passes {
            if count_givens(&puzzle.cells) <= target {
                break;
            }
            rng.shuffle(&mut order);
            let mut progress = false;
            for &idx in order.iter() {
                if count_givens(&puzzle.cells) <= target {
                    break;
                }
                if puzzle.cells[idx].is_none() {
                    continue;
                }
                let save = puzzle.cells[idx];
                puzzle.cells[idx] = None;
                let (sols, _) = solve_all(&puzzle, 2);
                uniqueness_checks += 1;
                if sols.len() == 1 {
                    progress = true;
                } else {
                    puzzle.cells[idx] = save;
                }
            }
            if !progress {
                break;
            }
        }

        // Choose the query cell among the holes.
        let holes: Vec<usize> = puzzle
            .cells
            .iter()
            .enumerate()
            .filter(|(_, g)| g.is_none())
            .map(|(i, _)| i)
            .collect();
        if holes.is_empty() {
            continue; // degenerate (shouldn't happen): redraw
        }
        let qflat = match opts.query {
            Some((qr, qc)) => qr * n + qc,
            None => holes[rng.below(holes.len())],
        };
        puzzle.query = Some((qflat / n, qflat % n));
        puzzle.validate()?;

        // Final uniqueness proof + solution cross-check.
        let (sols, _) = solve_all(&puzzle, 2);
        uniqueness_checks += 1;
        if sols.len() != 1 {
            return Err("internal error: final puzzle is not unique".to_string());
        }
        if sols[0] != solution {
            return Err("internal error: unique solution differs from drawn grid".to_string());
        }

        let givens = count_givens(&puzzle.cells);
        let (d_attempts, d_backtracks, d_capped) = rowmajor_difficulty(&puzzle);
        return Ok(Generated {
            puzzle,
            solution,
            size: n,
            givens,
            seed: opts.seed,
            uniqueness_checks,
            elapsed_ms: t0.elapsed().as_millis(),
            difficulty_attempts: d_attempts,
            difficulty_backtracks: d_backtracks,
            difficulty_capped: d_capped,
        });
    }
    Err(format!(
        "could not place forced query hole after {} retries (try another seed)",
        QUERY_RETRIES
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_name;

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = Rng::new(43);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn rng_shuffle_permutes() {
        let mut rng = Rng::new(7);
        let mut xs: Vec<u8> = (0..5).collect();
        rng.shuffle(&mut xs);
        let mut sorted = xs.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn random_latin_is_valid() {
        let mut rng = Rng::new(1234);
        for n in 2..=7 {
            let sq = random_complete_latin(&mut rng, n).unwrap();
            assert_eq!(sq.len(), n * n);
            for r in 0..n {
                let mut row: Vec<u8> = (0..n).map(|c| sq[r * n + c]).collect();
                row.sort_unstable();
                assert_eq!(row, (0..n as u8).collect::<Vec<_>>());
            }
            for c in 0..n {
                let mut col: Vec<u8> = (0..n).map(|r| sq[r * n + c]).collect();
                col.sort_unstable();
                assert_eq!(col, (0..n as u8).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn generate_is_deterministic() {
        let opts = GenOptions::example_like(20260704);
        let a = generate(&opts).unwrap();
        let b = generate(&opts).unwrap();
        assert_eq!(a.puzzle, b.puzzle);
        assert_eq!(a.solution, b.solution);
    }

    #[test]
    fn generate_example_like_is_unique() {
        let opts = GenOptions::example_like(99);
        let g = generate(&opts).unwrap();
        assert_eq!(g.size, 5);
        g.puzzle.validate().unwrap();
        // Target reached or stopped only because uniqueness broke.
        assert!(g.givens <= 14, "givens={}", g.givens);
        let (qr, qc) = g.puzzle.query.expect("query must be set");
        assert!(g.puzzle.get(qr, qc).is_none(), "query cell must be empty");
        let (sols, _) = solve_all(&g.puzzle, 2);
        assert_eq!(sols.len(), 1);
        assert_eq!(sols[0], g.solution);
        // Givens agree with the solution.
        for (i, cell) in g.puzzle.cells.iter().enumerate() {
            if let Some(v) = cell {
                assert_eq!(*v, g.solution[i]);
            }
        }
        let _ = symbol_name(g.solution[qr * 5 + qc]);
    }

    #[test]
    fn generate_small_sizes_unique() {
        for (n, givens, seed) in [(3usize, 4usize, 11u64), (4, 7, 12), (6, 16, 13)] {
            let opts = GenOptions {
                size: n,
                givens,
                seed,
                max_passes: 2,
                query: None,
            };
            let g = generate(&opts).unwrap();
            let (sols, _) = solve_all(&g.puzzle, 2);
            assert_eq!(sols.len(), 1, "n={} seed={}", n, seed);
            assert_eq!(sols[0], g.solution);
        }
    }

    #[test]
    fn generate_forced_query_honoured() {
        let opts = GenOptions {
            size: 5,
            givens: 11,
            seed: 5,
            max_passes: 3,
            query: Some((0, 1)),
        };
        let g = generate(&opts).unwrap();
        assert_eq!(g.puzzle.query, Some((0, 1)));
        assert!(g.puzzle.get(0, 1).is_none());
        let (sols, _) = solve_all(&g.puzzle, 2);
        assert_eq!(sols.len(), 1);
    }

    #[test]
    fn generate_rejects_bad_options() {
        assert!(generate(&GenOptions {
            size: 10,
            ..GenOptions::default()
        })
        .is_err());
        assert!(generate(&GenOptions {
            max_passes: 0,
            ..GenOptions::default()
        })
        .is_err());
    }
}
