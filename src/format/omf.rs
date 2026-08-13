//! Open Mining Format support and the shared in-memory document.

use std::{io::Cursor, sync::Arc};

pub use ::omf::error::Error;
use ::omf::{
    Project,
    file::{ReadAt, Reader, Writer},
    validate::Problems,
};

/// Shared immutable bytes backing an in-memory OMF document.
#[derive(Clone)]
pub struct Memory(Arc<[u8]>);

impl Memory {
    fn new(bytes: Vec<u8>) -> Self {
        Self(bytes.into())
    }

    /// Return the complete encoded OMF archive.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl ReadAt for Memory {
    fn read_at(&self, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
        let start = usize::try_from(offset).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "offset is too large"))?;
        if start >= self.0.len() {
            return Ok(0);
        }
        let end = start.saturating_add(buffer.len()).min(self.0.len());
        let source = &self.0[start..end];
        buffer[..source.len()].copy_from_slice(source);
        Ok(source.len())
    }

    fn size(&self) -> std::io::Result<u64> {
        self.0.len().try_into().map_err(|_| std::io::Error::other("OMF document is too large"))
    }
}

/// The reader used to decode arrays in an in-memory [`Document`].
pub type MemoryReader = Reader<Memory>;

/// The writer used while constructing an in-memory [`Document`].
pub type MemoryWriter = Writer<Cursor<Vec<u8>>>;

/// An owned OMF document held entirely in memory.
///
/// This is the interchange representation used by every format adapter. It
/// retains the standard OMF [`Project`] and an OMF reader for decoding all
/// referenced typed arrays and images; no format-specific intermediate schema
/// is introduced.
pub struct Document {
    bytes: Memory,
    reader: MemoryReader,
    project: Project,
    warnings: Problems,
}

impl Document {
    fn from_vec(bytes: Vec<u8>) -> Result<Self, Error> {
        let bytes = Memory::new(bytes);
        let reader = Reader::new(bytes.clone())?;
        let (project, warnings) = reader.project()?;
        Ok(Self { bytes, reader, project, warnings })
    }

    /// Build an in-memory OMF document from the standard OMF writer and model.
    ///
    /// Format readers use this method to populate OMF arrays and return the
    /// resulting document without first writing a temporary `.omf` file.
    pub fn build<E>(build: impl FnOnce(&mut MemoryWriter) -> Result<Project, E>) -> Result<Self, E>
    where
        E: From<Error>,
    {
        let mut writer = MemoryWriter::new(Cursor::new(Vec::new())).map_err(E::from)?;
        let project = build(&mut writer)?;
        let (cursor, _) = writer.finish(project).map_err(E::from)?;
        Self::from_vec(cursor.into_inner()).map_err(E::from)
    }

    /// The OMF format version represented by this document.
    pub fn format_version(&self) -> [u32; 2] {
        self.reader.version()
    }

    /// The standard OMF project, including every element and array descriptor.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Non-fatal OMF validation warnings.
    pub fn warnings(&self) -> &Problems {
        &self.warnings
    }

    /// Access the OMF reader used to decode referenced arrays and images.
    pub fn reader(&self) -> &MemoryReader {
        &self.reader
    }

    /// Return the encoded OMF archive backing this document.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_bytes()
    }

    /// Consume the document and return its OMF project and reader.
    pub fn into_parts(self) -> (MemoryReader, Project, Problems) {
        (self.reader, self.project, self.warnings)
    }
}

/// Deserialize OMF bytes into the shared in-memory representation.
pub fn deserialize(bytes: &[u8]) -> Result<Document, Error> {
    Document::from_vec(bytes.to_vec())
}

/// Serialize an in-memory OMF document into one owned OMF byte stream.
pub fn serialize(document: &Document) -> super::ByteStreams {
    vec![document.as_bytes().to_vec()]
}
