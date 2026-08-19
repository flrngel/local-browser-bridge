# v0.8.0 live evidence

These screenshots contain only the repository's deterministic browser demo and native macOS fixture. Screenshots of the user's real Chrome extension list, tab strip, and popup are retained only in the ignored local `target/sota-e2e` directory to avoid publishing personal browser metadata.

## Browser screenshots

| File | Proven operation |
| --- | --- |
| `browser-01-observe.jpg` | Structured DOM plus screenshot |
| `browser-02-fill.jpg` | Fill text field |
| `browser-03-select.jpg` | Select option |
| `browser-04-click.jpg` | Trusted element click and submitted output |
| `browser-05-shadow-click.jpg` | Open-shadow-root discovery and trusted click |
| `browser-07-inactive-observe.jpg` | Inactive-tab CDP screenshot without activation |
| `browser-08-activate.jpg` | Explicit tab activation |
| `browser-09-click-at-fixed.jpg` | Snapshot-bound trusted coordinate click |
| `browser-10-type-text.jpg` | Snapshot-bound direct text input |
| `browser-11-key.jpg` | Snapshot-bound key input |
| `browser-12-scroll.jpg` | Viewport scroll to offscreen target |
| `browser-13-bottom-click.jpg` | Trusted offscreen-element click after scroll |
| `browser-14-evaluate.jpg` | JavaScript evaluation with visible proof |
| `browser-15-navigate-step2.jpg` | URL navigation |
| `browser-16-back.jpg` | History back |
| `browser-17-forward.jpg` | History forward |
| `browser-18-reload.jpg` | Reload preserving route state |

## Computer screenshots

| File | Proven operation |
| --- | --- |
| `computer-01-observe.jpg` | Exact background window observation |
| `computer-02-move.jpg` | Synthetic pointer move with real cursor unchanged |
| `computer-03-click.jpg` | Background click |
| `computer-04-drag.jpg` | Background drag |
| `computer-05-scroll.jpg` | Background scroll |
| `computer-06-type-text.jpg` | Background text input |
| `computer-07-key.jpg` | Background key input |
| `computer-09-semantic-observe.jpg` | AX semantic elements |
| `computer-10-semantic-set-value-verified.jpg` | AX value write and read-back |
| `computer-11-semantic-invoke-verified.jpg` | AX button invocation and visible state change |

Machine-readable, privacy-sanitized outcomes are in `results.json`.
