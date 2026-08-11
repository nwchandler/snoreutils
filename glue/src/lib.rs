use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs::File,
    io::{Read, Write},
    path::Path,
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
        let mut filenames = HashSet::new();
        let mut filename_to_path: Option<FilenameToPath> = None;
        for path in paths {
            let record = FileRecord::from_path(path)?;
            let filename = &record.filename.to_string();
            if !filenames.insert(filename.clone()) {
                if filename_to_path.is_none() {
                    filename_to_path = Some(FilenameToPath(HashMap::new()));
                }
                filename_to_path.as_mut().unwrap().append(
                    filename.clone(),
                    path.as_ref().to_string_lossy().to_string(),
                );
            }
            records.push(record);
        }
        if let Some(duplicate_filenames) = filename_to_path {
            Err(ArchiveCreationError::DuplicateFilenames(
                duplicate_filenames,
            ))
        } else {
            Ok(Self { records })
        }
    }
}

/// This type represents errors that can occur when creating a new ['Archive'].
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
pub struct FilenameToPath(HashMap<String, Vec<String>>);

impl FilenameToPath {
    fn append(&mut self, name: String, path: String) {
        self.0.entry(name).or_insert_with(Vec::new).push(path);
    }
}

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
        let truncated = self.size > size;
        let size = if truncated { size } else { self.size };

        let preview = str::from_utf8(&self.content[..size as usize]);
        match preview {
            // If we were able to decode as UTF-8, then treat this as
            // a string.
            Ok(s) => Some(Preview::String {
                preview: String::from(s),
                truncated,
            }),
            // If we were not able to decode as UTF-8, then treat it as
            // data instead.
            Err(_) => Some(Preview::Data),
        }
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
    use std::io::Write;

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
}
