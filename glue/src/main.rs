use std::fs::File;

use anyhow::Result;
use binrw::{BinRead, BinWrite};
use clap::{Parser, Subcommand};
use prettytable::{Table, format, row};

use glue::Archive;

const DEFAULT_ARCHIVE_FILE: &str = "archive.glue";
const DEFAULT_PREVIEW_SIZE: u64 = 64;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Create {
            archive: target,
            source,
        } => {
            let archive = Archive::from_paths(source)?;
            let mut output = File::create(target)?;
            archive.write(&mut output)?;
        }

        Commands::Inspect {
            archive: target,
            preview_size,
        } => {
            let preview_size = *preview_size;
            let mut reader = File::open(target)?;
            let archive = Archive::read(&mut reader)?;

            println!("{}:", target);
            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
            let titles = row![
               "NAME",
                c->"SIZE",
               "PREVIEW",
            ];
            table.set_titles(titles);

            for record in archive.records {
                let preview = match record.preview(preview_size) {
                    Some(preview) => match preview {
                        glue::Preview::String {
                            mut preview,
                            truncated,
                        } => {
                            if truncated {
                                preview += "...";
                            }
                            preview
                        }
                        glue::Preview::Data => String::from("<data>"),
                    },
                    None => String::from("<empty>"),
                };
                table.add_row(row![
                    record.filename,
                    r->format!("{} bytes", record.size),
                    preview
                ]);
            }

            table.printstd();
        }

        Commands::Extract { archive } => {
            let mut reader = File::open(archive)?;
            let archive = Archive::read(&mut reader)?;

            let dir = std::env::current_dir()?;
            archive.extract(dir)?;
        }
    }
    Ok(())
}

/// A lightweight file archive / unarchive utility.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a glue archive out of source files.
    ///
    /// The path will be elided in the output archive, keeping
    /// only the base name of the provided source(s).
    Create {
        #[arg(short, long, default_value = DEFAULT_ARCHIVE_FILE)]
        /// The path to the output archive file.
        archive: String,

        /// The input files to add to the archive.
        #[arg(required = true)]
        source: Vec<String>,
    },

    /// Inspect the contents of an archive file.
    Inspect {
        /// The archive file you want to inspect.
        #[arg(required = true)]
        archive: String,

        /// The length of text to preview in the output table.
        #[arg(short, long, default_value_t = DEFAULT_PREVIEW_SIZE)]
        preview_size: u64,
    },

    /// Extract the contents of an archive file.
    Extract {
        /// The archive file you want to extract.
        #[arg(required = true)]
        archive: String,
    },
}
