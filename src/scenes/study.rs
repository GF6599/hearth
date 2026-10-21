//! A study desk by a window on a rainy night: a town behind the glass, a
//! cat on the sill, books, a lamp, a computer and a mug. One picture drawn
//! by hand, a character to an octant pixel, and laid out in letters two
//! pixels square, so a letter is as wide as it is tall. The rain and the
//! steam move a pixel at a time.

use std::ops::Range;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The pixels across and down in each letter of the picture: a cell across
/// and half a cell down, so a letter is as wide as it is tall.
const LETTER: usize = 2;

/// The room, a character to a pixel. Each character is a role of the
/// theme: `w` wall, `f` window frame, `s` night sky, `v` stars, `b` town,
/// `y` and `r` lit town windows, `T` moon, pages and the lamp's bulb, `l`
/// sill, `c` cat and `e` its shade, `d` desk top, `n` desk front, `m` metal
/// and the monitor, `k` screen, and `L`, `I`, `G`, `P`, `R`, `F` the
/// colours of books, lamp and mug. The rest are mixes of two roles that
/// shade them, as `SHADES` gives them. A cell shows two colours of its
/// pixels, so the art keeps each cell's two pixels across and four down to
/// two colours.
const ART: [&str; PICTURE.1 * LETTER] = [
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwPwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwGGwwwwwwwwwwwwwwwwPPwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLwwGGwwwwFFwwwwwwwwwwwPwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwffffffffffffffffffffffffffffffffffffffffffffffffffffffffwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLwwGGPPwwFFwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwffffffffffffffffffffffffffffffffffffffffffffffffffffffffwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLIIGGPPwwFFwwwwwwwwwwRRwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjssssssssssssssssssssssssfjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLIIGGPPRRFFIIwwwwwwwwRRwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjssssssssssssssssssssssssfjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLIIGGPPRRFFIIwwwwwwwwRRwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssTTTTsssssssssssssshffjsssssssssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLIIGGPPRRFFIIwwwwwwwwRRwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssTTTTTTsssssvssssssshffjssssssssssssssssssvssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwLLIIGGPPRRFFIIwwwwwwwwRRwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssTTMTTTTTsssssssssssshffjssssssssssvssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwddddddddddddddddddddddddddddddddddddddddddwwwwwwwwwwwwwwww",
    "wwwwwwhfssssTTTTTTTTsssssssssssshffjsssssssssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwddddddddddddddddddddddddddddddddddddddddddwwwwwwwwwwwwwwww",
    "wwwwwwhfssssTTTTTTTTsssssssssssshffjsssssssssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwzzwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwzzwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssTTTTMMTTsssssssssssshffjsssssssssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwzzwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwzzwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssTTTTTTssssssssssssshffjssssssssssssssssssssssvshjwwwwwwwwwwwwwwwwwwwwwwwwwwwzwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwzwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssTTTTsssssssssssssshffjsssssssssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjsssssbsssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjsssssbsssssssvsssssssssshjwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssssssssssssssssvssshffjsssssbsssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjsssssbsssssssssssssssssshjwwwwwwwwwwwwwwwwwwwwhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjbbbbbbbbsssssssssssssssshjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssvssssssssvssssssssssshffjbbbbbbbbssssssssvssssssshjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjbbybrbbbsssssssssssssssshjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjbbbbbbbbsssssssssssssssshjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwffffffffffffffffffffffffffffffffffffffffffffffffffffffffwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwffffffffffffffffffffffffffffffffffffffffffffffffffffffffwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjbbbbbbbbssssssssssssssssfjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfsssssssssssssssssssssssshffjbbbbbbbbssssssssssssssssfjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssbbbbbbbbsssssssssshffjbbbbbbebbbbbbbbbbcbbbbbbhjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssbbbbbbbbsssssssssshffjbbbbbbeecbbbbbbcccbbbbbbhjwwwwwwwwwwwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssbbrbbbbbsssssssssshffjbbbbbbeeccbbbbccccbbbbbbhjwwwwww888888wwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfssssssbbbbbbbbsssssssssshffjbbbbbbeeccccccccccbbbbybhjwwwww88888888wwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbbbbbssssssbbbbhffjbbbbbbeeccccccccccbbbbbbhjwwwwGGGGGGGGGGwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbybbbssssssbbbbhffjbbbbbbeeccccccccccbbbbbbhjwwwGGGGGGGGGGGGwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbybbbbbbbbbbbssssssbrbbhffjbbbbbbeeccccccccccbbbbbbhjwwGGGGGGGGGGGGGGwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbbbbbssssssbbbbhffjbbbbbbeeccccccccccbbbbbbhjwwGGGGGGGGGGGGGGwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbbbbbbbbbbbbbbbhffjbbbbbbbecccccccccbbbbbbbhjwwwwwwwTTTTwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbybbbbbbbbbbbbbbbhffjbbbbbbbbccccccccbbbbbbrbhjwwwwwwwwTTwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbrbbbbbbbbbbbybbbbbbbhffjbbbbbbbbccccccccbbbbbbbbhjwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbbbbbbbbbbbbbybhffjbbbbbbbbccccccccbbbbbbbbhjwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbbbrbbbbbbbbbbbhffjbbbbbbbccccccccccbbbbbbbhjwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbybbbbbbbbbbbbbbbrbbbbbhffjbbbbbbccccccccccccbbbbbbhjwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbybbbbbbbbbbbbbhffjbbbbbbccccccccccccbbbbbbhjwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwhfbbbbbbbbbbbbbbbbbbbbbbbbhffjbbbbbecccccccccccccbbbbbhjwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwffffffffffffffffffffffffffffffffffeeccccccccccccccffffffwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwffffffffffffffffffffffffffffffffffeeccccccccccccccffffffwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeccccccccccccccaaaaaaaaaawwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeccccccccccccccaaaaaaaaaawwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwhmkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkmjwwwwwwwwwwwwwwwwww",
    "wwwwwwIIIIIIIIIIIIIIIIIIIIIITTwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjww77RRRR55wwwwwwww",
    "wwwwwwIIIIIIIIIIIIIIIIIIIIII99wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjww77RRRR55wwwwwwww",
    "wwwwww2222222222222222222222TTwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwwwwwwwwwwwwwwwwwwwhhmmmmjjwwwwwwwwwwwwwwwwwwww77RRRR5555wwwwww",
    "wwwwwwIIIIIIIIIIIIIIIIIIIIII99wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwwwwwwwwwwwwwwwwwwwhhmmmmjjwwwwwwwwwwwwwwwwwwww77RRRR55ww5wwwww",
    "wwwwLLLLLLLLLLLLLLLLLLLLLLLLLLLLTTwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwwwwwwwwwwwwwwwwwwwhhmmmmjjwwwwwwwwwwwwwwwwwwww77RRRR55ww5wwwww",
    "wwwwLLLLLLLLLLLLLLLLLLLLLLLLLLLL99wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmwwwwwwwwwwwwwwwwwwwwwwwwwwwwhhmmmmjjwwwwwwwwwwwwwwwwwwww77RRRR55ww5wwwww",
    "wwww1111111111111111111111111111TTwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmmmmmmmmmmwwwwwwwwwwwwwwwwwwmmmmmmmmmmmmmmmmmmmmwwwwwwwwwwwwww77RRRR5555wwwwww",
    "wwwwLLLLLLLLLLLLLLLLLLLLLLLLLLLL99TTTTTTTTTTTTTTTTwwwwwwwwwwwwwwwwmmmmmmmmmmwwwwwwwwwwwwwwwwwwmmmmmmmmmmmmmmmmmmmmwwwwwwwwwwwwwwwwRRRRwwwwwwwwww",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaammmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaammmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmaaaaaaaaaaaaaaaaaaaaaaaa",
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnmmmmnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnmmmmnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
];

