//! The coarse frame (ADR-0001): a rendered frame reduced to square cells of
//! mean RGB, stored as text in a scenario's golden and compared per cell
//! with a tolerance and a budget. Pure, so it is tested without a GPU.

use serde::{Deserialize, Serialize};

/// The cell edge in pixels (ADR-0001).
pub const CELL: u32 = 16;

/// A frame reduced to cells.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoarseFrame {
    /// The cell edge in pixels.
    pub cell: u32,
    /// The frame it was reduced from, `[width, height]` in pixels.
    pub size: [u32; 2],
    /// One string per row of cells, top to bottom: each cell's mean RGB as
    /// six hex digits, left to right. A cell the frame's edge cuts short
    /// averages the pixels it holds.
    pub rows: Vec<String>,
}

/// When two coarse frames count as the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tolerance {
    /// The largest difference of any channel's cell mean (0–255) that is
    /// not a difference.
    pub per_cell: u8,
    /// How many cells may differ by more than `per_cell` before the
    /// comparison fails.
    pub budget: usize,
}

impl Default for Tolerance {
    /// Frames are identical run to run on lavapipe (plans/c1-m0 step 4), so
    /// this only has to absorb another Mesa version's rasterisation: a
    /// per-pixel difference of a few levels on edges, averaged over 256
    /// pixels, stays far below 8. Any cell past it is a real change, so
    /// none is allowed by default.
    fn default() -> Self {
        Self {
            per_cell: 8,
            budget: 0,
        }
    }
}

/// A connected group of differing cells, as the pixels it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    /// `[min_x, min_y, max_x, max_y]` in pixels, max exclusive.
    pub rect: [u32; 4],
    pub cells: usize,
    /// The largest channel difference in the region.
    pub max_delta: u8,
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [x0, y0, x1, y1] = self.rect;
        write!(
            f,
            "x {x0}..{x1}, y {y0}..{y1} ({} cells, max delta {})",
            self.cells, self.max_delta
        )
    }
}

impl CoarseFrame {
    pub fn reduce(image: &image::RgbaImage, cell: u32) -> Self {
        let (width, height) = image.dimensions();
        let (cols, rows) = (width.div_ceil(cell), height.div_ceil(cell));
        let rows = (0..rows)
            .map(|row| {
                (0..cols)
                    .map(|col| {
                        let [x0, y0, x1, y1] = cell_rect(cell, [width, height], col, row);
                        let mut sum = [0u64; 3];
                        for y in y0..y1 {
                            for x in x0..x1 {
                                let p = image.get_pixel(x, y).0;
                                for c in 0..3 {
                                    sum[c] += u64::from(p[c]);
                                }
                            }
                        }
                        let n = u64::from((x1 - x0) * (y1 - y0));
                        let mean = sum.map(|s| (s + n / 2) / n);
                        format!("{:02x}{:02x}{:02x}", mean[0], mean[1], mean[2])
                    })
                    .collect()
            })
            .collect();
        Self {
            cell,
            size: [width, height],
            rows,
        }
    }

    fn cols(&self) -> u32 {
        self.size[0].div_ceil(self.cell)
    }

    /// Every cell's RGB, row-major, or why the frame is malformed.
    fn cells(&self) -> Result<Vec<[u8; 3]>, String> {
        let cols = self.cols() as usize;
        if self.rows.len() != self.size[1].div_ceil(self.cell) as usize {
            return Err(format!(
                "{} rows for a {:?} frame",
                self.rows.len(),
                self.size
            ));
        }
        let mut out = Vec::with_capacity(cols * self.rows.len());
        for (r, row) in self.rows.iter().enumerate() {
            if row.len() != cols * 6 || !row.is_ascii() {
                return Err(format!("row {r} is not {cols} cells of six hex digits"));
            }
            for c in 0..cols {
                let rgb = u32::from_str_radix(&row[c * 6..c * 6 + 6], 16)
                    .map_err(|_| format!("row {r}, cell {c} is not hex"))?;
                let [_, r8, g8, b8] = rgb.to_be_bytes();
                out.push([r8, g8, b8]);
            }
        }
        Ok(out)
    }
}

fn cell_rect(cell: u32, [width, height]: [u32; 2], col: u32, row: u32) -> [u32; 4] {
    [
        col * cell,
        row * cell,
        ((col + 1) * cell).min(width),
        ((row + 1) * cell).min(height),
    ]
}

/// Compares `actual` against `golden`. `Ok` holds the differing regions the
/// budget absorbed (usually none); `Err` says what failed, naming each
/// differing region by its pixels.
pub fn check(
    golden: &CoarseFrame,
    actual: &CoarseFrame,
    tolerance: Tolerance,
) -> Result<Vec<Region>, String> {
    if (golden.cell, golden.size) != (actual.cell, actual.size) {
        return Err(format!(
            "the coarse frame's shape changed: {:?} px in {} px cells, golden {:?} in {}",
            actual.size, actual.cell, golden.size, golden.cell
        ));
    }
    let a = golden
        .cells()
        .map_err(|e| format!("malformed golden: {e}"))?;
    let b = actual
        .cells()
        .map_err(|e| format!("malformed frame: {e}"))?;
    let delta: Vec<u8> = a
        .iter()
        .zip(&b)
        .map(|(p, q)| (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0))
        .collect();
    let regions = regions(actual, &delta, tolerance.per_cell);
    let cells: usize = regions.iter().map(|r| r.cells).sum();
    if cells <= tolerance.budget {
        return Ok(regions);
    }
    let list: Vec<String> = regions.iter().map(|r| format!("  {r}")).collect();
    Err(format!(
        "{cells} cells differ by more than {} (budget {}), in {} region(s):\n{}",
        tolerance.per_cell,
        tolerance.budget,
        regions.len(),
        list.join("\n")
    ))
}

