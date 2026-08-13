//! LAS conversion through the OMF data model.

use std::io::Cursor;

use crate::{
    Attribute, AttributeData, Element, Geometry, Location, PointSet, Project,
    format::{
        ByteStreams, all_elements,
        omf::{Document, MemoryWriter},
    },
};

const INTENSITY: &str = "las:intensity";
const RETURN_NUMBER: &str = "las:return_number";
const NUMBER_OF_RETURNS: &str = "las:number_of_returns";
const SCAN_DIRECTION: &str = "las:scan_direction_left_to_right";
const EDGE: &str = "las:edge_of_flight_line";
const CLASSIFICATION: &str = "las:classification";
const SYNTHETIC: &str = "las:synthetic";
const KEY_POINT: &str = "las:key_point";
const WITHHELD: &str = "las:withheld";
const OVERLAP: &str = "las:overlap";
const SCANNER_CHANNEL: &str = "las:scanner_channel";
const SCAN_ANGLE: &str = "las:scan_angle";
const USER_DATA: &str = "las:user_data";
const POINT_SOURCE_ID: &str = "las:point_source_id";
const GPS_TIME: &str = "las:gps_time";
const RED: &str = "las:red";
const GREEN: &str = "las:green";
const BLUE: &str = "las:blue";
const NIR: &str = "las:nir";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Las,
    Laz,
}

impl Encoding {
    fn is_compressed(self) -> bool {
        self == Self::Laz
    }

    fn name(self) -> &'static str {
        match self {
            Self::Las => "LAS",
            Self::Laz => "LAZ",
        }
    }
}

