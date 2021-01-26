use std::io;
use std::io::*;
use zip::*;
use zip::result::{ZipResult, ZipError};

/// find a file ending with the given character string in the archive, 
/// copy into memory (no longer requiring ownership of archive) and return a cursor to read that memory.
pub fn extract_match_to_memory<R: Read + io::Seek>(archive: &mut ZipArchive<R>, ending: &str) -> ZipResult<io::Cursor<Vec<u8>>> {
    //XXXXX::: NO!!!! order of file_names() does not correspond to by_index().
    // let file_number = archive.file_names().position(|f| f.ends_with(ending)});
    
    for file_number in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(file_number) {
            if file.name().ends_with(ending) {
                let mut buffer: Vec<u8> = vec![];
                let _bytes_read = file.read_to_end(&mut buffer)?;
                return Ok(io::Cursor::new(buffer));
            }
        }
    }
    Err(ZipError::Io(Error::new(ErrorKind::NotFound, 
        "No index found for file name with specified ending.")))
}
