use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use clap::{Parser, Subcommand};
use rs_fsrs::{FSRS, Parameters, Rating, State};

const HEADER: &str = "# reps cards. fields: id\\tstate\\tdue\\tlast_review\\tstability\\tdifficulty\\treps\\tlapses\\tfront\\tback\n";
const NUM_FIELDS: usize = 10;
const LEGACY_NUM_FIELDS: usize = 5;

#[derive(Parser)]
#[command(name = "reps", version, about = "Local terminal spaced repetition.")]
struct Cli {
    /// Path to the card file. Defaults to $REPS_FILE, then the platform data dir.
    #[arg(long, global = true)]
    file: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Add a card. Prompts for any side not given; with no arguments, keeps
    /// prompting for cards until a blank front.
    Add {
        front: Option<String>,
        back: Option<String>,
    },
    /// Change the text of a card. Its schedule is kept.
    Edit {
        id: u64,
        #[arg(long)]
        front: Option<String>,
        #[arg(long)]
        back: Option<String>,
    },
    /// Remove a card.
    Rm { id: u64 },
    /// List all cards.
    List,
    /// Review cards that are due.
    Review,
    /// Print a reminder if any cards are due today; print nothing otherwise.
    Due {
        /// Print only the number of due cards.
        #[arg(long)]
        count: bool,
    },
    /// Print the resolved card-file path.
    Path,
}

#[derive(Clone)]
struct Card {
    id: u64,
    sched: rs_fsrs::Card,
    front: String,
    back: String,
}

fn main() {
    let cli = Cli::parse();
    let path = resolve_path(cli.file);
    let result = match cli.cmd {
        Cmd::Add { front, back } => cmd_add(&path, front, back),
        Cmd::Edit { id, front, back } => cmd_edit(&path, id, front, back),
        Cmd::Rm { id } => cmd_rm(&path, id),
        Cmd::List => cmd_list(&path),
        Cmd::Review => cmd_review(&path),
        Cmd::Due { count } => cmd_due(&path, count),
        Cmd::Path => {
            println!("{}", path.display());
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("reps: {e}");
        std::process::exit(1);
    }
}

fn resolve_path(flag: Option<PathBuf>) -> PathBuf {
    if let Some(p) = flag {
        return p;
    }
    if let Ok(p) = std::env::var("REPS_FILE") {
        return PathBuf::from(p);
    }
    let base = dirs::data_dir().expect("could not determine platform data dir");
    base.join("reps").join("cards.tsv")
}

fn scheduler() -> FSRS {
    // Short-term learning steps are measured in minutes, which suits in-session
    // drilling; reps is checked at most a few times a day, so schedule in days.
    FSRS::new(Parameters {
        enable_short_term: false,
        ..Parameters::default()
    })
}

/// A card is due once its due date, in local time, is today or earlier.
fn is_due(card: &Card, now: DateTime<Utc>) -> bool {
    let due_day = card.sched.due.with_timezone(&Local).date_naive();
    let today = now.with_timezone(&Local).date_naive();
    due_day <= today
}

fn check_text(s: &str) -> io::Result<()> {
    if s.contains('\n') || s.contains('\t') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "card text may not contain newlines or tabs",
        ));
    }
    Ok(())
}

fn find(cards: &mut [Card], id: u64) -> io::Result<&mut Card> {
    cards
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no card with id {id}")))
}

fn add_card(path: &Path, cards: &mut Vec<Card>, front: String, back: String) -> io::Result<()> {
    check_text(&front)?;
    check_text(&back)?;
    let id = cards.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    cards.push(Card {
        id,
        sched: rs_fsrs::Card::new(),
        front,
        back,
    });
    save(path, cards)?;
    println!("added card {id}");
    Ok(())
}

/// Returns `None` on end of input.
fn prompt(label: &str) -> io::Result<Option<String>> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        println!();
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

/// Prompts until a non-blank answer; `None` on end of input.
fn prompt_nonblank(label: &str) -> io::Result<Option<String>> {
    loop {
        match prompt(label)? {
            Some(s) if s.is_empty() => continue,
            other => return Ok(other),
        }
    }
}

fn cmd_add(path: &Path, front: Option<String>, back: Option<String>) -> io::Result<()> {
    let mut cards = load(path)?;
    if let Some(front) = front {
        let back = match back {
            Some(b) => b,
            None => match prompt_nonblank("back")? {
                Some(b) => b,
                None => return Ok(()),
            },
        };
        return add_card(path, &mut cards, front, back);
    }

    println!("adding cards. leave front blank to finish.");
    loop {
        let front = match prompt("front")? {
            Some(f) if !f.is_empty() => f,
            _ => break,
        };
        let back = match prompt_nonblank("back")? {
            Some(b) => b,
            None => break,
        };
        if let Err(e) = add_card(path, &mut cards, front, back) {
            if e.kind() != io::ErrorKind::InvalidInput {
                return Err(e);
            }
            eprintln!("reps: {e}");
        }
        println!();
    }
    Ok(())
}

