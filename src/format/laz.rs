//! LAZ conversion through the OMF data model.

pub use super::las::Error;
use super::{
    ByteStreams,
    las::{self, Encoding},
    omf::Document,
};

/// Deserialize LAZ bytes into the shared in-memory OMF representation.
pub fn deserialize(bytes: &[u8]) -> Result<Document, Error> {
    las::deserialize_with_encoding(bytes, Encoding::Laz)
}

/// Serialize every point set in an OMF document into LAZ byte streams.
///
/// Each point set becomes one standalone LAZ stream. Other elements, solid
/// colors, and attributes without a LAS mapping are omitted.
pub fn serialize(document: &Document) -> Result<ByteStreams, Error> {
    las::serialize_with_encoding(document, Encoding::Laz)
}