/// Two roles of a theme, for a mix of them.
type Pair = fn(&Palette) -> (Color, Color);

/// The characters of the art that mix two roles: the share of the second
/// role in the first. Each lights or shades a part in colours of its own,
/// and none mixes in the base, so a see-through base keeps every edge hard.
const SHADES: [(u8, Pair, f32); 11] = [
    (b'h', |p| (p.muted, p.subtle), 0.5),
    (b'j', |p| (p.muted, p.overlay), 0.5),
    (b'a', |p| (p.subtle, p.text), 0.3),
    (b'z', |p| (p.subtle, p.muted), 0.5),
    (b'M', |p| (p.text, p.subtle), 0.45),
    (b'9', |p| (p.text, p.subtle), 0.45),
    (b'1', |p| (p.love, p.muted), 0.4),
    (b'2', |p| (p.iris, p.muted), 0.4),
    (b'5', |p| (p.rose, p.love), 0.35),
    (b'7', |p| (p.rose, p.text), 0.45),
    (b'8', |p| (p.gold, p.text), 0.4),
];

/// The picture's width and height, in letters.
pub const PICTURE: (usize, usize) = (72, 40);
/// The part of the picture that a pane under `LEAST` shows: the window,
/// the cat and the sill, with a letter of wall on each side of the frame,
/// so the window is in its middle.
pub const WINDOW: Area = (2, 1, 30, 24);
/// The fewest cells across and down at which the whole room shows. A pane
/// cut further shows a strip of desk that reads as nothing.
pub const LEAST: (u16, u16) = (48, 16);
/// A part of the picture: left, top, width and height, in letters.
pub type Area = (usize, usize, usize, usize);

