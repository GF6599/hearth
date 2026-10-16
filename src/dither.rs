//! The ordered dither that the scenes share.

/// A 4x4 Bayer matrix. An ordered dither, not a random one, so a share that
/// holds still draws a pattern that holds still.
const BAYER: [[usize; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// The number of shares a tile tells apart.
pub const SHADES: usize = 16;

/// The place of the pixel at (x, row) in the order in which a tile lights,
/// from 0 to `SHADES - 1`, so each share is spread across the tile.
pub fn rank(x: usize, row: usize) -> usize {
    BAYER[row % 4][x % 4]
}

/// Whether `sixteenths` of a tile light the pixel at (x, row).
pub fn lights(sixteenths: usize, x: usize, row: usize) -> bool {
    rank(x, row) < sixteenths
}
