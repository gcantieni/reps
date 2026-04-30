use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

const SECS_PER_DAY: u64 = 86_400;
const BOX_INTERVALS_DAYS: [u32; 5] = [1, 2, 4, 7, 14];

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
    /// Add a card.
    Add { front: String, back: String },
    /// List all cards.
    List,
    /// Review cards that are due.
    Review,
    /// Print the resolved card-file path.
    Path,
}

#[derive(Clone)]
struct Card {
    id: u64,
    boxn: u8,
    due_day: i64,
    front: String,
    back: String,
}

fn main() {
    let cli = Cli::parse();
    let path = resolve_path(cli.file);
    let result = match cli.cmd {
        Cmd::Add { front, back } => cmd_add(&path, &front, &back),
        Cmd::List => cmd_list(&path),
        Cmd::Review => cmd_review(&path),
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

fn cmd_add(path: &PathBuf, front: &str, back: &str) -> io::Result<()> {
    if front.contains('\n') || back.contains('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "card text may not contain newlines",
        ));
    }
    let mut cards = load(path)?;
    let id = cards.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let card = Card {
        id,
        boxn: 1,
        due_day: today_day(),
        front: front.to_string(),
        back: back.to_string(),
    };
    cards.push(card);
    save(path, &cards)?;
    println!("added card {id}");
    Ok(())
}

fn cmd_list(path: &PathBuf) -> io::Result<()> {
    let cards = load(path)?;
    if cards.is_empty() {
        println!("no cards yet -- try: reps add \"front\" \"back\"");
        return Ok(());
    }
    let today = today_day();
    for c in &cards {
        let due = if c.due_day <= today {
            "due".to_string()
        } else {
            format!("in {}d", c.due_day - today)
        };
        println!("[{}] box {}  {}  {} -> {}", c.id, c.boxn, due, c.front, c.back);
    }
    Ok(())
}

fn cmd_review(path: &PathBuf) -> io::Result<()> {
    let mut cards = load(path)?;
    let today = today_day();
    let due_idxs: Vec<usize> = cards
        .iter()
        .enumerate()
        .filter(|(_, c)| c.due_day <= today)
        .map(|(i, _)| i)
        .collect();

    if due_idxs.is_empty() {
        println!("nothing due. nice.");
        return Ok(());
    }

    println!("{} card(s) due. y = correct, n = wrong, q = quit.\n", due_idxs.len());
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    for i in due_idxs {
        let card = cards[i].clone();
        print!("{}\n[enter to flip] ", card.front);
        io::stdout().flush()?;
        let _ = lines.next();
        print!("-> {}\n[y/n/q] ", card.back);
        io::stdout().flush()?;
        let ans = match lines.next() {
            Some(Ok(s)) => s,
            _ => break,
        };
        let ans = ans.trim();
        let correct = match ans {
            "y" | "Y" => true,
            "n" | "N" => false,
            "q" | "Q" => break,
            _ => {
                println!("(skipped)\n");
                continue;
            }
        };
        let (newbox, newdue) = schedule(card.boxn, correct, today);
        cards[i].boxn = newbox;
        cards[i].due_day = newdue;
        save(path, &cards)?;
        println!();
    }
    println!("done.");
    Ok(())
}

fn schedule(boxn: u8, correct: bool, today: i64) -> (u8, i64) {
    let new_box = if correct {
        (boxn + 1).min(5)
    } else {
        1
    };
    let interval = BOX_INTERVALS_DAYS[(new_box as usize) - 1] as i64;
    (new_box, today + interval)
}

fn today_day() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (secs / SECS_PER_DAY) as i64
}

fn load(path: &PathBuf) -> io::Result<Vec<Card>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(5, '\t').collect();
        if parts.len() != 5 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("bad row at line {}: expected 5 tab-separated fields", lineno + 1),
            ));
        }
        let id: u64 = parts[0].parse().map_err(invalid)?;
        let boxn: u8 = parts[1].parse().map_err(invalid)?;
        let due_day: i64 = parts[2].parse().map_err(invalid)?;
        out.push(Card {
            id,
            boxn,
            due_day,
            front: parts[3].to_string(),
            back: parts[4].to_string(),
        });
    }
    Ok(out)
}

fn save(path: &PathBuf, cards: &[Card]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut buf = String::new();
    buf.push_str("# reps cards. fields: id\\tbox\\tdue_day\\tfront\\tback (due_day = unix days)\n");
    for c in cards {
        buf.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            c.id, c.boxn, c.due_day, c.front, c.back
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
