# ♨️ hearth

A terminal screensaver. One app, several scenes, a picker to choose from, and one key to walk through them.

![hearth demo](doc/demo.gif)

## Scenes

| Scene | What it draws |
|---|---|
| `noise` | Random glyphs over the whole terminal, a placeholder that proves the loop |
| `matrix` | Columns of half-width katakana and capitals fall at their own pace, a bright head and a tail that fades from foam to pine |
| `fire` | The doom fire, in half blocks: heat rises from a white-hot base through gold, rose, love and a grey smoke and dies into the background, in Rose Pine colours only, dithered where one meets the next |
| `life` | Conway's game of life on a torus, newborn cells in rose and settled ones in iris, reseeded when the board goes still |
| `pipes` | A few pipes in box-drawing runes walk and turn, wrap at the edges, and the screen clears once most of it is drawn |
| `bonsai` | The cbonsai tree grows step by step from a pot, gold wood and leaves in pine and foam, then holds and a new one grows |
| `aquarium` | Fish in three sizes swim at their own pace and depth, let out bubbles that rise to the waterline, over swaying seaweed |
| `typing` | Types one function at a time from a random file of a git repository, the way a person would, with a cursor and a pause at each line, at a speed you set, coloured by the grammar of each file's language, in a centred frame with line numbers, under a breadcrumb of the file's path and function and over a status line of the language, the cursor's line and column, and the speed, between a file tree open to the current file and a minimap of the whole file |

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
hearth noise        # start one scene
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

[speed]
typing = 50     # keystrokes per second, from 1 to 1000
fire = 1.5      # times the normal pace: 0.25, 0.5, 0.75, 1, 1.5, 2, 3 or 4
```

A scene or `--repo` on the command line comes before the file. `+` and `-` save the new speed to the file at once and keep its comments. A save that fails shows in the notice, and the speed still holds for the run. A key hearth does not know, or a value outside its range, stops startup with a message that names the file and the key.

### Picker

The picker lists each scene with a one-line summary and the speed it starts at, under the `{hearth}` mark. The marked scene runs in a pane beside the list, at its own speed, from its first frame. A narrow terminal drops the pane and shows the mark as the braced word. A short terminal drops the mark, and then the summaries, so every scene stays in the list.

| Key | Action |
|---|---|
| `j` / `k` | Move down and up the list, also `Down` / `Up` |
| `Enter` | Start the marked scene |
| `Esc` | Close the pane, when a split just opened it |
| `q` / `Ctrl-c` | Quit |

### Scene

| Key | Action |
|---|---|
| `Tab` | Next scene |
| `r` | Restart the scene with a new seed |
| `+` / `-` | Run the scene on screen faster or slower |
| `Esc` | Back to the picker |
| `v` / `s` | Split the pane side by side or stacked |
| `x` | Close the pane |
| `h` `j` `k` `l` | Move to the pane on that side, also the arrows |
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
