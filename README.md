# Et Tu — graphical brute forcer + problem generator

Solves and generates shape puzzles like `et tu problem.png`.

**Inferred rules** (no rules were given; this is the only interpretation under
which the photographed puzzle has a unique solution — verified by the test
suite):
- `N×N` grid (example: 5×5).
- `N` symbols (example: star, triangle, circle, plus, square).
- Each symbol appears **exactly once per row and once per column**
  (Latin square). Column 4 of the photo already contains all five shapes.
- Each symbol has a fixed colour (legend, bottom row of the photo):
  star→yellow `(255,242,0)`, triangle→green `(34,177,76)`,
  circle→blue `(63,72,204)`, plus→red `(237,28,36)`, square→brown `(185,122,87)`.
- `?` is the query cell. The photo's `?` at row 0, col 1 solves to **triangle (green)**.

## Run

```sh
cargo run                      # solver GUI (eframe/egui)
cargo run -- --headless        # solve built-in example, print solution
cargo run -- --headless --puzzle puzzles/et_tu_problem.txt --count 2
cargo run --bin et_tu_generator            # generator GUI (no args)
cargo run --bin et_tu_generator -- --seed 99 --givens 11
cargo test
```

## Puzzle text format

```
N=5
. ? . . *
. S . . o
+ . T . S
* . S . T
. . . T +
```

- `#` / `//` comments and blank lines ignored; optional `N=5` / `size 5` header.
- Cells may be space-separated or packed (`.?..*`).
- `*`=star `T`/`^`=triangle `o`/`C`=circle `+`=plus `S`/`#`=square,
  `.`/`_`/`-`/`0`=empty, `?`=empty+query, `1`..=`N`=symbols 1-based.

## GUI

- Left: animated grid. Thick black = given, orange = query `?`,
  green/red flash = last assign/backtrack. Solved deductions get a dot.
- Right: legend (click to pick the place-tool), edit tools (place/erase/query),
  `Solve (animate)` / `Pause` / `Step` / `Reset` / `Fast solve`,
  steps-per-frame slider, row-major vs smart-MRV order toggle,
  colour on/off, editable puzzle text.
- Shapes are drawn with `egui::Painter` in legend colours.

## Layout

- `src/lib.rs`: `Puzzle`, `solve_one`, `solve_all`, stepwise `BruteForcer`
  (`Order::RowMajor` for honest animation, `Order::Mrv` for speed), text I/O.
- `src/gen.rs`: generator core (`GenOptions`, `generate()`, deterministic
  `Rng`, random Latin squares, hole digging with uniqueness proofs,
  row-major difficulty scoring).
- `src/main.rs`: solver eframe GUI + `--headless` CLI.
- `src/bin/generator.rs`: generator eframe GUI + headless batch CLI.
- `src/paint.rs`: shape painting shared by both GUIs.

## Problem generator (`et_tu_generator`)

Pipeline: draw a random complete Latin square (randomized MRV fill), dig
holes in random order while `solve_all(limit 2)` still finds exactly one
solution, pin `?` on a hole, score difficulty with a capped row-major
brute-force probe. Fully deterministic per seed.

```sh
et_tu_generator --seed 99                              # print one 5x5/11 puzzle
et_tu_generator --size 7 --givens 20 --query 0,1 --out p7.txt
et_tu_generator --seed 7 --count 4 --out batches/      # batch into a directory
et_tu_generator --size 5 --givens 11 --no-solution      # omit the answer key
```

- Output files are solver-loadable (`#` comment lines carry the seed, the
  query answer, and the answer key, and are ignored by the text parser).
- `puzzles/sample_generated_5x5.txt` is a checked-in example.
- Roundtrip: `et_tu --headless --puzzle <generated file>` reproduces the
  claimed query answer (verified for 5×5 and 7×7 outputs).
- GUI: size/givens/seed/dig-passes controls, optional pinned `?`, live
  preview with solution overlay, difficulty readout, save-to-file and
  copy-to-clipboard (paste into the solver's puzzle-text box).
