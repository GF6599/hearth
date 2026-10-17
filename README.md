# ♨️ hearth

A terminal screensaver. One app, several scenes, a picker to choose from, and one key to walk through them.

![hearth demo](doc/demo.gif)

## Scenes

| Scene | After | What it draws |
|---|---|---|
| `embers` | | Logs on a grate over hearth stones glow at their ends and cracks and throw sparks that cool from gold to rose to grey as they rise. Each log burns down over a few minutes into coals, a new log drops onto the pile, and now and then a log settles with a burst |
| `ink` | | Drops of ink in love, iris, foam, pine and rose fall into still water, seen from the side, in quadrant blocks. Each drop sinks and opens out, slow currents draw it into threads and curls that slide along the walls, and it thins by dither and fades over 20 to 40 seconds. A new drop falls every 5 to 12 seconds, never in the colour of the one before, and where two clouds meet they mix |
| `tide` | | A beach seen from above, in quadrant blocks, with the sea along the bottom. Waves rush up the sand at a slant, stop at an uneven line of foam and drain away. The sand they leave is wet and dark and dries back over half a minute. Shells and pebbles lie where the sea reaches and hide when it returns, and over about four minutes the tide rises and falls |
| `constellations` | | A night sky of stars at three brightnesses that twinkle in place. Every 15 to 30 seconds four to six near stars join: a line of braille dots draws itself from star to star, holds for about eight seconds under an invented name such as the kettle or the heron, and fades. Now and then a shooting star falls at a slant with a short gold trail |
| `solar` | | The sun and the eight planets, Mercury to Neptune, seen from above at a slant in braille dots, with the orbits evened out so all eight fit. Each planet, a solid cluster of dots in its own colour, goes round its dotted orbit at the pace that Kepler's third law gives, from about eight seconds for Mercury to a minute and a half for Neptune, and leaves a short trail that thins toward its end. The Moon goes round Earth, Saturn wears a flat ring, and Mercury passes in front of the sun and behind it |
| `matrix` | [cmatrix](https://github.com/abishekvashok/cmatrix) by Chris Allegretta | Columns of half-width katakana and capitals fall at their own pace, a bright head and a tail that fades from foam to pine |
| `fire` | [the PSX Doom fire](https://fabiensanglard.net/doom_fire_psx/), via Fabien Sanglard | The doom fire, in half blocks: heat rises from a white-hot base through gold, rose, love and a grey smoke and dies into the background, in Rose Pine colours only, dithered where one meets the next |
| `life` | [John Conway's Game of Life](https://en.wikipedia.org/wiki/Conway%27s_Game_of_Life) | Conway's game of life on a torus, newborn cells in rose and settled ones in iris, reseeded when the board goes still |
| `pipes` | [pipes.sh](https://github.com/pipeseroni/pipes.sh) by Matthew Simpson | A few pipes in box-drawing runes walk and turn, wrap at the edges, and the screen clears once most of it is drawn |
| `bonsai` | [cbonsai](https://gitlab.com/jallbrit/cbonsai) by John Allbritten | The cbonsai tree grows step by step from a pot, gold wood and leaves in pine and foam, then holds and a new one grows |
| `aquarium` | [asciiquarium](https://robobunny.com/projects/asciiquarium/html/) by Kirk Baucom | Fish in three sizes swim at their own pace and depth, let out bubbles that rise to the waterline, over swaying seaweed |
| `murmuration` | [the boids](https://www.red3d.com/cwr/boids/) of Craig Reynolds | A flock of starlings in braille dots folds, stretches, splits and joins in a dusk sky that lightens over a line of trees. The dense core of the flock is bright and its edges fade. Every 30 to 60 seconds a hawk crosses once, and the flock opens around it |
| `hourglass` | [the Pomodoro Technique](https://en.wikipedia.org/wiki/Pomodoro_Technique) of Francesco Cirillo | An hourglass of two round bulbs in a rose frame keeps a Pomodoro cycle on the real clock: by default a work round of 25 minutes in gold sand, a break of 5 in foam, and a long break of 15 after the fourth round. The sand falls grain by grain through the neck and piles at its natural slope, and the top empties as the phase ends. Then the glass turns over and the next phase starts. Four marks beside the glass count the rounds. The cycle starts when the scene does |
| `typing` | [gittype](https://github.com/unhappychoice/gittype) by Yuji Ueki | Types one function at a time from a random file of a git repository, the way a person would, with a cursor and a pause at each line, at a speed you set, coloured by the grammar of each file's language, in a centred frame with line numbers, under a breadcrumb of the file's path and function and over a status line of the language, the cursor's line and column, and the speed, between a file tree open to the current file and a minimap of the whole file |
| `noise` | | Random glyphs over the whole terminal, a placeholder that proves the loop |

`hearth --help` lists every scene the build carries.

## Install

With a Rust toolchain, 1.85 or later, because the crate uses the 2024 edition:

```sh
cargo install --path .
```

Or build and copy the binary to `~/self-made-bin/`:

```sh
just ship
```

A short alias saves typing when the app runs as a screensaver:

```sh
alias hth=hearth
```

## Usage

```sh
hearth              # open the picker
hearth tide         # start one scene
hearth --seed 7     # replay the same frames
```

`--seed <n>` fixes the random source, so two runs with the same seed draw the same frames and the same chain of restarts. Without it, the clock seeds each run.

Every scene keeps its own speed. `+` and `-` change the speed of the scene on screen, and the new value shows in the corner for a second. Typing steps in keystrokes per second, from 1 to 1000, and starts at 50. The other scenes step through ¼×, ½×, ¾×, 1×, 1.5×, 2×, 3× and 4×. A restart, Tab and the picker keep each scene's speed, and the config file keeps it between runs.

The typing scene types hearth's own source unless `--repo` names another repository:

```sh
hearth typing --repo ~/code/some-project      # a checkout on disk
hearth typing --repo rust-lang/rust           # a GitHub repository
hearth typing --repo https://example.com/x.git
```

A GitHub repository or a clone address is cloned once, shallow, into `~/.cache/hearth` (or `$XDG_CACHE_HOME/hearth`), with one line on stderr before the terminal opens. Every later run reads the cache, so the scene starts without a network. Only tracked text files under 64 KiB are read, so an ignored or untracked file never shows. A repository with no function in any of them is refused at startup.

### Config file

hearth reads `~/.config/hearth/config.toml` (or `$XDG_CONFIG_HOME/hearth/config.toml`) at startup. Every key is optional, and a missing file gives the defaults:

```toml
scene = "fire"            # the scene a bare `hearth` starts. Absent: the picker
repo = "rust-lang/rust"   # what the typing scene types. Absent: hearth's own source
theme = "rose-pine-dawn"  # rose-pine, rose-pine-moon, rose-pine-dawn, catppuccin-mocha, catppuccin-latte, tokyo-night, tokyo-night-day, kanagawa-wave, kanagawa-lotus, dracula, dracula-alucard, solarized-dark or solarized-light. Absent: rose-pine
background = "terminal"   # theme or terminal. Absent: theme

[speed]
typing = 50     # keystrokes per second, from 1 to 1000
fire = 1.5      # times the normal pace: 0.25, 0.5, 0.75, 1, 1.5, 2, 3 or 4

[hourglass]
work = 50       # minutes, from 1 to 180. Also break and long_break. Absent: 25, 5 and 15
```

The theme colours every scene and the picker. `t` moves to the next theme, starts each scene again in its colours, and saves the theme to the file the way `+` and `-` save a speed. Dawn, Latte, Tokyo Night Day, Kanagawa Lotus, Alucard and Solarized Light are light themes, so their fire grows from the gold role and not from white. `background = "terminal"` leaves out the theme's base colour, its panels and the murmuration dusk, so the terminal's own background shows behind every scene and the picker, and a transparent or translucent terminal stays so. The other colours stay as the theme draws them, the grey of the fire's smoke included. A scene or `--repo` on the command line comes before the file. `+` and `-` save the new speed to the file at once and keep its comments. A save that fails shows in the notice, and the speed still holds for the run. The hourglass keeps real time, so `[speed]` has no key for it, and on the hourglass `+` and `-` show the phase and the time left and save nothing. A key hearth does not know, or a value outside its range, stops startup with a message that names the file and the key.

### Picker

The picker lists each scene with a one-line summary and the speed it starts at, under the `{hearth}` mark, whose letters glow in drifting patches like embers between gold braces. The marked scene runs in a pane beside the list, at its own speed, from its first frame. The list and the pane are one height. The pane's bottom rule names the work and the person whose idea the scene reuses, whole or not at all. A narrow terminal drops the pane and shows the mark as the braced word. When the scenes do not all fit, the list scrolls with the mark and a thumb on its right rule shows where it is. A short terminal drops the mark when fewer than five scenes would stay in view, and then the summaries.

| Key | Action |
|---|---|
| `j` / `k` | Move down and up the list, also `Down` / `Up` |
| `Enter` | Start the marked scene |
| `Esc` | Close the pane, when a split just opened it |
| `t` | Next colour theme |
| `q` / `Ctrl-c` | Quit |

### Scene

| Key | Action |
|---|---|
| `Tab` | Next scene |
| `r` | Restart the scene with a new seed |
| `+` / `-` | Run the scene on screen faster or slower. On the hourglass, show the phase and the time left |
| `Esc` | Back to the picker |
| `v` / `s` | Split the pane side by side or stacked |
| `x` | Close the pane |
| `h` `j` `k` `l` | Move to the pane on that side, also the arrows |
| `t` | Next colour theme |
| `q` / `Ctrl-c` | Quit |

### Panes

The screen splits the way i3 does. `v` or `s` cuts the focused pane in two, and the new half opens the picker, with the scene after the focused one marked. `Esc` in that picker takes the split back. A thin rule separates the panes, and gold marks at two corners show the focused one. `Tab`, `r`, `+`, `-` and `Esc` act on the focused pane only, and every other pane runs on. `x` gives a pane's area to the other half of its split. The last pane stays. Each split stays at half and half when the terminal changes size.

## Development

```sh
just          # list every recipe
just tools    # install cargo-deny once, at the version the gate uses
just check    # the quality gate: clippy, the tests, cargo deny, then a format check
just demo     # record doc/demo.gif with vhs
```

Every scene is a module under `src/scenes/` that implements the `Scene` trait: `tick` advances one frame and `render` draws it. A scene takes its random source from the seed, so a test can assert one exact frame.

## License

MIT.
