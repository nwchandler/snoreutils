use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};

use anyhow::Result;

/// Copies the contents of the file at the path to the output.
pub fn file<P: AsRef<Path>, W: Write>(path: P, mut output: W) -> Result<()> {
    let file = fs::File::open(&path)?;
    stream(file, &mut output)?;
    Ok(())
}

/// Copies the contents of a generic reader to the output.
pub fn stream<R: Read, W: Write>(mut input: R, mut output: W) -> Result<()> {
    io::copy(&mut input, &mut output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use tempfile::NamedTempFile;

    #[test]
    fn file_fn_copies_text_content() {
        let mut file = NamedTempFile::new().expect("new named file");
        writeln!(file, "Hello, world!").expect("write file contents");

        let mut output = Vec::new();

        super::file(file.path(), &mut output).expect("cat::file failed");

        assert_eq!(String::from_utf8(output).unwrap(), "Hello, world!\n");
    }

    #[test]
    fn stream_fn_copies_text_input() {
        let buffer = "Hello, world!\n";
        let cursor = Cursor::new(buffer);

        let mut output = Vec::new();

        super::stream(cursor, &mut output).expect("cat::stream failed");

        assert_eq!(String::from_utf8(output).unwrap(), "Hello, world!\n");
    }

    #[test]
    fn stream_fn_copies_binary_input() {
        let buffer = b"\xAA\xBB\xCC\xDD\xEE\xFF";
        let cursor = Cursor::new(buffer);

        let mut output = Vec::new();

        super::stream(cursor, &mut output).expect("cat::stream failed");

        assert_eq!(output.as_slice(), buffer);
    }
}