const SCREEN: Area = (42, 10, 20, 16);
/// The glass the rain runs on, the steam over the mug, the lamp with the
/// light it throws on the wall and the desk, and the room the cat's tail
/// swings in, the same way.
const RAIN: Area = (4, 3, 26, 19);
const STEAM: Area = (63, 17, 8, 9);
const LAMP: Area = (32, 15, 7, 16);
const SWING: Area = (25, 22, 7, 7);
/// The middle of the lamp's bulb, in letters, and the row of the desk it
/// lights.
const BULB: (f32, f32) = (35.5, 18.5);
const LIT_DESK: usize = 30;
/// The glow of the bulb on the wall under the shade: how far it reaches in
/// pixels, which keeps it inside the lamp's area, how far along the light
/// from the wall through rose to gold it reaches at the bulb, and the steps
/// it fades in.
const GLOW_REACH: f32 = 7.5;
const GLOW_PEAK: f32 = 0.9;
const GLOW_STEPS: usize = 8;
/// The columns of the desk, in pixels, that the lamp's pool of light takes
/// in gold, and in rose past them as far as the lamp's area reaches. Each
/// edge lies between two cells, so no cell holds both.
const POOL: Range<usize> = 68..74;
/// Each layer that moves, by name, and the area it draws in. No layer
/// draws outside its own area, so the rest of the room stays still.
pub const LAYERS: [(&str, Area); 5] = [
    ("rain", RAIN),
    ("steam", STEAM),
    ("lamp", LAMP),
    ("tail", SWING),
    ("screen", SCREEN),
];
const SYNTAX: [fn(&Palette) -> Color; 4] = [|p| p.iris, |p| p.foam, |p| p.gold, |p| p.subtle];
/// The characters of the picture outside the glass. The rain runs over
/// these only, because the frame and the cat are inside the room.
const OUTSIDE: &[u8] = b"sbyrTvM";
const DROPS: usize = 14;
/// The ticks from one letter typed on the screen to the next, half a
/// second. The cursor blinks at twice this.
const TYPING: u64 = 8;
/// The letters a finished line waits, counted in typed letters, before the
/// screen scrolls up.
const LINE_PAUSE: usize = 3;
/// Where the steam leaves the mug, how fast it rises in letters a tick,
/// and how far it sways to each side.
const SPOUT: (f64, f64) = (65.5, 25.0);
const RISE: f64 = 0.1;
const SWAY: f64 = 1.5;
/// A puff of steam fades into the wall as it rises: by this share of the
/// wall at most, in this many steps, over this many ticks.
const STEAM_FADE: f32 = 0.6;
const STEAM_FADES: usize = 4;
const STEAM_TICKS: f64 = 50.0;
/// The cat's tail at rest, swung left and swung right, hanging over the
/// front of the sill.
const TAILS: [[(usize, usize); 7]; 3] = [
    [
        (28, 22),
        (28, 23),
        (28, 24),
        (28, 25),
        (28, 26),
        (29, 27),
        (30, 26),
    ],
    [
        (28, 22),
        (28, 23),
        (27, 24),
        (27, 25),
        (26, 26),
        (26, 27),
        (25, 26),
    ],
    [
        (28, 22),
        (28, 23),
        (29, 24),
        (29, 25),
        (30, 26),
        (30, 27),
        (31, 26),
    ],
];
/// One flick of the tail, a pose for each three ticks: left, rest, right,
/// rest.
const FLICK: [usize; 4] = [1, 0, 2, 0];
const FLICK_TICKS: usize = 3;
/// The places the tail's brush takes along each letter of it, enough that
/// a diagonal leaves no gap.
const TAIL_STEPS: usize = 4;

