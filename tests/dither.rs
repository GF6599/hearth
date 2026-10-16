#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::dither::{self, SHADES};

/// The pixels of one tile that `sixteenths` light, from the upper left.
fn lit(sixteenths: usize, left: usize, top: usize) -> Vec<bool> {
    (0..16)
        .map(|at| dither::lights(sixteenths, left + at % 4, top + at / 4))
        .collect()
}

/// A share of the tile lights that share of its pixels, at any tile.
/// Given: each share from none to all sixteen sixteenths.
/// When: the pixels of a tile are read at several tile positions.
/// Then: exactly that many pixels are lit, and a pixel lit at one share
/// stays lit at every larger share.
///
/// Why it matters: a share that lights more or fewer pixels than it holds
/// bands a gradient, and a pixel that goes dark as the share grows flickers.
#[test]
fn a_share_lights_that_many_pixels_and_keeps_them_lit() {
    for (left, top) in [(0, 0), (4, 0), (8, 12), (3, 5)] {
        for sixteenths in 0..=SHADES {
            // When:
            let now = lit(sixteenths, left, top);

            // Then:
            let count = now.iter().filter(|on| **on).count();
            assert_eq!(count, sixteenths, "tile at ({left}, {top})");
            if sixteenths < SHADES {
                let next = lit(sixteenths + 1, left, top);
                assert!(
                    now.iter().zip(&next).all(|(was, is)| !was || *is),
                    "a pixel went dark from {sixteenths} sixteenths at ({left}, {top})"
                );
            }
        }
    }
}
