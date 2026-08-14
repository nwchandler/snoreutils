use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use binrw::{NullString, binrw};
use thiserror::Error;

/// Archive represents a collection of file contents, grouped
/// together into a single unit.
#[binrw]
#[brw(big, magic = b"GLUE")]
#[derive(Debug, Default)]
pub struct Archive {
    /// The number of records (files) in the archive.
    #[bw(try_calc(u32::try_from(records.len())))]
    #[brw(align_after = 16)]
    pub record_count: u32,

    /// The collection of records (files) in the archive.
    #[br(count = record_count)]
    pub records: Vec<FileRecord>,
}

impl Archive {
    /// Instantiates an [`Archive`] from a slice of paths. As long as there
    /// are no errors in reading the referenced files, and there are
    /// no duplicate filenames, the [`Archive`] will include records for each file.
    ///
    /// # Errors
    /// Problems in creating the archive will return an [`ArchiveCreationError`].
    pub fn from_paths<P: AsRef<Path>>(paths: &[P]) -> Result<Self, ArchiveCreationError> {
        let mut records = Vec::new();

        let mut duplicate_filenames_detected = false;
        let mut names_to_paths: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for path in paths {
            let record = FileRecord::from_path(path)?;
            let filename = record.filename.to_string();

            if let Some(existing) = names_to_paths.get_mut(&filename) {
                duplicate_filenames_detected = true;
                existing.push(path.as_ref().to_path_buf());
            } else {
                names_to_paths.insert(filename, vec![path.as_ref().to_path_buf()]);
            }

            records.push(record);
        }
        if duplicate_filenames_detected {
            Err(ArchiveCreationError::DuplicateFilenames(FilenameToPath(
                names_to_paths,
            )))
        } else {
            Ok(Self { records })
        }
    }

    /// Extract [`Archive`] contents to the provided directory path.
    ///
    /// # Errors
    /// Problems creating the output files or invalid archive contents will
    /// return [`ArchiveExtractionError`].
    pub fn extract<P: AsRef<Path>>(&self, dir: P) -> Result<(), ArchiveExtractionError> {
        for record in self.records.iter() {
            let filename = record.filename.to_string();
            if !Self::filename_is_valid(&filename) {
                return Err(ArchiveExtractionError::InvalidFilename(filename.into()));
            }
            let path = dir.as_ref().join(filename);

            let mut file = File::create_new(path)?;
            if record.extract(&mut file).is_err() {
                return Err(ArchiveExtractionError::FileRecordError);
            }
        }
        Ok(())
    }

    // This function does a basic check on a filename to avoid bad input (such
    // as non-UTF-8 names and path traversals). This is a very simplistic
    // implementation currently, relying on lexical evaluation and very
    // simple filename assumptions.
    fn filename_is_valid<P: AsRef<Path>>(filename: P) -> bool {
        let Some(path_string) = filename.as_ref().to_str() else {
            return false;
        };
        if path_string.contains(['/', '\\']) {
            return false;
        }
        true
    }
}

/// This type represents errors that can occur when extracting an [`Archive`].
#[derive(Error, Debug)]
pub enum ArchiveExtractionError {
    #[error("unable to create file: {0}")]
    FileCreationError(#[from] std::io::Error),

    #[error("error creating file from file record")]
    FileRecordError,

    #[error("invalid filename: {0}")]
    InvalidFilename(PathBuf),
}

/// This type represents errors that can occur when creating a new [`Archive`].
#[derive(Error, Debug)]
pub enum ArchiveCreationError {
    /// The filenames in an archive must be unique.
    #[error("duplicate filenames detected: {0}")]
    DuplicateFilenames(FilenameToPath),

    /// Uncategorized error.
    // FIXME: Replace anyhow with more precise error types
    #[error("an unexpected error occurred")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct FilenameToPath(HashMap<String, Vec<PathBuf>>);

impl Display for FilenameToPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, paths) in self.0.iter() {
            write!(f, "{name}: {:?}; ", paths)?;
        }
        Ok(())
    }
}