fn cmd_edit(path: &Path, id: u64, front: Option<String>, back: Option<String>) -> io::Result<()> {
    if front.is_none() && back.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nothing to change -- pass --front and/or --back",
        ));
    }
    let mut cards = load(path)?;
    let card = find(&mut cards, id)?;
    if let Some(f) = front {
        check_text(&f)?;
        card.front = f;
    }
    if let Some(b) = back {
        check_text(&b)?;
        card.back = b;
    }
    save(path, &cards)?;
    println!("updated card {id}");
    Ok(())
}

fn cmd_rm(path: &Path, id: u64) -> io::Result<()> {
    let mut cards = load(path)?;
    find(&mut cards, id)?;
    cards.retain(|c| c.id != id);
    save(path, &cards)?;
    println!("removed card {id}");
    Ok(())
}

fn cmd_list(path: &Path) -> io::Result<()> {
    let cards = load(path)?;
    if cards.is_empty() {
        println!("no cards yet -- try: reps add \"front\" \"back\"");
        return Ok(());
    }
    let now = Utc::now();
    let today = now.with_timezone(&Local).date_naive();
    for c in &cards {
        let due = if is_due(c, now) {
            "due".to_string()
        } else {
            let due_day = c.sched.due.with_timezone(&Local).date_naive();
            format!("in {}d", (due_day - today).num_days())
        };
        println!(
            "[{}] {:<10} {:<7} {} -> {}",
            c.id,
            state_name(c.sched.state),
            due,
            c.front,
            c.back
        );
    }
    Ok(())
}

fn cmd_due(path: &Path, count_only: bool) -> io::Result<()> {
    let cards = load(path)?;
    let now = Utc::now();
    let n = cards.iter().filter(|c| is_due(c, now)).count();
    if count_only {
        println!("{n}");
    } else if n > 0 {
        let noun = if n == 1 { "card" } else { "cards" };
        println!("reps: {n} {noun} due -- run `reps review`");
    }
    Ok(())
}

fn format_days(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    let days = (to - from).num_days();
    if days < 30 {
        format!("{days}d")
    } else if days < 365 {
        format!("{:.1}mo", days as f64 / 30.0)
    } else {
        format!("{:.1}y", days as f64 / 365.0)
    }
}

/// ANSI styling for interactive output, disabled when stdout is not a color terminal.
struct Style {
    color: bool,
}

impl Style {
    fn detect() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
        Style {
            color: io::stdout().is_terminal() && !no_color && !dumb,
        }
    }

    fn paint(&self, sgr: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{sgr}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
}

fn rating_label(r: Rating) -> (&'static str, &'static str) {
    match r {
        Rating::Again => ("again", "1;31"),
        Rating::Hard => ("hard", "1;33"),
        Rating::Good => ("good", "1;32"),
        Rating::Easy => ("easy", "1;34"),
    }
}

/// The rating prompt. With color, each choice also shows the interval it would schedule.
fn rating_prompt(style: &Style, preview: &rs_fsrs::RecordLog, now: DateTime<Utc>) -> String {
    let mut parts: Vec<String> = Rating::iter()
        .map(|&r| {
            let (name, sgr) = rating_label(r);
            let key = format!("{} {name}", r as u8);
            if style.color {
                let interval = format_days(now, preview[&r].card.due);
                format!("{} {}", style.paint(sgr, &key), style.dim(&interval))
            } else {
                key
            }
        })
        .collect();
    parts.push(style.dim("q quit"));
    if style.color {
        format!("{}  {} ", parts.join("   "), style.bold(">"))
    } else {
        format!("[{}] ", parts.join(", "))
    }
}

/// Reads review input one keypress at a time on a terminal, or one line at a time otherwise.
enum KeyInput {
    Tty,
    Lines(io::Lines<io::StdinLock<'static>>),
}

/// Restores cooked mode even if reading a key fails.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

impl KeyInput {
    fn detect() -> Self {
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            KeyInput::Tty
        } else {
            KeyInput::Lines(io::stdin().lock().lines())
        }
    }

    fn is_tty(&self) -> bool {
        matches!(self, KeyInput::Tty)
    }

    /// Returns `None` on end of input or an interrupt. Enter reads as `'\n'`, Esc as `'q'`.
    fn read_key(&mut self) -> io::Result<Option<char>> {
        match self {
            KeyInput::Lines(lines) => match lines.next() {
                Some(line) => Ok(Some(line?.trim().chars().next().unwrap_or('\n'))),
                None => Ok(None),
            },
            KeyInput::Tty => {
                use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
                crossterm::terminal::enable_raw_mode()?;
                let _guard = RawModeGuard;
                loop {
                    let Event::Key(key) = event::read()? else {
                        continue;
                    };
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if let KeyCode::Char('c' | 'd') = key.code {
                            return Ok(None);
                        }
                        continue;
                    }
                    match key.code {
                        KeyCode::Char(c) => return Ok(Some(c)),
                        KeyCode::Enter => return Ok(Some('\n')),
                        KeyCode::Esc => return Ok(Some('q')),
                        _ => continue,
                    }
                }
            }
        }
    }
}

