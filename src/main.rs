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
//! ```sh
//! $ doc-merge --src /path/to/crate/target/doc/ --src /path/to/other/target/doc --dest /path/to/docs/
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use fs_extra::dir::CopyOptions;
use jzon::JsonValue;
use regex::Regex;

/// Merge an individiual cargo doc site into a shared rustdoc site.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct DocMerge {
    /// The locations of documentations to merge together.
    ///
    /// The documentation is expected to already be built, usually with `cargo doc --no-deps` or
    /// similar. Workspaces are supported.
    #[arg(long)]
    src: Vec<PathBuf>,

    /// The root of the shared rustdoc site.
    #[arg(long, default_value = "./docs")]
    dest: PathBuf,

    /// The name of the crate that will have it's index.html be symlinked to the target directory.
    /// If note passed, no index.html will be symlinked.
    #[arg(long)]
    index_crate: Option<String>,
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
        if self.src.len() < 2 {
            fatal!("At least two documentation paths must be passed for merging");
        }
        // create destination if it doesnt exist
        fs::create_dir_all(&self.dest)?;

        // Copy the each subdirectory in the source to the destination (but not the files).
        let opts = CopyOptions {
            overwrite: true,
            ..Default::default()
        };
        for src in &self.src {
            for entry in src.read_dir()? {
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
        }

        // parse all search-index.js files for crates
        let search_index_regex = Regex::new(r"JSON\.parse\('(.*)'\)")?;
        let mut crates = BTreeMap::<String, JsonValue>::new();
        for docs_path in &self.src {
            let content = fs::read_to_string(docs_path.join("search-index.js"))?;
            let search_index_raw = search_index_regex
                .captures(&content)
                .expect("search-index.js must have searchIndex");
            let search_index = jzon::parse(&search_index_raw[1])?;
            for item in search_index
                .as_array()
                .expect("searchIndex json must be array")
            {
                let crate_name = item[0].as_str().unwrap();
                let crate_data = item[1].clone();
                crates.insert(crate_name.to_owned(), crate_data);
            }
        }

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

        // write search-index.js
        let search_index_items = crates
            .iter()
            .map(|(name, data)| jzon::array![name.as_str(), data.to_owned()]);
        let search_index_json = JsonValue::Array(search_index_items.collect());
        write!(
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.dest.as_path().join("search-index.js"))?,
            include_str!("./templates/search-index.js"),
            searchIndexJson = search_index_json,
        )?;

        if let Some(index_crate) = self.index_crate.as_deref() {
            let index_path = self.dest.as_path().join("index.html");
            if fs::exists(&index_path)? {
                fs::remove_file(&index_path)?;
            }
            #[cfg(unix)]
            let symlink = std::os::unix::fs::symlink;
            #[cfg(windows)]
            let symlink = std::os::windows::fs::symlink_file;
            symlink(
                self.dest.as_path().join(index_crate).join("index.html"),
                &index_path,
            )?;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let doc_merge = DocMerge::parse();
    doc_merge.execute()
}