/// A FileRecord represents the contents and basic metadata about
/// a file that is in an archive.
#[binrw]
#[brw(big)]
#[derive(Debug, Default)]
pub struct FileRecord {
    #[brw(align_after = 8)]
    pub filename: NullString,

    // usize would probably be more natural here, but that is not reliably
    // serializable/deserializable between systems. Because u64 is a common
    // max file size on most current systems (and is the type of file size in
    // file metadata [std::fs::Metadata]), that's what we use here.
    pub size: u64,

    #[br(count = size)]
    #[brw(align_after = 16)]
    pub content: Vec<u8>,
}

impl FileRecord {
    /// Instantiate a [`FileRecord`] from the contents of path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let filename = match path.file_name() {
            Some(filename) => match filename.to_str() {
                Some(filename) => filename.into(),
                None => bail!("unable to convert filename"),
            },
            None => bail!("bad file path"),
        };
        let mut file = File::open(path)?;

        let size = file.metadata()?.len();

        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        Ok(Self {
            filename,
            size,
            content,
        })
    }

    /// Preview the contents of a [`FileRecord`]. If the record's contents
    /// are empty, [`None`] is returned. Otherwise, a [`Preview`] is returned.
    pub fn preview(&self, size: u64) -> Option<Preview> {
        if self.size == 0 {
            return None;
        }
        let mut truncated = self.size > size;
        let size = if truncated { size } else { self.size };

        // UTF-8 characters can be up to 4 bytes. If we fail to decode the
        // requested input size properly, we check whether we are inside of
        // a multi-byte UTF-8 character and cut back a bit, until we can be
        // sure that this is not text.
        for i in 0..size_of::<char>() {
            let preview = str::from_utf8(&self.content[..(size as usize) - i]);
            if preview.is_ok() {
                return Some(Preview::String {
                    preview: String::from(preview.unwrap()),
                    truncated,
                });
            }
            // If we haven't found a text string at this point, then we have
            // a truncated output.
            truncated = true;
        }

        Some(Preview::Data)
    }

    /// Extract the [`FileRecord`]'s contents to the provided writer.
    pub fn extract<W: Write>(&self, writer: &mut W) -> Result<()> {
        let content = self.content.as_slice();
        writer.write_all(content)?;
        Ok(())
    }
}

/// Preview of a [`FileRecord`]'s contents.
pub enum Preview {
    /// String represents a file whose contents are textual. It includes
    /// the preview, along with an indication of whether the preview is
    /// truncated or not. For short files, it is possible the preview could
    /// contain the entire contents, and truncated is set to false in this
    /// case, true otherwise.
    String { preview: String, truncated: bool },

    /// Data represents that the file's contents are not textual.
    Data,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::io::Read;
    use std::io::Write;
    use std::path::Path;

    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn file_record_extract_copies_all_data() {
        let input = vec![1, 2, 3, 4];
        let buffer = Vec::new();
        let mut writer = Cursor::new(buffer);

        let file_record = super::FileRecord {
            filename: "foo.txt".into(),
            size: input.len() as u64,
            content: input.clone(),
        };

        assert!(file_record.extract(&mut writer).is_ok());

        let result = writer.into_inner();
        assert_eq!(result, input, "output content is not equal to input");
    }

    #[test]
    fn file_record_extract_errors_if_output_truncated() {
        let input = vec![1, 2, 3, 4];
        let buffer = [0; 2];
        let mut writer = Cursor::new(buffer);

        let file_record = super::FileRecord {
            filename: "foo.txt".into(),
            size: input.len() as u64,
            content: input,
        };

        assert!(file_record.extract(&mut writer).is_err());
    }