/// One line of code on the screen: its indent, then each word's length and
/// colour.
#[derive(Debug, Clone)]
struct Code {
    indent: usize,
    words: Vec<(usize, Color)>,
}

impl Code {
    fn width(&self) -> usize {
        self.words
            .iter()
            .map(|(length, _)| length + 1)
            .sum::<usize>()
            - 1
    }
}

/// A streak of rain on the glass: its column in pixels, and the row of its
/// lowest point, its speed in a tick and its length, in letters.
#[derive(Debug, Clone, Copy)]
struct Streak {
    x: usize,
    y: f64,
    speed: f64,
    length: usize,
}

#[derive(Debug)]
pub struct Study {
    palette: &'static Palette,
    rng: Rng,
    ticks: u64,
    screen: Vec<Code>,
    /// The letters typed on the last line of the screen.
    typed: usize,
    rain: Vec<Streak>,
    /// The age of each wisp of steam in ticks, and where its sway starts.
    steam: Vec<(f64, f64)>,
    next_wisp: u32,
    /// The ticks left in the lamp's flicker, and the ticks until the next.
    flicker: u32,
    next_flicker: u32,
    /// The tick of the tail's flick, or none while it rests.
    flick: Option<usize>,
    next_flick: u32,
    /// The share of the way from the last tick to the next. The rain and
    /// the steam draw on from their tick by it, so they move a pixel at a
    /// time at any rate.
    progress: f64,
    /// The colour of each character of the art, and of each step of the
    /// lamp's glow on the wall, mixed once from the theme.
    colours: [Color; 128],
    glow: Vec<Color>,
    /// The shades of a puff of steam, from new to faded.
    steam_shades: Vec<Color>,
}

/// The colour of each character of the art: its role, or the mix that
/// `SHADES` gives it. A character the art does not use takes the wall's.
fn colours(palette: &Palette) -> [Color; 128] {
    let mut colours = [palette.overlay; 128];
    let roles = [
        (b'f', palette.muted),
        (b'm', palette.muted),
        (b'P', palette.pine),
        (b's', palette.base),
        (b'k', palette.base),
        (b'b', palette.overlay),
        (b'v', palette.subtle),
        (b'y', palette.gold),
        (b'G', palette.gold),
        (b'r', palette.rose),
        (b'R', palette.rose),
        (b'T', palette.text),
        (b'c', palette.text),
        (b'e', palette.subtle),
        (b'l', palette.subtle),
        (b'd', palette.subtle),
        (b'n', palette.surface),
        (b'L', palette.love),
        (b'I', palette.iris),
        (b'F', palette.foam),
    ];
    for (letter, colour) in roles {
        colours[usize::from(letter)] = colour;
    }
    for (letter, pair, share) in SHADES {
        let (from, to) = pair(palette);
        colours[usize::from(letter)] = octant::mix(from, to, share);
    }
    colours
}

