//! Byte serializers and deserializers for supported file formats.

use crate::{Element, Geometry};

/// Independently usable byte streams produced by a format serializer.
///
/// Each inner vector is the complete contents of one destination file or
/// transport payload. The number of streams depends on the destination
/// format's native container capabilities.
pub type ByteStreams = Vec<Vec<u8>>;

pub mod las;
pub mod laz;
pub mod obj;
pub mod omf;
pub mod tiff;

pub(crate) fn all_elements(elements: &[Element]) -> Vec<&Element> {
    fn append<'a>(elements: &'a [Element], output: &mut Vec<&'a Element>) {
        for element in elements {
            output.push(element);
            if let Geometry::Composite(composite) = &element.geometry {
                append(&composite.elements, output);
            }
        }
    }

    let mut output = Vec::new();
    append(elements, &mut output);
    output
}
