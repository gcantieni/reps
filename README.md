# reps

A terminal-based, local-only spaced repetition tool. Cards live in a plain text
file you can read, edit, grep, and check into git.

## Status

`0.1.0` — minimal but real. Add, edit, and remove cards; review what's due.
Scheduling uses [FSRS](https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-Algorithm)
(Free Spaced Repetition Scheduler, the algorithm Anki uses by default) via the
[`rs-fsrs`](https://crates.io/crates/rs-fsrs) crate, with default parameters
and 90% target retention. Intervals are whole days, and a card becomes due at
the start of its due date in local time.

## Install

```
cargo install reps
```

## Usage

```
reps add         # prompts for front and back, repeatedly; blank front to finish
reps add "What is the capital of France?" "Paris"
reps edit 1 --back "Paris, France"
reps rm 1
reps list
reps review
reps due         # one-line reminder if anything is due, silent otherwise
reps due --count # just the number
reps path        # print the resolved card-file path
```

During review, press any key to flip the card, then rate your recall with a
single keypress: `1` again, `2` hard, `3` good, `4` easy (`n` and `y` are
shortcuts for again and good). `q`, Esc, or Ctrl-C quits; progress is saved
after every card. When input isn't a terminal, reps reads one line per answer
instead. On a color terminal, each rating is colored and shows the interval it
would schedule. Color is off when output isn't a terminal, `TERM=dumb`, or
`NO_COLOR` is set.

## Shell reminder

To get a reminder each time you open a shell, add this to `~/.bashrc`:

```bash
if command -v reps >/dev/null; then
    reps due
fi
```

By default cards are stored at the platform data dir
(`~/Library/Application Support/reps/cards.tsv` on macOS,
`$XDG_DATA_HOME/reps/cards.tsv` on Linux). Override with `--file <path>` or
`$REPS_FILE`.

## File format

Tab-separated, one card per line:

```
id<TAB>state<TAB>due<TAB>last_review<TAB>stability<TAB>difficulty<TAB>reps<TAB>lapses<TAB>front<TAB>back
```

`state` is one of `new`, `learning`, `review`, `relearning`. `due` and
`last_review` are RFC 3339 UTC timestamps. `stability`, `difficulty`, `reps`,
and `lapses` are FSRS memory-state fields. Lines starting with `#` are
comments. Card text may not contain tabs or newlines.

Rows in the older 5-field Leitner format (`id, box, due_day, front, back`) are
still read, as new cards.

## Roadmap

- Optimize FSRS parameters from review history.
- Tags, decks, search.
- Anki import/export.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