impl Study {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        let mut study = Self {
            palette,
            rng,
            ticks: 0,
            screen: Vec::new(),
            typed: 0,
            rain: Vec::new(),
            steam: Vec::new(),
            next_wisp: 0,
            flicker: 0,
            next_flicker: 0,
            flick: None,
            next_flick: 0,
            progress: 0.0,
            colours: colours(palette),
            glow: (0..=GLOW_STEPS)
                .map(|step| {
                    let along = GLOW_PEAK * step as f32 / GLOW_STEPS as f32;
                    octant::ramp(&[palette.overlay, palette.rose, palette.gold], along)
                })
                .collect(),
            steam_shades: (0..STEAM_FADES)
                .map(|step| {
                    let share = STEAM_FADE * step as f32 / (STEAM_FADES - 1) as f32;
                    octant::mix(palette.subtle, palette.overlay, share)
                })
                .collect(),
        };
        let lines = SCREEN.3 / 2;
        study.screen = (0..lines).map(|_| study.code()).collect();
        study.rain = (0..DROPS).map(|_| study.streak(false)).collect();
        study.next_flicker = study.rng.random_range(300..900);
        study.next_flick = study.rng.random_range(80..250);
        study
    }

    fn code(&mut self) -> Code {
        let indent = self.rng.random_range(0..3);
        let mut room = SCREEN.2 - 4 - indent * 2;
        let mut words = Vec::new();
        while room >= 3 {
            let length = self.rng.random_range(2..=5).min(room - 1);
            let colour = SYNTAX[self.rng.random_range(0..SYNTAX.len())](self.palette);
            words.push((length, colour));
            room -= length + 1;
            if self.rng.random_bool(0.3) {
                break;
            }
        }
        Code { indent, words }
    }

    /// A new streak in a column of glass, anywhere on it at the start, and
    /// above it after, so the streaks do not start in step.
    fn streak(&mut self, above: bool) -> Streak {
        let (left, top, wide, tall) = RAIN;
        let x = loop {
            let x = self.rng.random_range(left * LETTER..(left + wide) * LETTER);
            if OUTSIDE.contains(&ART[top * LETTER].as_bytes()[x]) {
                break x;
            }
        };
        let y = if above {
            top as f64 - self.rng.random_range(0.0..12.0)
        } else {
            self.rng.random_range(top as f64..(top + tall) as f64)
        };
        Streak {
            x,
            y,
            speed: self.rng.random_range(0.25..0.7),
            length: self.rng.random_range(2..=4),
        }
    }

    /// The colour of each pixel of the picture, with the layers that move.
    fn pixels(&self) -> Vec<Vec<Color>> {
        let mut rows: Vec<Vec<Color>> = ART
            .iter()
            .map(|line| {
                line.bytes()
                    .map(|letter| self.colours[usize::from(letter)])
                    .collect()
            })
            .collect();
        self.draw_screen(&mut rows);
        self.draw_lamp(&mut rows);
        self.draw_tail(&mut rows);
        self.draw_rain(&mut rows);
        self.draw_steam(&mut rows);
        rows
    }

    /// Each line of code is a row of pixels across the top of its letters,
    /// and the cursor a letter tall.
    fn draw_screen(&self, rows: &mut [Vec<Color>]) {
        let (left, top, _, _) = SCREEN;
        let last = self.screen.len() - 1;
        for (line, code) in self.screen.iter().enumerate() {
            let y = (top + 1 + line * 2) * LETTER;
            let start = left + 2 + code.indent * 2;
            let shown = if line == last { self.typed } else { usize::MAX };
            let mut x = start;
            for &(length, colour) in &code.words {
                let typed = length.min(shown.saturating_sub(x - start));
                rows[y][x * LETTER..(x + typed) * LETTER].fill(colour);
                x += length + 1;
            }
            if line == last && (self.ticks / (TYPING * 2)).is_multiple_of(2) {
                let cursor = (start + self.typed.min(code.width())) * LETTER;
                rows[y][cursor] = self.palette.text;
                rows[y + 1][cursor] = self.palette.text;
            }
        }
    }

    fn draw_rain(&self, rows: &mut [Vec<Color>]) {
        let (_, top, _, tall) = RAIN;
        let glass = top * LETTER..(top + tall) * LETTER;
        for streak in &self.rain {
            // A streak that starts again on the next tick runs on below
            // the glass until then, so no frame draws it on its way back
            // to the top.
            let y = streak.y + streak.speed * self.progress;
            let lowest = (y * LETTER as f64).floor() as isize;
            let length = (streak.length * LETTER) as isize;
            for row in lowest + 1 - length..=lowest {
                let Ok(row) = usize::try_from(row) else {
                    continue;
                };
                if glass.contains(&row) && OUTSIDE.contains(&ART[row].as_bytes()[streak.x]) {
                    rows[row][streak.x] = self.palette.foam;
                }
            }
        }
    }

    /// Each puff of steam is a letter in size, placed to the pixel, and
    /// fades into the wall as it rises.
    fn draw_steam(&self, rows: &mut [Vec<Color>]) {
        let (left, top, wide, tall) = STEAM;
        let across = left * LETTER..(left + wide) * LETTER;
        let down = top * LETTER..(top + tall) * LETTER;
        let inside = |at: usize, within: &Range<usize>| {
            at.clamp(within.start, within.end)..(at + LETTER).clamp(within.start, within.end)
        };
        let scale = LETTER as f64;
        let last = self.steam_shades.len() - 1;
        for &(age, sway) in &self.steam {
            for behind in [0.0, 8.0, 16.0] {
                let age = age + self.progress - behind;
                if age < 0.0 {
                    continue;
                }
                let x = (SPOUT.0 + (age * 0.12 + sway).sin() * SWAY) * scale;
                let y = (SPOUT.1 - age * RISE) * scale;
                let fade = ((age / STEAM_TICKS) * last as f64).round() as usize;
                let colour = self.steam_shades[fade.min(last)];
                let columns = inside(x.round() as usize, &across);
                for row in &mut rows[inside(y.round() as usize, &down)] {
                    row[columns.clone()].fill(colour);
                }
            }
        }
    }

    /// Lit, the lamp's bulb glows on the wall under its shade and throws a
    /// pool of gold and rose on the desk. Dim, as it flickers, its
    /// bulb turns gold, its shade rose, and the light goes. The pole and
    /// the base stay in front of the light.
    fn draw_lamp(&self, rows: &mut [Vec<Color>]) {
        let (left, top, wide, tall) = LAMP;
        let dim = !self.flicker.is_multiple_of(2);
        let palette = self.palette;
        let bulb = (BULB.0 * LETTER as f32, BULB.1 * LETTER as f32);
        let desk = LIT_DESK * LETTER..(LIT_DESK + 1) * LETTER;
        let colour = |letter: u8| self.colours[usize::from(letter)];
        let lines = rows.iter_mut().zip(ART).enumerate();
        for (y, (row, line)) in lines.skip(top * LETTER).take(tall * LETTER) {
            let pixels = row.iter_mut().zip(line.bytes()).enumerate();
            for (x, (lit, letter)) in pixels.skip(left * LETTER).take(wide * LETTER) {
                let (dx, dy) = (x as f32 + 0.5 - bulb.0, y as f32 + 0.5 - bulb.1);
                if dim {
                    match letter {
                        b'T' => *lit = palette.gold,
                        b'G' => *lit = palette.rose,
                        b'8' => *lit = colour(b'7'),
                        _ => {}
                    }
                } else if letter == b'w' && dy > 0.0 {
                    let near = (1.0 - dx.hypot(dy) / GLOW_REACH).max(0.0);
                    let step = (near * near * GLOW_STEPS as f32).round() as usize;
                    if step > 0 {
                        *lit = self.glow[step.min(GLOW_STEPS)];
                    }
                } else if desk.contains(&y) && letter == b'a' {
                    *lit = if POOL.contains(&x) {
                        palette.gold
                    } else {
                        palette.rose
                    };
                }
            }
        }
    }

    /// The tail through the points of its pose, a letter apart, two pixels
    /// thick and one at its tip.
    fn draw_tail(&self, rows: &mut [Vec<Color>]) {
        let pose = self.flick.map_or(0, |tick| FLICK[tick / FLICK_TICKS]);
        let scale = LETTER as f32;
        let points: Vec<(f32, f32)> = TAILS[pose]
            .iter()
            .map(|&(x, y)| ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale))
            .collect();
        let tip = points.len() - 2;
        for (segment, pair) in points.windows(2).enumerate() {
            let brush = if segment == tip { 1.0 } else { scale };
            let [(x0, y0), (x1, y1)] = [pair[0], pair[1]];
            for step in 0..=TAIL_STEPS {
                let along = step as f32 / TAIL_STEPS as f32;
                let (x, y) = (x0 + (x1 - x0) * along, y0 + (y1 - y0) * along);
                let (left, top) = ((x - brush / 2.0).floor(), (y - brush / 2.0).floor());
                let (left, top, brush) = (left as usize, top as usize, brush as usize);
                for row in rows.iter_mut().skip(top).take(brush) {
                    row[left..left + brush].fill(self.palette.text);
                }
            }
        }
    }

    fn type_on(&mut self) {
        let width = self.screen[self.screen.len() - 1].width();
        if self.typed < width + LINE_PAUSE {
            self.typed += 1;
        } else {
            self.screen.remove(0);
            let code = self.code();
            self.screen.push(code);
            self.typed = 0;
        }
    }
}

