# reps

A terminal-based, local-only spaced repetition tool. Cards live in a plain text
file you can read, edit, grep, and check into git.

## Status

`0.1.0` — minimal but real. Add cards, list cards, review what's due. Uses
[Leitner boxes](https://en.wikipedia.org/wiki/Leitner_system) (5 boxes,
intervals 1/2/4/7/14 days).

## Install

```
cargo install reps
```

## Usage

```
reps add "What is the capital of France?" "Paris"
reps list
reps review
reps path        # print the resolved card-file path
```

By default cards are stored at the platform data dir
(`~/Library/Application Support/reps/cards.tsv` on macOS,
`$XDG_DATA_HOME/reps/cards.tsv` on Linux). Override with `--file <path>` or
`$REPS_FILE`.

## File format

Tab-separated, one card per line:

```
id<TAB>box<TAB>due_day<TAB>front<TAB>back
```

`due_day` is a Unix day number (seconds-since-epoch / 86400). Lines starting
with `#` are comments. Newlines in card text are not allowed.

## Roadmap

- Optional shell-prompt integration (one-line "card peek" before each prompt).
- SM-2 algorithm as an alternative to Leitner.
- Tags, decks, search.
- Anki import/export.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
