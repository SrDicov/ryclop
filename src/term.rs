//! Minimal terminal UI: colours only on a TTY, `s/N` and numeric prompts.

use std::io::{self, IsTerminal, Write};

use crate::cli::Auto;

const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

fn colours() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn paint(code: &str, text: &str) -> String {
    if colours() {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

pub fn info(text: &str) {
    println!("{text}");
}

pub fn ok(text: &str) {
    println!("{}", paint(GREEN, text));
}

pub fn warn(text: &str) {
    println!("{}", paint(YELLOW, text));
}

pub fn error(text: &str) {
    eprintln!("{}", paint(RED, text));
}

pub fn bold(text: &str) -> String {
    paint(BOLD, text)
}

/// Ask a yes/no question. Accepts English and Spanish affirmatives.
pub fn confirm(prompt: &str, auto: Auto) -> bool {
    if auto == Auto::Yes {
        return true;
    }
    let Some(answer) = read_line(prompt) else {
        return false;
    };
    matches!(
        answer.to_lowercase().as_str(),
        "y" | "yes" | "s" | "si" | "sí"
    )
}

/// Read a selection of 1-based numbers, deduplicated and sorted.
/// Returns an empty vector for `n`, EOF, or nothing valid.
pub fn select(prompt: &str, count: usize) -> Vec<usize> {
    let Some(answer) = read_line(prompt) else {
        return Vec::new();
    };
    if answer.eq_ignore_ascii_case("n") || answer.eq_ignore_ascii_case("no") {
        return Vec::new();
    }
    let mut picked = std::collections::BTreeSet::new();
    for token in answer.split([',', ' ', '\t']) {
        if let Ok(number) = token.parse::<usize>() {
            if (1..=count).contains(&number) {
                picked.insert(number - 1);
            }
        }
    }
    picked.into_iter().collect()
}

fn read_line(prompt: &str) -> Option<String> {
    print!("{prompt}");
    io::stdout().flush().ok()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line).ok()?;
    Some(line.trim().to_owned())
}