impl Scene for Study {
    fn tick(&mut self, _area: Rect) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(TYPING) {
            self.type_on();
        }
        let bottom = (RAIN.1 + RAIN.3) as f64;
        for index in 0..self.rain.len() {
            let streak = &mut self.rain[index];
            streak.y += streak.speed;
            if streak.y - streak.length as f64 >= bottom {
                self.rain[index] = self.streak(true);
            }
        }
        for wisp in &mut self.steam {
            wisp.0 += 1.0;
        }
        // A wisp goes once the last letter of steam that follows it has
        // risen out of the steam's area.
        let gone = SPOUT.1 - STEAM.1 as f64 + 1.0 + 16.0 * RISE;
        self.steam.retain(|(age, _)| age * RISE < gone);
        if self.next_wisp == 0 {
            let sway = self.rng.random_range(0.0..std::f64::consts::TAU);
            self.steam.push((0.0, sway));
            self.next_wisp = self.rng.random_range(30..60);
        } else {
            self.next_wisp -= 1;
        }
        if self.flicker > 0 {
            self.flicker -= 1;
        } else if self.next_flicker == 0 {
            self.flicker = 7;
            self.next_flicker = self.rng.random_range(300..900);
        } else {
            self.next_flicker -= 1;
        }
        self.flick = match self.flick {
            Some(tick) if tick + 1 < FLICK.len() * FLICK_TICKS => Some(tick + 1),
            Some(_) => {
                self.next_flick = self.rng.random_range(80..250);
                None
            }
            None if self.next_flick == 0 => Some(0),
            None => {
                self.next_flick -= 1;
                None
            }
        };
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let pixels = self.pixels();
        let (from_x, from_y, wide, tall) = if area.width >= LEAST.0 && area.height >= LEAST.1 {
            (0, 0, PICTURE.0, PICTURE.1)
        } else {
            WINDOW
        };
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        // The view is centred, so a pane too small for it cuts it evenly.
        // It starts on a whole letter, so each letter fills a half of a
        // cell and no cell holds parts of three. Past the view the room
        // around it shows, and a pixel past the picture's edge takes the
        // edge's colour, so the wall, the sill and the desk run on to the
        // pane's edges.
        let room = (canvas.width() / LETTER, canvas.height() / LETTER);
        let left = from_x as isize - (room.0 as isize - wide as isize) / 2;
        let top = from_y as isize - (room.1 as isize - tall as isize) / 2;
        let (left, top) = (left * LETTER as isize, top * LETTER as isize);
        // A letter is a cell across but half a cell down, and a cell draws
        // two colours of its pixels. The view starts on a whole cell, so a
        // cell holds the same pixels of the picture, and draws them the
        // same, at every size of the pane.
        let cell = CELL_HEIGHT as isize;
        let top = top.div_euclid(cell) * cell;
        let (right, bottom) = (pixels[0].len() as isize - 1, pixels.len() as isize - 1);
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                let from_x = (x as isize + left).clamp(0, right) as usize;
                let from_y = (y as isize + top).clamp(0, bottom) as usize;
                canvas.set(x, y, pixels[from_y][from_x]);
            }
        }
        canvas.render(area, buf);
    }
}
