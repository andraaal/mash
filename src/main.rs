mod args;
mod builtin;
mod cmd;
mod parser;
mod completion;

use crate::args::Args;
use crate::cmd::{Cmd, StreamTarget};
use crate::parser::{Expr, Parser};
use rustyline::history::DefaultHistory;
use rustyline::Editor;
use std::fs::OpenOptions;
use std::io::{self, Write};
use rustyline::error::ReadlineError;
use crate::completion::ShellHelper;

pub(crate) type RLEditor = Editor<ShellHelper, DefaultHistory>;
const HISTORY_FILE: &str = ".mash_history";

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
    io::stdout().flush().unwrap();

    let mut rl: RLEditor = Editor::new().unwrap();
    rl.set_helper(Some(ShellHelper::new()));
    let _ = rl.load_history(HISTORY_FILE);

    loop {
        match rl.readline("$ ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);

                let words = Args::new(&trimmed);
                let peek_args = words.peekable();
                let parser = Parser::new(peek_args);
                match parser.compile() {
                    Ok(exprs) => {
                        for expr in exprs {
                            if let Err(err) = execute(expr, &mut rl).unwrap().wait(&mut rl) {
                                eprintln!("{}", err);
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
                let _ = rl.save_history(HISTORY_FILE);
                break;
            }
            Err(e) => {
                eprintln!("readline error: {}", e);
                let _ = rl.save_history(HISTORY_FILE);
                break;
            }
        }
    }
}

fn execute(stmt: Expr, rl: &mut RLEditor) -> Result<Cmd, std::io::Error> {
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