    #[test]
    fn file_record_from_path_fn_reads_an_existing_file() {
        let content = "Hello, world!\n";
        let mut temp = NamedTempFile::new().expect("create temp file");
        write!(&mut temp, "{}", &content).expect("write to temp file");

        let file_record =
            super::FileRecord::from_path(temp.path()).expect("create FileRecord from temp file");

        let temp_filename = temp.path().file_name().unwrap().to_str().unwrap();

        assert_eq!(
            file_record.filename,
            temp_filename.into(),
            "unexpected FileRecord filename"
        );
        assert_eq!(
            file_record.size,
            content.len().try_into().unwrap(),
            "unexpected FileRecord size"
        );
        assert_eq!(
            file_record.content,
            content.as_bytes().to_vec(),
            "unexpected FileRecord content"
        )
    }

    #[test]
    fn archive_from_paths_detects_duplicate_filenames() {
        use std::fs::File;

        let filename = "my-file.txt";

        let mut dirs = vec![];
        let mut paths = vec![];
        for _ in 0..2 {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join(filename);
            File::create_new(&path).unwrap();
            dirs.push(dir);
            paths.push(path);
        }

        let result = super::Archive::from_paths(&paths);

        assert!(
            result.is_err(),
            "archive with duplicate filenames should return error"
        );
    }

    #[test]
    fn archive_extract() {
        let dir = TempDir::new().unwrap();
        let dir = dir.path();

        let archive = super::Archive {
            records: vec![
                super::FileRecord {
                    filename: "1.txt".into(),
                    size: 5,
                    content: "hello".into(),
                },
                super::FileRecord {
                    filename: "2.txt".into(),
                    size: 6,
                    content: vec![1, 2, 3, 4, 5, 6],
                },
            ],
        };

        archive.extract(&dir).unwrap();

        let dir_entries = std::fs::read_dir(&dir).unwrap();
        assert_eq!(
            dir_entries.count(),
            2,
            "there should be two extracted files"
        );

        let mut file = std::fs::File::open(dir.join("1.txt")).unwrap();
        let mut buf = vec![];
        assert_eq!(5, file.read_to_end(&mut buf).unwrap());
        assert_eq!("hello".as_bytes(), buf);

        let mut file = std::fs::File::open(dir.join("2.txt")).unwrap();
        let mut buf = vec![];
        assert_eq!(6, file.read_to_end(&mut buf).unwrap());
        assert_eq!(vec![1, 2, 3, 4, 5, 6], buf);
    }

    #[test]
    fn test_archive_filename_is_valid() {
        let path = Path::new("foo.txt");
        assert!(super::Archive::filename_is_valid(path));

        let path = Path::new("./foo.txt");
        assert!(!super::Archive::filename_is_valid(path));

        let path = Path::new("../foo.txt");
        assert!(!super::Archive::filename_is_valid(path));

        let path = Path::new("C:\\foo.txt");
        assert!(!super::Archive::filename_is_valid(path));
    }

    #[test]
    fn archive_extract_prevents_invalid_filenames() {
        let dir = TempDir::new().unwrap();
        let dir = dir.path();

        let subdir = dir.join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        let archive = super::Archive {
            records: vec![super::FileRecord {
                filename: "../1.txt".into(),
                size: 5,
                content: "hello".into(),
            }],
        };

        assert!(matches!(
            archive.extract(&subdir),
            Err(super::ArchiveExtractionError::InvalidFilename(_))
        ));
    }

    #[test]
    fn preview_handles_utf8() {
        let input = "Hello 🦀";
        let raw_input = input.as_bytes();
        let record = super::FileRecord {
            filename: "foo.txt".into(),
            size: raw_input.len() as u64,
            content: raw_input.to_vec(),
        };

        // Cut off before the emoji
        let super::Preview::String { preview, .. } = record.preview(5).unwrap() else {
            panic!("preview failed for partial contents")
        };
        assert_eq!(preview, "Hello");

        // Include the whole contents
        let super::Preview::String { preview, .. } =
            record.preview(raw_input.len() as u64).unwrap()
        else {
            panic!("preview failed for full contents")
        };
        assert_eq!(preview, "Hello 🦀");

        // Cut off mid-emoji
        let super::Preview::String { preview, .. } = record.preview(7).unwrap() else {
            panic!("preview failed in mid-emoji")
        };
        assert_eq!(preview, "Hello ");
    }
}
