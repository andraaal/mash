mod args;
mod builtin;
mod cmd;

use std::fs::OpenOptions;
use crate::args::Args;
use crate::cmd::{Cmd, Expr, Parser, StreamTarget};
use std::io::{self, Write};

fn main() {
    println!("$$\\      $$\\  $$$$$$\\   $$$$$$\\  $$\\   $$\\
$$$\\    $$$ |$$  __$$\\ $$  __$$\\ $$ |  $$ |
$$$$\\  $$$$ |$$ /  $$ |$$ /  \\__|$$ |  $$ |
$$\\$$\\$$ $$ |$$$$$$$$ |\\$$$$$$\\  $$$$$$$$ |
$$ \\$$$  $$ |$$  __$$ | \\____$$\\ $$  __$$ |
$$ |\\$  /$$ |$$ |  $$ |$$\\   $$ |$$ |  $$ |
$$ | \\_/ $$ |$$ |  $$ |\\$$$$$$  |$$ |  $$ |
\\__|     \\__|\\__|  \\__| \\______/ \\__|  \\__|


");
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        if input.trim().is_empty() {
            continue;
        }
        let words = Args::new(input.trim());
        let peek_args = words.peekable();
        let parser = Parser::new(peek_args);
        match parser.compile() {
            Ok(exprs) => {
                for expr in exprs {
                    if let Err(err) = execute(expr).unwrap().wait() {
                        println!("{}", err);
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
}

fn execute(stmt: Expr) -> Result<Cmd, std::io::Error> {
    match stmt {
        Expr::Cmd(cmd) => {
            Ok(cmd)
        }
        Expr::OverwriteOutToFile(cmd, target_file) => {
            let mut command = execute(*cmd)?;
            let file = OpenOptions::new().write(true).create(true).truncate(true).open(&target_file)?;
            command.set_stdout(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::AppendOutToFile(cmd, target_file) => {
            let mut command = execute(*cmd)?;
            let file = OpenOptions::new().append(true).create(true).open(&target_file)?;
            command.set_stdout(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::OverwriteErrToFile(cmd, target_file) => {
            let mut command = execute(*cmd)?;
            let file = OpenOptions::new().write(true).create(true).truncate(true).open(&target_file)?;
            command.set_stderr(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::AppendErrToFile(cmd, target_file) => {
            let mut command = execute(*cmd)?;
            let file = OpenOptions::new().append(true).create(true).open(&target_file)?;
            command.set_stderr(StreamTarget::File(file))?;
            Ok(command)
        }
        Expr::Error => {
            panic!("Compiler's fault: Should not execute if there are any error tokens.")
        }
        Expr::Pipe(lhs, rhs) => {
            let mut left_cmd = execute(*lhs)?;
            let mut right_cmd = execute(*rhs)?;
            left_cmd.set_stdout(StreamTarget::Child(&mut right_cmd))?;
            if let Err(err) = left_cmd.wait() {
                println!("{}", err);
            }
            Ok(right_cmd)
        }
    }
}