/// The 4-connected groups of cells whose delta exceeds `per_cell`, in
/// row-major order of their first cell.
fn regions(frame: &CoarseFrame, delta: &[u8], per_cell: u8) -> Vec<Region> {
    let cols = frame.cols() as usize;
    let mut seen = vec![false; delta.len()];
    let mut out = Vec::new();
    for start in 0..delta.len() {
        if seen[start] || delta[start] <= per_cell {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let (mut c0, mut r0, mut c1, mut r1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut cells, mut max_delta) = (0, 0);
        while let Some(i) = stack.pop() {
            let (c, r) = (i % cols, i / cols);
            (c0, r0, c1, r1) = (c0.min(c), r0.min(r), c1.max(c), r1.max(r));
            cells += 1;
            max_delta = max_delta.max(delta[i]);
            let mut near = Vec::with_capacity(4);
            if c > 0 {
                near.push(i - 1);
            }
            if c + 1 < cols {
                near.push(i + 1);
            }
            if r > 0 {
                near.push(i - cols);
            }
            if i + cols < delta.len() {
                near.push(i + cols);
            }
            for j in near {
                if !seen[j] && delta[j] > per_cell {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        let [x0, y0, ..] = cell_rect(frame.cell, frame.size, c0 as u32, r0 as u32);
        let [.., x1, y1] = cell_rect(frame.cell, frame.size, c1 as u32, r1 as u32);
        out.push(Region {
            rect: [x0, y0, x1, y1],
            cells,
            max_delta,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100×40 frame (7×3 cells, the last column and row cut short) of one
    /// grey.
    fn flat() -> image::RgbaImage {
        image::RgbaImage::from_pixel(100, 40, image::Rgba([40, 40, 40, 255]))
    }

    fn paint(img: &mut image::RgbaImage, [x0, y0, x1, y1]: [u32; 4], rgb: [u8; 3]) {
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
            }
        }
    }

    #[test]
    fn reduces_to_cell_means_including_cut_cells() {
        let mut img = flat();
        // Half of cell (0, 0) white: its mean is halfway.
        paint(&mut img, [0, 0, 8, 16], [240, 240, 240]);
        // The cut corner cell (6, 2) is 4×8 pixels; all of it red.
        paint(&mut img, [96, 32, 100, 40], [200, 0, 0]);
        let frame = CoarseFrame::reduce(&img, 16);
        assert_eq!(frame.size, [100, 40]);
        assert_eq!(frame.rows.len(), 3);
        assert!(frame.rows.iter().all(|r| r.len() == 7 * 6));
        assert_eq!(&frame.rows[0][..6], "8c8c8c");
        assert_eq!(&frame.rows[0][6..12], "282828");
        assert_eq!(&frame.rows[2][36..], "c80000");
    }

    #[test]
    fn a_change_within_the_tolerance_passes() {
        let golden = CoarseFrame::reduce(&flat(), 16);
        let mut img = flat();
        paint(&mut img, [16, 16, 32, 32], [48, 40, 40]);
        let actual = CoarseFrame::reduce(&img, 16);
        assert_ne!(golden, actual);
        assert_eq!(check(&golden, &actual, Tolerance::default()), Ok(vec![]));
    }

    #[test]
    fn a_change_past_the_tolerance_fails_and_is_named() {
        let golden = CoarseFrame::reduce(&flat(), 16);
        let mut img = flat();
        paint(&mut img, [16, 16, 32, 32], [49, 40, 40]);
        let err = check(
            &golden,
            &CoarseFrame::reduce(&img, 16),
            Tolerance::default(),
        )
        .unwrap_err();
        assert!(
            err.contains("x 16..32, y 16..32 (1 cells, max delta 9)"),
            "{err}"
        );
    }

    #[test]
    fn a_change_over_the_budget_fails() {
        let golden = CoarseFrame::reduce(&flat(), 16);
        let mut img = flat();
        // Two separate regions: a bar along the cut bottom row, and one
        // cell at the top left.
        paint(&mut img, [0, 34, 100, 40], [255, 0, 0]);
        paint(&mut img, [0, 0, 16, 16], [255, 255, 255]);
        let actual = CoarseFrame::reduce(&img, 16);
        let tolerance = |budget| Tolerance {
            per_cell: 8,
            budget,
        };
        assert_eq!(
            check(&golden, &actual, tolerance(8)).map(|r| r.len()),
            Ok(2)
        );
        let err = check(&golden, &actual, tolerance(7)).unwrap_err();
        assert!(err.contains("8 cells differ by more than 8 (budget 7), in 2 region(s)"));
        assert!(err.contains("x 0..16, y 0..16 (1 cells"), "{err}");
        assert!(err.contains("x 0..100, y 32..40 (7 cells"), "{err}");
    }

    #[test]
    fn a_resized_or_malformed_frame_fails() {
        let golden = CoarseFrame::reduce(&flat(), 16);
        let smaller = CoarseFrame::reduce(&image::RgbaImage::new(96, 40), 16);
        assert!(check(&golden, &smaller, Tolerance::default()).is_err());
        let mut bad = golden.clone();
        bad.rows[1].replace_range(0..6, "zzzzzz");
        let err = check(&bad, &golden, Tolerance::default()).unwrap_err();
        assert!(err.contains("malformed golden: row 1, cell 0"), "{err}");
    }
}
