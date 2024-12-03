//! This crate provides a primitive `doc-merge` command.
//!
//! It does one and exactly one thing: it lets you combine the `cargo doc` output from multiple
//! crates into one location and adds an index. If you have multiple crates that you want to
//! combine into a single documentation site, this crate might be what you need.
//!
//! While it's not a requirement, this crate is written with the expectation that you are usually
//! running `cargo doc --no-deps`, because you're trying to document your own crates, and not their
//! dependencies.
//!
//! ## Installation
//!
//! ```sh
//! $ cargo install doc-merge
//! ```
//!
//! ## Usage
//!
//! From a crate root:
//!
//! ```sh
//! $ doc-merge --dest /path/to/docs/
//! ```
//!
//! From somewhere else:
//!
//! ```sh
//! $ doc-merge --src /path/to/crate/target/doc/ --dest /path/to/docs/
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use fs_extra::dir::CopyOptions;
use regex::Regex;

/// Merge an individiual cargo doc site into a shared rustdoc site.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct DocMerge {
    /// The location of the documentation to merge in.
    ///
    /// The documentation is expected to already be built, usually with `cargo doc --no-deps` or
    /// similar. Workspaces are supported.
    #[arg(long, default_value = "./target/doc")]
    src: PathBuf,

    /// The root of the shared rustdoc site.
    #[arg(long)]
    dest: PathBuf,

    /// The name of the crate that will have it's index.html be copied and used.
    #[arg(long)]
    index_crate: String,

    /// Create the destination directory if it does not exist.
    #[arg(long)]
    create_dest: bool,
}

macro_rules! fatal {
  ($($arg:tt)*) => {{
    eprintln!("Fatal: {}", format!($($arg)*));
    std::process::exit(1);
  }}
}

impl DocMerge {
    fn execute(self) -> Result<()> {
        // Sanity check: Does the source directory exist?
        if !self.src.is_dir() {
            fatal!(
                "Source documentation not found at {}. Did you run `cargo doc`?",
                self.src.to_str().expect("Invalid path")
            );
        }

        // Sanity check: Does the destination directory exist?
        if !self.dest.is_dir() {
            match self.create_dest {
                true => fs::create_dir_all(&self.dest)?,
                false => fatal!(
          "Destination directory {} not found. If this is intentional, use `--create-dest`.",
          self.dest.to_str().expect("Invalid path")
        ),
            }
        }

        // Copy the each subdirectory in the source to the destination (but not the files).
        let opts = CopyOptions {
            overwrite: true,
            ..Default::default()
        };
        for entry in self.src.read_dir()? {
            let entry = entry?;
            if entry.path().is_dir() {
                fs_extra::copy_items(&[entry.path()], &self.dest, &opts)?;
            }
            if entry
                .file_name()
                .to_str()
                .expect("Invalid filename")
                .ends_with(".html")
            {
                fs::copy(entry.path(), &self.dest.join(entry.file_name()))?;
            }
        }

        // Add this crate's data to the search index and source file database.
        let key_regex = Regex::new(r#"^"([a-z0-9_]+)":"#)?;
        for js in ["search-index.js"] {
            // If the destination does not yet have this file, copy it over.
            if !self.dest.as_path().join(js).is_file() {
                fs::copy(self.src.as_path().join(js), &self.dest.join(js))?;
                continue;
            }

            // Read the source and destination files and ensure the presence of each of the source crates
            // in the destination.
            let mut src_js: BTreeMap<String, String> =
                fs::read_to_string(self.src.as_path().join(js))?
                    .split('\n')
                    .filter_map(|line| {
                        Some((
                            key_regex.captures(line)?[1].to_string(),
                            line.replace(r"}\", r"},\"),
                        ))
                    })
                    .collect();
            let mut contents = fs::read_to_string(self.dest.as_path().join(js))?
                .split('\n')
                .map(|line| {
                    key_regex
                        .captures(line)
                        .and_then(|c| src_js.remove(&c[1]))
                        .unwrap_or_else(|| line.to_string())
                })
                .collect::<Vec<String>>();
            src_js.into_values().for_each(|v| contents.insert(1, v));

            write!(
                fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(self.dest.as_path().join(js))?,
                "{}",
                contents.join("\n").replace("},\\\n}');", "}\\\n}');")
            )?;
        }

        // Okay, all the files except index.html and crates.js are in place.
        // Read the search index again to get the information we need to build those.
        let doc_regex = Regex::new(r#""doc":"([^"]+)"#)?;
        let crates: BTreeMap<String, Option<String>> =
            fs::read_to_string(self.dest.as_path().join("search-index.js"))?
                .split('\n')
                .filter_map(|line| {
                    // Get the crate name, and also try to get a crate description if there is one.
                    let crate_name = key_regex.captures(line)?[1].to_string();
                    let crate_desc = doc_regex.captures(line).map(|c| c[1].to_string());
                    Some((crate_name, crate_desc))
                })
                .collect();

        // Write the crates.js file.
        write!(
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.dest.as_path().join("crates.js"))?,
            "window.ALL_CRATES = [{}];",
            crates
                .keys()
                .map(|k| format!("\"{}\"", k))
                .collect::<Vec<String>>()
                .join(",")
                .as_str()
        )?;

        let index_path = self.dest.as_path().join("index.html");
        if fs::exists(&index_path)? {
            fs::remove_file(&index_path)?;
        }
        #[cfg(unix)]
        let symlink = std::os::unix::fs::symlink;
        #[cfg(windows)]
        let symlink = std::os::windows::fs::symlink_file;
        symlink(
            self.dest
                .as_path()
                .join(&self.index_crate)
                .join("index.html"),
            &index_path,
        )?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let doc_merge = DocMerge::parse();
    doc_merge.execute()
}
