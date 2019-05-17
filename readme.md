# Description

This crate provides both a library and a CLI tool.
- The library implements a parser for the output of `cargo check --message-format json`. It also provides an interface for directly invoking cargo and parsing.
- The CLI tool uses the library to provide compact error reporting for Rust projects

[On crates.io](crates.io/crates/coral)

[Documentation](docs.rs/coral)

# Demo

![Demo Gif](https://github.com/kaikalii/coral/blob/master/demo.gif "Demo GIF")

# Installation

Installation requires cargo, and is very simple:
```
cargo install coral
```
