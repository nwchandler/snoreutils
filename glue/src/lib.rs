use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::{Result, bail};
use binrw::{NullString, binrw};

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
    /// Instantiates an [Archive] from a slice of paths. As long as there
    /// are no errors in reading the referenced files, the [Archive] will
    /// include records for each file.
    pub fn from_paths<P: AsRef<Path>>(paths: &[P]) -> Result<Self> {
        let mut records = Vec::new();
        for path in paths {
            records.push(FileRecord::from_path(path)?);
        }
        Ok(Self { records })
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

    pub size: u32,

    #[br(count = size)]
    #[brw(align_after = 16)]
    pub content: Vec<u8>,
}

impl FileRecord {
    /// Instantiate a [FileRecord] from the contents of path.
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

        let size = file.metadata()?.len() as u32;

        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        Ok(Self {
            filename,
            size,
            content,
        })
    }

    /// Preview the contents of a [FileRecord]. If the record's contents
    /// are empty, [None] is returned. Otherwise, a [Preview] is returned.
    pub fn preview(&self, size: u32) -> Option<Preview> {
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

    /// Extract the [FileRecord]s contents to the provided writer.
    pub fn extract<W: Write>(&self, writer: &mut W) -> Result<()> {
        let content = &self.content.as_slice();
        writer.write(&content)?;
        Ok(())
    }
}

/// Preview of a [FileRecord]s contents.
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
    use std::io::Write;

    use tempfile::NamedTempFile;

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
            content.len() as u32,
            "unexpected FileRecord size"
        );
        assert_eq!(
            file_record.content,
            content.as_bytes().to_vec(),
            "unexpected FileRecord content"
        )
    }
}
