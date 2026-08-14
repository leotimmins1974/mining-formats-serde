//! Serialize and deserialize mining formats through one in-memory OMF representation.

mod ffi;

pub mod format;

pub use format::omf::{Document, Error as OmfError, MemoryReader, MemoryWriter, deserialize as deserialize_omf, serialize as serialize_omf};
pub use omf::{
    Array, Attribute, AttributeData, BlockModel, Color, Composite, DataType, Element, Geometry, Grid2, Grid3, GridSurface, LineSet, Location, NumberColormap, NumberRange, Orient2,
    Orient3, PointSet, Project, SubblockMode, Subblocks, Surface, Vector3, array_type,
};
