# ♨️ hearth

A terminal screensaver. One app, several scenes, a picker to choose from, and one key to walk through them.

![hearth demo](doc/demo.gif)

## Scenes

| Scene | What it draws |
|---|---|
| `noise` | Random glyphs over the whole terminal, a placeholder that proves the loop |
| `matrix` | Columns of half-width katakana and capitals fall at their own pace, a bright head and a tail that fades from foam to pine |
| `fire` | The doom fire, in half blocks: heat rises from a white-hot base through gold and love and dies into the background |
| `life` | Conway's game of life on a torus, newborn cells in rose and settled ones in iris, reseeded when the board goes still |
| `pipes` | A few pipes in box-drawing runes walk and turn, wrap at the edges, and the screen clears once most of it is drawn |
| `bonsai` | The cbonsai tree grows step by step from a pot, gold wood and leaves in pine and foam, then holds and a new one grows |
| `aquarium` | Fish in three sizes swim at their own pace and depth, let out bubbles that rise to the waterline, over swaying seaweed |
| `typing` | Types one function at a time from a random file of a git repository, the way a person would, with a cursor and a pause at each line, at a speed you set, coloured by the grammar of each file's language, in a centred frame with line numbers, under a breadcrumb of the file's path and function and over a status line of the language, the cursor's line and column, and the speed, beside a file tree open to the current file |

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
hearth typing --speed 80   # type 80 keystrokes per second
```

`--seed <n>` fixes the random source, so two runs with the same seed draw the same frames and the same chain of restarts. Without it, the clock seeds each run.

`--speed <cps>` sets how fast the typing scene types, in keystrokes per second, from 1 to 1000. The default is 50. `+` and `-` change it while the scene runs, and the new value shows in the corner for a second.

The typing scene types hearth's own source unless `--repo` names another repository:

```sh
hearth typing --repo ~/code/some-project      # a checkout on disk
hearth typing --repo rust-lang/rust           # a GitHub repository
hearth typing --repo https://example.com/x.git
```

A GitHub repository or a clone address is cloned once, shallow, into `~/.cache/hearth` (or `$XDG_CACHE_HOME/hearth`), with one line on stderr before the terminal opens. Every later run reads the cache, so the scene starts without a network. Only tracked text files under 64 KiB are read, so an ignored or untracked file never shows. A repository with no function in any of them is refused at startup.

### Picker

| Key | Action |
|---|---|
| `j` / `k` | Move down and up the list, also `Down` / `Up` |
| `Enter` | Start the marked scene |
| `q` / `Ctrl-c` | Quit |

### Scene

| Key | Action |
|---|---|
| `Tab` | Next scene |
| `r` | Restart the scene with a new seed |
| `+` / `-` | Type faster or slower, in the typing scene |
| `Esc` | Back to the picker |
| `q` / `Ctrl-c` | Quit |

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
