use std::collections::HashSet;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::line_buffer::LineBuffer;
use rustyline::validate::Validator;
use rustyline::{Changeset, Context, Helper};

pub(crate) struct ShellHelper {
    commands: Vec<String>,
    history_hinter: HistoryHinter,
    file_completer: FilenameCompleter,
}

impl ShellHelper {
    pub(crate) fn new() -> Self {
        let mut commands = vec![
            "exit".to_string(),
            "echo".to_string(),
            "type".to_string(),
            "pwd".to_string(),
            "cd".to_string(),
            "history".to_string(),
        ];

        let mut seen = HashSet::new();
        commands.retain(|cmd| seen.insert(cmd.clone()));

        ShellHelper {
            commands,
            history_hinter: HistoryHinter {},
            file_completer: FilenameCompleter::new(),
        }
    }
}

impl Helper for ShellHelper {}
impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}


impl Hinter for ShellHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<Self::Hint> {
        self.history_hinter.hint(line, pos, ctx)
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let safe_pos = pos.min(line.len());
        let before = &line[..safe_pos];

        let start = before
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);

        let token = &line[start..safe_pos];
        let first_word = before[..start].trim().is_empty();

        if first_word {
            let mut out = Vec::new();
            for cmd in &self.commands {
                if cmd.starts_with(token) {
                    out.push(Pair {
                        display: cmd.clone(),
                        replacement: cmd.clone(),
                    });
                }
            }
            Ok((start, out))
        } else {
            self.file_completer.complete(line, pos, ctx)
        }
    }

    fn update(&self, line: &mut LineBuffer, start: usize, elected: &str, cl: &mut Changeset) {
        self.file_completer.update(line, start, elected, cl)
    }
}