/// A LAS or LAZ conversion failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write LAS/LAZ: {0}")]
    Las(#[from] ::las::Error),
    #[error("could not read or write OMF: {0}")]
    Omf(#[from] ::omf::error::Error),
    #[error("expected {expected} bytes but received {actual} bytes")]
    UnexpectedEncoding { expected: &'static str, actual: &'static str },
    #[error("LAS waveform packets are not representable in OMF")]
    Waveform,
    #[error("LAS extra-byte records have no schema that can be represented safely in OMF")]
    ExtraBytes,
    #[error("the OMF document contains no point set that LAS/LAZ can serialize")]
    NothingToSerialize,
    #[error("OMF attribute {attribute:?} on element {element:?} is invalid for LAS: {reason}")]
    Attribute { element: String, attribute: String, reason: String },
}

/// Deserialize LAS bytes into the shared in-memory OMF representation.
pub fn deserialize(bytes: &[u8]) -> Result<Document, Error> {
    deserialize_with_encoding(bytes, Encoding::Las)
}

pub(crate) fn deserialize_with_encoding(bytes: &[u8], encoding: Encoding) -> Result<Document, Error> {
    Document::build(|writer| read_project(bytes, writer, encoding))
}

fn read_project(bytes: &[u8], writer: &mut MemoryWriter, encoding: Encoding) -> Result<Project, Error> {
    let mut reader = ::las::Reader::new(Cursor::new(bytes.to_vec()))?;
    let header = reader.header().clone();
    if header.point_format().is_compressed != encoding.is_compressed() {
        return Err(Error::UnexpectedEncoding {
            expected: encoding.name(),
            actual: if header.point_format().is_compressed { "LAZ" } else { "LAS" },
        });
    }
    if header.point_format().extra_bytes != 0 {
        return Err(Error::ExtraBytes);
    }

    let point_data = reader.read_all()?;
    let points = point_data.points().collect::<Result<Vec<_>, _>>()?;
    if points.iter().any(|point| point.waveform.is_some()) {
        return Err(Error::Waveform);
    }
    if points.iter().any(|point| !point.extra_bytes.is_empty()) {
        return Err(Error::ExtraBytes);
    }

    let vertices = points.iter().map(|point| [point.x, point.y, point.z]);
    let mut element = Element::new("LAS points", PointSet::new(writer.array_vertices(vertices)?));

    push_i64(writer, &mut element, INTENSITY, points.iter().map(|p| Some(i64::from(p.intensity))))?;
    push_i64(writer, &mut element, RETURN_NUMBER, points.iter().map(|p| Some(i64::from(p.return_number))))?;
    push_i64(writer, &mut element, NUMBER_OF_RETURNS, points.iter().map(|p| Some(i64::from(p.number_of_returns))))?;
    push_bool(
        writer,
        &mut element,
        SCAN_DIRECTION,
        points.iter().map(|p| Some(p.scan_direction == ::las::point::ScanDirection::LeftToRight)),
    )?;
    push_bool(writer, &mut element, EDGE, points.iter().map(|p| Some(p.is_edge_of_flight_line)))?;
    push_i64(writer, &mut element, CLASSIFICATION, points.iter().map(|p| Some(i64::from(u8::from(p.classification)))))?;
    push_bool(writer, &mut element, SYNTHETIC, points.iter().map(|p| Some(p.is_synthetic)))?;
    push_bool(writer, &mut element, KEY_POINT, points.iter().map(|p| Some(p.is_key_point)))?;
    push_bool(writer, &mut element, WITHHELD, points.iter().map(|p| Some(p.is_withheld)))?;
    push_bool(writer, &mut element, OVERLAP, points.iter().map(|p| Some(p.is_overlap)))?;
    push_i64(writer, &mut element, SCANNER_CHANNEL, points.iter().map(|p| Some(i64::from(p.scanner_channel))))?;
    push_f64(writer, &mut element, SCAN_ANGLE, points.iter().map(|p| Some(f64::from(p.scan_angle))))?;
    push_i64(writer, &mut element, USER_DATA, points.iter().map(|p| Some(i64::from(p.user_data))))?;
    push_i64(writer, &mut element, POINT_SOURCE_ID, points.iter().map(|p| Some(i64::from(p.point_source_id))))?;

    if header.point_format().has_gps_time {
        push_f64(writer, &mut element, GPS_TIME, points.iter().map(|p| p.gps_time))?;
    }
    if header.point_format().has_color {
        push_i64(writer, &mut element, RED, points.iter().map(|p| p.color.map(|c| i64::from(c.red))))?;
        push_i64(writer, &mut element, GREEN, points.iter().map(|p| p.color.map(|c| i64::from(c.green))))?;
        push_i64(writer, &mut element, BLUE, points.iter().map(|p| p.color.map(|c| i64::from(c.blue))))?;
    }
    if header.point_format().has_nir {
        push_i64(writer, &mut element, NIR, points.iter().map(|p| p.nir.map(i64::from)))?;
    }

    let mut project = Project::new("LAS project");
    project.application = format!("mfsd {}", env!("CARGO_PKG_VERSION"));
    project.coordinate_reference_system = crs_from_header(&header)?;
    project.elements.push(element);
    Ok(project)
}

/// Serialize every point set in an OMF document into LAS byte streams.
///
/// Each point set becomes one standalone LAS stream. Other elements, solid
/// colors, and attributes without a LAS mapping are omitted.
pub fn serialize(document: &Document) -> Result<ByteStreams, Error> {
    serialize_with_encoding(document, Encoding::Las)
}

pub(crate) fn serialize_with_encoding(document: &Document, encoding: Encoding) -> Result<ByteStreams, Error> {
    let mut streams = Vec::new();
    for element in all_elements(&document.project().elements) {
        let Geometry::PointSet(point_set) = &element.geometry else {
            continue;
        };
        streams.push(serialize_point_set(document, element, point_set, encoding)?);
    }
    if streams.is_empty() {
        return Err(Error::NothingToSerialize);
    }
    Ok(streams)
}

fn serialize_point_set(document: &Document, element: &Element, point_set: &PointSet, encoding: Encoding) -> Result<Vec<u8>, Error> {
    let vertices = read_vertices(document, point_set)?;
    let attributes = LasAttributes::read(document, element, vertices.len())?;
    let format_id = attributes.point_format(element)?;

    let mut builder = ::las::Builder::from(::las::Version::new(1, 4));
    builder.point_format = ::las::point::Format::new(format_id)?;
    builder.point_format.is_compressed = encoding.is_compressed();
    builder.generating_software = format!("mfsd {}", env!("CARGO_PKG_VERSION"));
    let mut header = builder.into_header()?;
    let crs = document.project().coordinate_reference_system.trim();
    if looks_like_wkt(crs) {
        let mut bytes = crs.as_bytes().to_vec();
        if !bytes.ends_with(&[0]) {
            bytes.push(0);
        }
        header.set_wkt_crs(bytes)?;
    }

    let mut output = ::las::Writer::new(Cursor::new(Vec::new()), header)?;
    for (i, [x, y, z]) in vertices.into_iter().enumerate() {
        let mut point = ::las::Point { x, y, z, ..Default::default() };
        point.intensity = attributes.u16(INTENSITY, i, 0, element)?;
        point.return_number = attributes.u8(RETURN_NUMBER, i, 0, element)?;
        point.number_of_returns = attributes.u8(NUMBER_OF_RETURNS, i, 0, element)?;
        point.scan_direction = if attributes.boolean(SCAN_DIRECTION, i, false, element)? {
            ::las::point::ScanDirection::LeftToRight
        } else {
            ::las::point::ScanDirection::RightToLeft
        };
        point.is_edge_of_flight_line = attributes.boolean(EDGE, i, false, element)?;
        let classification = attributes.u8(CLASSIFICATION, i, 0, element)?;
        if classification == 12 {
            point.classification = ::las::point::Classification::Unclassified;
            point.is_overlap = true;
        } else {
            point.classification = ::las::point::Classification::new(classification)?;
        }
        point.is_synthetic = attributes.boolean(SYNTHETIC, i, false, element)?;
        point.is_key_point = attributes.boolean(KEY_POINT, i, false, element)?;
        point.is_withheld = attributes.boolean(WITHHELD, i, false, element)?;
        point.is_overlap |= attributes.boolean(OVERLAP, i, false, element)?;
        point.scanner_channel = attributes.u8(SCANNER_CHANNEL, i, 0, element)?;
        point.scan_angle = attributes.f64(SCAN_ANGLE, i, 0.0, element)? as f32;
        point.user_data = attributes.u8(USER_DATA, i, 0, element)?;
        point.point_source_id = attributes.u16(POINT_SOURCE_ID, i, 0, element)?;
        point.gps_time = attributes.optional_f64(GPS_TIME, i);

        let red = attributes.optional_u16(RED, i, element)?;
        let green = attributes.optional_u16(GREEN, i, element)?;
        let blue = attributes.optional_u16(BLUE, i, element)?;
        point.color = match (red, green, blue) {
            (None, None, None) => None,
            (Some(red), Some(green), Some(blue)) => Some(::las::Color::new(red, green, blue)),
            _ => {
                return Err(attribute_group_error(element, "LAS RGB channels must all be present or all be absent"));
            }
        };
        point.nir = attributes.optional_u16(NIR, i, element)?;
        output.write_point(point)?;
    }
    output.close()?;
    Ok(output.into_inner()?.into_inner())
}

fn push_i64(writer: &mut MemoryWriter, element: &mut Element, name: &str, values: impl IntoIterator<Item = Option<i64>>) -> Result<(), Error> {
    element.attributes.push(Attribute::from_numbers(name, Location::Vertices, writer.array_numbers(values)?));
    Ok(())
}

fn push_f64(writer: &mut MemoryWriter, element: &mut Element, name: &str, values: impl IntoIterator<Item = Option<f64>>) -> Result<(), Error> {
    element.attributes.push(Attribute::from_numbers(name, Location::Vertices, writer.array_numbers(values)?));
    Ok(())
}

fn push_bool(writer: &mut MemoryWriter, element: &mut Element, name: &str, values: impl IntoIterator<Item = Option<bool>>) -> Result<(), Error> {
    element.attributes.push(Attribute::from_booleans(name, Location::Vertices, writer.array_booleans(values)?));
    Ok(())
}

#[derive(Default)]
struct LasAttributes {
    integers: std::collections::HashMap<String, Vec<Option<i64>>>,
    floats: std::collections::HashMap<String, Vec<Option<f64>>>,
    booleans: std::collections::HashMap<String, Vec<Option<bool>>>,
}

impl LasAttributes {
    fn read(document: &Document, element: &Element, count: usize) -> Result<Self, Error> {
        let mut output = Self::default();
        for attribute in &element.attributes {
            if attribute.location != Location::Vertices {
                continue;
            }
            match &attribute.data {
                AttributeData::Number { values, .. } if is_integer_name(&attribute.name) => {
                    let values = document
                        .reader()
                        .array_numbers(values)?
                        .try_into_i64()
                        .map_err(|_| attribute_error(element, attribute, "expected integer values"))?
                        .collect::<Result<Vec<_>, _>>()?;
                    check_len(element, attribute, count, values.len())?;
                    output.integers.insert(attribute.name.clone(), values);
                }
                AttributeData::Number { values, .. } if is_float_name(&attribute.name) => {
                    let values = document
                        .reader()
                        .array_numbers(values)?
                        .try_into_f64()
                        .map_err(|_| attribute_error(element, attribute, "expected floating-point values"))?
                        .collect::<Result<Vec<_>, _>>()?;
                    check_len(element, attribute, count, values.len())?;
                    output.floats.insert(attribute.name.clone(), values);
                }
                AttributeData::Boolean { values } if is_boolean_name(&attribute.name) => {
                    let values = document.reader().array_booleans(values)?.collect::<Result<Vec<_>, _>>()?;
                    check_len(element, attribute, count, values.len())?;
                    output.booleans.insert(attribute.name.clone(), values);
                }
                _ => {}
            }
        }
        Ok(output)
    }

    fn point_format(&self, element: &Element) -> Result<u8, Error> {
        let has_gps = self.floats.contains_key(GPS_TIME);
        let color_count = [RED, GREEN, BLUE].into_iter().filter(|name| self.integers.contains_key(*name)).count();
        if color_count != 0 && color_count != 3 {
            return Err(attribute_group_error(element, "LAS RGB channels must all be present or all be absent"));
        }
        let has_color = color_count == 3;
        let has_nir = self.integers.contains_key(NIR);
        let extended = self.any_integer_above(RETURN_NUMBER, 7)
            || self.any_integer_above(NUMBER_OF_RETURNS, 7)
            || self.any_integer_above(CLASSIFICATION, 31)
            || self.any_integer_above(SCANNER_CHANNEL, 0);

        if (extended || has_nir) && !has_gps {
            return Err(attribute_group_error(element, "extended LAS point fields and NIR require las:gps_time"));
        }
        if has_nir && !has_color {
            return Err(attribute_group_error(element, "LAS NIR requires all three LAS color channels"));
        }
        Ok(if has_nir {
            8
        } else if extended && has_color {
            7
        } else if extended {
            6
        } else {
            match (has_gps, has_color) {
                (false, false) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (true, true) => 3,
            }
        })
    }

    fn any_integer_above(&self, name: &str, limit: i64) -> bool {
        self.integers.get(name).is_some_and(|values| values.iter().flatten().any(|&value| value > limit))
    }

    fn integer(&self, name: &str, i: usize, default: i64, element: &Element) -> Result<i64, Error> {
        match self.integers.get(name) {
            None => Ok(default),
            Some(values) => values[i].ok_or_else(|| named_attribute_error(element, name, "null is not valid for this LAS field")),
        }
    }

    fn u8(&self, name: &str, i: usize, default: u8, element: &Element) -> Result<u8, Error> {
        self.integer(name, i, i64::from(default), element)?
            .try_into()
            .map_err(|_| named_attribute_error(element, name, "value is outside the u8 range"))
    }

    fn u16(&self, name: &str, i: usize, default: u16, element: &Element) -> Result<u16, Error> {
        self.integer(name, i, i64::from(default), element)?
            .try_into()
            .map_err(|_| named_attribute_error(element, name, "value is outside the u16 range"))
    }

    fn optional_u16(&self, name: &str, i: usize, element: &Element) -> Result<Option<u16>, Error> {
        self.integers
            .get(name)
            .map(|values| {
                values[i]
                    .map(|value| value.try_into().map_err(|_| named_attribute_error(element, name, "value is outside the u16 range")))
                    .transpose()
            })
            .transpose()
            .map(Option::flatten)
    }

    fn f64(&self, name: &str, i: usize, default: f64, element: &Element) -> Result<f64, Error> {
        match self.floats.get(name) {
            None => Ok(default),
            Some(values) => values[i].ok_or_else(|| named_attribute_error(element, name, "null is not valid for this LAS field")),
        }
    }

    fn optional_f64(&self, name: &str, i: usize) -> Option<f64> {
        self.floats.get(name).and_then(|values| values[i])
    }

    fn boolean(&self, name: &str, i: usize, default: bool, element: &Element) -> Result<bool, Error> {
        match self.booleans.get(name) {
            None => Ok(default),
            Some(values) => values[i].ok_or_else(|| named_attribute_error(element, name, "null is not valid for this LAS field")),
        }
    }
}

fn read_vertices(document: &Document, point_set: &PointSet) -> Result<Vec<[f64; 3]>, Error> {
    let project_origin = document.project().origin;
    document
        .reader()
        .array_vertices(&point_set.vertices)?
        .map(|vertex| {
            let vertex = vertex?;
            Ok([
                vertex[0] + point_set.origin[0] + project_origin[0],
                vertex[1] + point_set.origin[1] + project_origin[1],
                vertex[2] + point_set.origin[2] + project_origin[2],
            ])
        })
        .collect()
}

fn crs_from_header(header: &::las::Header) -> Result<String, Error> {
    if let Some(bytes) = header.get_wkt_crs_bytes() {
        return Ok(String::from_utf8_lossy(bytes).trim_end_matches('\0').to_owned());
    }
    if let Some(crs) = header.get_geotiff_crs()?
        && let Some(code) = crs.get_projected_crs_geo_key_value().or_else(|| crs.get_geodetic_crs_geo_key_value())
    {
        return Ok(format!("EPSG:{code}"));
    }
    Ok(String::new())
}

fn looks_like_wkt(crs: &str) -> bool {
    let upper = crs.trim_start().to_ascii_uppercase();
    ["PROJCRS[", "GEOGCRS[", "GEODCRS[", "COMPOUNDCRS[", "VERTCRS[", "PROJCS[", "GEOGCS["]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
}

fn is_integer_name(name: &str) -> bool {
    [
        INTENSITY,
        RETURN_NUMBER,
        NUMBER_OF_RETURNS,
        CLASSIFICATION,
        SCANNER_CHANNEL,
        USER_DATA,
        POINT_SOURCE_ID,
        RED,
        GREEN,
        BLUE,
        NIR,
    ]
    .contains(&name)
}

fn is_float_name(name: &str) -> bool {
    [SCAN_ANGLE, GPS_TIME].contains(&name)
}

fn is_boolean_name(name: &str) -> bool {
    [SCAN_DIRECTION, EDGE, SYNTHETIC, KEY_POINT, WITHHELD, OVERLAP].contains(&name)
}

fn check_len(element: &Element, attribute: &Attribute, expected: usize, found: usize) -> Result<(), Error> {
    if expected == found {
        Ok(())
    } else {
        Err(attribute_error(element, attribute, format!("length {found} does not match point count {expected}")))
    }
}

fn attribute_error(element: &Element, attribute: &Attribute, reason: impl Into<String>) -> Error {
    named_attribute_error(element, &attribute.name, reason)
}

fn named_attribute_error(element: &Element, attribute: &str, reason: impl Into<String>) -> Error {
    Error::Attribute {
        element: element.name.clone(),
        attribute: attribute.to_owned(),
        reason: reason.into(),
    }
}

fn attribute_group_error(element: &Element, reason: impl Into<String>) -> Error {
    named_attribute_error(element, "LAS attribute group", reason)
}
