# Gitarium

Grow a Git repository into a pixel-art pond. Directories become islands, files become plants, and bridges show the directory tree.

## Embed in your README

1. [Request a GIF](https://github.com/sheinid/gitarium/issues/new?template=render.yml) for a public GitHub repository. You only need to do this once.
2. When the request closes with `render-ready`, paste this into your README and replace `OWNER/REPOSITORY` with the lowercase repository path:

```md
![Repository growing with Gitarium](https://sheinid.github.io/gitarium/embed/OWNER/REPOSITORY.gif)
```

[Open the GIF gallery](https://sheinid.github.io/gitarium/). GitHub Actions renders the timelapses; GitHub Pages hosts them. Changed repositories refresh every six hours. No installation, workflow, or token is needed in your repository.

The first render takes a few minutes after Actions starts. Loading an image does not start rendering. A GitHub account is needed to submit the one-time request. GitHub's README image cache may delay visible updates.

Gitarium's own history:

![Gitarium growing from its own commits](https://sheinid.github.io/gitarium/embed/sheinid/gitarium.gif)

You can also print the Markdown and first-render request link with the CLI:

```sh
gitarium embed OWNER/REPOSITORY --format human
```

The public queue supports up to 24 repositories, with repository and render size limits. [Limits, refresh behavior, and your own Pages instance](docs/pages.md).

## Run locally

Install [Rust](https://rust-lang.org/tools/install/) 1.85 or newer, Git, and FFmpeg with libx264.

```sh
cargo install --git https://github.com/sheinid/gitarium.git --locked

gitarium doctor
gitarium demo -o garden.mp4
gitarium snapshot /path/to/repository -o garden.png
gitarium export /path/to/repository -o growth.mp4 --duration 30
gitarium export /path/to/repository -o growth.gif --fps 15
```

PNG snapshots need no FFmpeg. Sprites are embedded in the executable. The `./gitarium` launcher also works from a source checkout and builds the release binary if needed.

## Smoother growth and deletion

Transitions use playback time independently of commit spacing. Islands rise from the pond, bridges build outward from their parents, and plants sprout into place. Deleted plants remain visible while they shrink and fade. Retired bridges retract and their islands sink away.

The default transition lasts 1.2 seconds. Change it with `--transition`:

```sh
gitarium export . -o growth.mp4 --duration 45 --transition 1.8 --hold 3
```

`--hold` pauses on the final settled scene and is included in `--duration`. The exporter reserves time for the final transitions before that pause. Very short clips shorten the transition to fit.

## Shape the pond

```sh
# Follow a branch and select an inclusive UTC date range.
gitarium export . --revision main --since 2026-01-01 --until 2026-12-31 \
  --calendar --duration 45 -o year.mp4

# Full HD, a fixed island threshold, and deeper directories.
gitarium export . --width 1920 --height 1080 --fps 30 --depth 6 \
  --max-islands 32 --island-threshold 24 -o pond.mp4

# Exclude files with repository-relative globs.
gitarium snapshot . --exclude '**/*.min.js' --exclude 'assets/**' -o garden.png

# Inspect the planned render without writing any output.
gitarium export . -o growth.mp4 --dry-run
```

Use `--force` to replace an output. Gitarium writes a temporary file and publishes it only after successful rendering. It never checks out or modifies the source repository. MP4 and GIF output use even dimensions, with 16:9 recommended for the composition.

## Nested islands and bridges

A directory qualifies for an island when its filtered subtree contains **more than N files** at an observed commit. Its files then move from the nearest active ancestor to the new island. If the count falls to N or less, they return to that ancestor. Each file has one owner.

N is automatic by default. Gitarium takes the latest snapshot's average files per nonempty directory, multiplies it by six, and rounds up to a multiple of four, bounded to 12..64. Small repositories scale down to at most one twelfth of the file count, with a minimum of four. This is a display heuristic, not a global repository statistic. For example, an average of four files per directory gives **N = 24** for a sufficiently large repository.

Set `--island-threshold N` to choose your own boundary. `--dry-run` reports the threshold, directory average, island depths, parents, radii, and file counts.

Each file-tree level reduces island radius by 20%. Every child has a bridge to its nearest displayed parent. There are 16 bridge variants, including planks, rails, rope, bamboo, logs, and stone slabs. Routes avoid unrelated islands and their banks.

The default layout allows 32 islands through depth six. Larger subtrees take priority when more directories qualify, and selected islands retain their ancestors. Unselected directories stay with their nearest displayed ancestor. Smaller islands hold fewer visible plants. Counts still include all filtered files.

## Plants and assets

There are 32 file species, 16 shoreline assets, and 16 bridge variants. Run `gitarium species` for the full catalog.

| Files | Plants |
| --- | --- |
| Rust / TypeScript / JavaScript / Python | Fern / cornflower / sunflower / lavender |
| Go / C and C++ / Java and Kotlin / Swift | Bamboo / prickly pear / tulips / apple sapling |
| Ruby / PHP / Haskell / Lua / Elixir | Poppies / iris / heather / violets / wild rose |
| Tests / documentation | Daisies / birch saplings |
| TOML / JSON / YAML / other config | Sedum / maidenhair fern / rushes / river stones |
| Lockfiles / build recipes / shell | Mushrooms / oat grass / cattails |
| SQL / GraphQL / tabular data / R and Julia | Mossy stone / blueberries / pampas grass / nasturtium |
| Templates / media / other files | Broadleaf herbs / water lilies / clover |

Lockfiles take priority over config detection. Tests take priority over language detection. Blob size and age affect plant size. Stable path hashes determine positions and orientation.

## Commands

| Command | Result |
| --- | --- |
| `export REPOSITORY -o garden.mp4` | Animate committed history as MP4 or GIF |
| `snapshot REPOSITORY -o garden.png` | Render a still image; `--progress 0..1` selects a point in history |
| `embed OWNER/REPOSITORY` | Print a hosted image URL and Markdown |
| `inspect REPOSITORY` | Show history statistics and species counts |
| `demo -o garden.mp4` | Render the bundled example; PNG and GIF work too |
| `species` | List plants, shoreline assets, and bridge variants |
| `doctor` | Check Git and video encoder availability |
| `schema export` | Inspect typed arguments derived from CLI definitions |

Use `--help` on a command for all options. Piped output defaults to JSON; terminals get readable text. Override with `--format json` or `--format human`. Progress goes to stderr. Exit codes are 0 for success, 1 for runtime errors, and 2 for invalid arguments.

## History limits

Gitarium reads first-parent history. Merge commits include the complete merged tree. Dirty and untracked files do not appear. Bare repositories work.

By default it samples 240 evenly spaced observations, including both endpoints. `--max-commits` accepts up to 2000 observations. Sampling can miss short-lived files or directories. Layout planning uses maximum simultaneous subtree counts, so historical path churn does not inflate island eligibility.

Renames appear as a removal and a new planting. Submodule contents are skipped. Git paths must be UTF-8. Captions use a bundled font drawn at the output resolution. Long names truncate to fit; crowded maps omit labels that would overlap. Unsupported glyphs appear as `?`. Shallow clones show available history and carry a partial-history label. Calendar mode handles backwards commit clocks with a monotone timestamp envelope.

Tracked `node_modules`, `target`, `vendor`, `dist`, `build`, `.next`, `__pycache__`, and `.venv` directories are hidden unless `--include-generated` is set. `--max-files` optionally limits filtered files per observed snapshot.

## Develop

```sh
cargo test --release --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
GITARIUM_TEST_FFMPEG=/path/to/ffmpeg cargo test --release --locked
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Tests cover Git trees and merges, threshold boundaries, parent links, file ownership, bridge routing, stable rendering, real-time transitions, retained deletions, GIF and MP4 encoding, Pages caching, and input validation.

The GIF request queue and publishing workflow live in `.github/` and `scripts/pages.py`. [Run your own Pages instance](docs/pages.md#run-your-own-pages-instance) to host another gallery.

## License and artwork

MIT. Sprite sheets were generated with ImageGen; the prompts are included in `assets/`. The renderer trims them to small pixel grids and a shared palette. Atkinson Hyperlegible is bundled under the SIL Open Font License in `assets/fonts/OFL.txt`. FFmpeg is an external dependency with its own license.