fn parse_rating(key: char) -> Option<Rating> {
    match key {
        '1' | 'n' | 'N' => Some(Rating::Again),
        '2' => Some(Rating::Hard),
        '3' | 'y' | 'Y' => Some(Rating::Good),
        '4' => Some(Rating::Easy),
        _ => None,
    }
}

fn cmd_review(path: &Path) -> io::Result<()> {
    let mut cards = load(path)?;
    let now = Utc::now();
    let due_idxs: Vec<usize> = cards
        .iter()
        .enumerate()
        .filter(|(_, c)| is_due(c, now))
        .map(|(i, _)| i)
        .collect();

    if due_idxs.is_empty() {
        println!("nothing due. nice.");
        return Ok(());
    }

    let style = Style::detect();
    let mut input = KeyInput::detect();
    let flip_hint = if input.is_tty() {
        "[any key to flip] "
    } else {
        "[enter to flip] "
    };
    let total = due_idxs.len();
    println!("{total} card(s) due. y and n are shortcuts for good and again.\n");
    let fsrs = scheduler();

    'cards: for (n, i) in due_idxs.into_iter().enumerate() {
        let card = cards[i].clone();
        let (front_tag, back_tag) = if style.color { ("", "") } else { ("Q: ", "A: ") };
        println!("{}", style.dim(&format!("-- {}/{total} --", n + 1)));
        println!("{front_tag}{}", style.bold(&card.front));
        print!("{}", style.dim(flip_hint));
        io::stdout().flush()?;
        match input.read_key()? {
            None | Some('q' | 'Q') => break,
            Some(_) => {}
        }
        if input.is_tty() {
            println!();
        }

        let now = Utc::now();
        let preview = fsrs.repeat(card.sched.clone(), now);
        println!("{back_tag}{}", style.paint("1;36", &card.back));
        print!("{}", rating_prompt(&style, &preview, now));
        io::stdout().flush()?;

        let rating = loop {
            match input.read_key()? {
                None | Some('q' | 'Q') => break 'cards,
                Some(key) => {
                    if let Some(r) = parse_rating(key) {
                        break r;
                    }
                }
            }
        };
        if input.is_tty() {
            let (name, sgr) = rating_label(rating);
            println!("{}", style.paint(sgr, name));
        }
        cards[i].sched = preview[&rating].card.clone();
        save(path, &cards)?;
        println!();
    }
    println!("done.");
    Ok(())
}

fn state_name(s: State) -> &'static str {
    match s {
        State::New => "new",
        State::Learning => "learning",
        State::Review => "review",
        State::Relearning => "relearning",
    }
}

fn parse_state(s: &str) -> io::Result<State> {
    match s {
        "new" => Ok(State::New),
        "learning" => Ok(State::Learning),
        "review" => Ok(State::Review),
        "relearning" => Ok(State::Relearning),
        _ => Err(invalid(format!("unknown state {s:?}"))),
    }
}

fn parse_time(s: &str) -> io::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(invalid)
}

fn format_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn parse_row(line: &str) -> io::Result<Card> {
    let parts: Vec<&str> = line.split('\t').collect();
    // Rows from the older Leitner format (id, box, due_day, front, back) restart as new cards.
    if parts.len() == LEGACY_NUM_FIELDS {
        return Ok(Card {
            id: parts[0].parse().map_err(invalid)?,
            sched: rs_fsrs::Card::new(),
            front: parts[3].to_string(),
            back: parts[4].to_string(),
        });
    }
    if parts.len() != NUM_FIELDS {
        return Err(invalid(format!(
            "expected {NUM_FIELDS} tab-separated fields, got {}",
            parts.len()
        )));
    }
    let sched = rs_fsrs::Card {
        state: parse_state(parts[1])?,
        due: parse_time(parts[2])?,
        last_review: parse_time(parts[3])?,
        stability: parts[4].parse().map_err(invalid)?,
        difficulty: parts[5].parse().map_err(invalid)?,
        reps: parts[6].parse().map_err(invalid)?,
        lapses: parts[7].parse().map_err(invalid)?,
        ..Default::default()
    };
    Ok(Card {
        id: parts[0].parse().map_err(invalid)?,
        sched,
        front: parts[8].to_string(),
        back: parts[9].to_string(),
    })
}

fn load(path: &Path) -> io::Result<Vec<Card>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let card = parse_row(line)
            .map_err(|e| invalid(format!("{}:{}: {e}", path.display(), lineno + 1)))?;
        out.push(card);
    }
    Ok(out)
}

fn save(path: &Path, cards: &[Card]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut buf = String::from(HEADER);
    for c in cards {
        let s = &c.sched;
        buf.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            c.id,
            state_name(s.state),
            format_time(s.due),
            format_time(s.last_review),
            s.stability,
            s.difficulty,
            s.reps,
            s.lapses,
            c.front,
            c.back
        ));
    }
    let tmp = path.with_extension("tsv.tmp");
    fs::write(&tmp, buf)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn invalid<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}
