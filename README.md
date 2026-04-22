# mash

`mash` is a small interactive shell written in Rust.

It supports running external commands, a handful of builtins, command history,
aliases, piping, and basic output/error redirection.

If you want to download this it is really not difficult. You just need cargo and a somewhat recent rust version and your
good to go. Just run `cargo build` and there is your mash shell.

Note: This README is mostly AI-generated, but has been reviewed by a human.

## Features

- Run external commands with arbitrary arguments
- Builtins:
  - `exit`
  - `echo`
  - `type`
  - `pwd`
  - `cd`
  - `history`
  - `alias`
  - `unalias`
- Pipes with `|`
- Redirection:
  - `>` / `1>` overwrite stdout
  - `>>` / `1>>` append stdout
  - `2>` overwrite stderr
  - `2>>` append stderr
- Interactive and persistent history in `.mash_history`
- Alias persistence in `.mash_aliases`
- Command and file completion from `rustyline`

## Rust Version

`mash` has been developed and tested with Rust 1.93. Other versions may work (probably do), but are not officially supported.

## Build and Run

Just use cargo to run it, nothing fancy.

```bash
cargo build
cargo run
```

## Quick Usage
A few examples of what this shell is capable of:

External commands:

```bash
ls -la
cat README.md
```

Builtins:

```bash
pwd
cd src
type echo
history 10
```

Pipes and redirection:

```bash
cat Cargo.toml | grep rustyline
echo hello > out.txt
```

## Builtins

- `exit`
  - Exits `mash`.
- `echo [args...]`
  - Prints arguments joined by spaces.
- `type <name>`
  - Shows whether `<name>` is an alias, a builtin, an executable on `PATH`, or not found.
- `pwd`
  - Prints the current working directory.
- `cd <path>`
  - Changes the current directory.
  - On Unix targets, `~` is expanded to `$HOME`.
- `history [count]`
  - Shows command history, optionally limiting to the last `count` entries.
- `alias <name> <replacement>`
  - Creates or updates an alias.
  - Note: this is space-delimited syntax (`alias ll ls`), not `alias ll='ls'`.
- `unalias <name>`
  - Removes an alias.
