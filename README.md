# doc-merge

This crate provides a primitive `doc-merge` command.

It does one and exactly one thing: it lets you combine the `cargo doc` output from multiple crates
into one location and adds an index. If you have multiple crates that you want to combine into a
single documentation site, this crate might be what you need.

While it's not a requirement, this crate is written with the expectation that you are usually
running `cargo doc --no-deps`, because you're trying to document your own crates, and not their
dependencies.

## Installation

```sh
$ cargo install doc-merge
```

## Usage

```sh
$ doc-merge --src /path/to/crate/target/doc/ --src /path/to/other/target/doc --dest /path/to/docs/
```
