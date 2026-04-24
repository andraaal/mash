mod args;
mod builtin;
mod cmd;
mod completion;
mod parser;

use crate::args::Args;
use crate::cmd::{Cmd, StreamTarget};
use crate::completion::ShellHelper;
use crate::parser::{Expr, Parser};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Error, ErrorKind, Write};
use std::process::exit;

pub(crate) type RLEditor = Editor<ShellHelper, DefaultHistory>;

pub(crate) struct ShellState {
    rl: RLEditor,
    aliases: HashMap<String, String>,
}

/// File used to persist command history between sessions.
const HISTORY_FILE: &str = ".mash_history";
/// File used to persist aliases between sessions.
const ALIAS_FILE: &str = ".mash_aliases";
/// Maximum depth allowed when resolving aliases recursively.
const MAX_ALIAS_DEPTH: usize = 10;

fn load_aliases(path: &str) -> HashMap<String, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::NotFound => return HashMap::new(),
        Err(err) => {
            eprintln!("failed to load aliases from {}: {}", path, err);
            return HashMap::new();
        }
    };

    let mut aliases = HashMap::new();
    for line in BufReader::new(file).lines() {
        match line {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }
                if let Some((alias, replacement)) = line.split_once('=')
                    && !alias.is_empty()
                {
                    aliases.insert(alias.to_owned(), replacement.to_owned());
                }
            }
            Err(err) => eprintln!("failed to read alias line: {}", err),
        }
    }

    aliases
}

fn save_aliases(path: &str, aliases: &HashMap<String, String>) -> Result<(), Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;

    let mut entries: Vec<(&String, &String)> = aliases.iter().collect();
    entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    for (alias, replacement) in entries {
        writeln!(file, "{}={}", alias, replacement)?;
    }

    Ok(())
}

fn main() {
    println!(
        "$$\\      $$\\  $$$$$$\\   $$$$$$\\  $$\\   $$\\
$$$\\    $$$ |$$  __$$\\ $$  __$$\\ $$ |  $$ |
$$$$\\  $$$$ |$$ /  $$ |$$ /  \\__|$$ |  $$ |
$$\\$$\\$$ $$ |$$$$$$$$ |\\$$$$$$\\  $$$$$$$$ |
$$ \\$$$  $$ |$$  __$$ | \\____$$\\ $$  __$$ |
$$ |\\$  /$$ |$$ |  $$ |$$\\   $$ |$$ |  $$ |
$$ | \\_/ $$ |$$ |  $$ |\\$$$$$$  |$$ |  $$ |
\\__|     \\__|\\__|  \\__| \\______/ \\__|  \\__|


"
    );
    if let Err(e) = io::stdout().flush() {
        eprintln!("failed to flush stdout: {}", e);
    }

    let mut rl: RLEditor = match Editor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("failed to create editor: {}", e);
            exit(1);
        }
    };

    let aliases = load_aliases(ALIAS_FILE);
    let mut helper = ShellHelper::new();
    let cmds = helper.get_commands_mut();
    for alias in &aliases {
        cmds.insert(alias.0.to_owned());
    }

    rl.set_helper(Some(helper));
    let _ = rl.load_history(HISTORY_FILE);
    let mut state = ShellState { rl, aliases };

    loop {
        match state.rl.readline("$ ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = state.rl.add_history_entry(trimmed);

                let words = Args::new(trimmed);
                let peek_args = words.peekable();
                let parser = Parser::new(peek_args, &state);
                match parser.compile() {
                    Ok(exprs) => {
                        for expr in exprs {
                            match execute(expr, &mut state) {
                                Ok(mut cmd) => {
                                    if let Err(e) = cmd.wait(&mut state) {
                                        eprintln!("{}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{}", e);
                                }
                            }
                        }
                    }
                    Err(errs) => {
                        for err in errs {
                            eprintln!("{}", err);
                        }
                    }
                }
            }

            Err(ReadlineError::Interrupted) => {
                continue;
            }
            Err(ReadlineError::Eof) => {
                exit_shell(&mut state);
            }
            Err(e) => {
                eprintln!("readline error: {}", e);
                exit_shell(&mut state);
            }
        }
    }
}

fn execute(stmt: Expr, rl: &mut ShellState) -> Result<Cmd, std::io::Error> {
    match stmt {
        Expr::Cmd(cmd) => Ok(cmd),
        Expr::OverwriteOutToFile(cmd, target_file) => {
            let mut command = execute(*cmd, rl)?;
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&target_file)?;
            command.set_stdout(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::AppendOutToFile(cmd, target_file) => {
            let mut command = execute(*cmd, rl)?;
            let file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&target_file)?;
            command.set_stdout(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::OverwriteErrToFile(cmd, target_file) => {
            let mut command = execute(*cmd, rl)?;
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&target_file)?;
            command.set_stderr(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::AppendErrToFile(cmd, target_file) => {
            let mut command = execute(*cmd, rl)?;
            let file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&target_file)?;
            command.set_stderr(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::Error => {
            panic!("Compiler's fault: Should not execute if there are any error tokens.")
        }
        Expr::Pipe(lhs, rhs) => {
            let mut left_cmd = execute(*lhs, rl)?;
            let mut right_cmd = execute(*rhs, rl)?;
            left_cmd.set_stdout(StreamTarget::Child(&mut right_cmd))?;
            if let Err(err) = left_cmd.wait(rl) {
                println!("{}", err);
            }
            Ok(right_cmd)
        }
    }
}

fn exit_shell(state: &mut ShellState) -> ! {
    if let Err(err) = save_aliases(ALIAS_FILE, &state.aliases) {
        eprintln!("failed to save aliases: {}", err);
    }

    if let Err(e) = state.rl.save_history(HISTORY_FILE) {
        eprintln!("failed to save history: {}", e);
    }
    exit(0);
}
