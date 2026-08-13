//! Grayscale TIFF and GeoTIFF conversion through the OMF data model.

use std::io::Cursor;

use ::tiff::{
    ColorType,
    decoder::{Decoder, DecodingResult},
    encoder::{TiffEncoder, colortype::Gray64Float},
    tags::Tag,
};
use serde_json::{Value, json};

use crate::{
    Element, Geometry, Grid2, GridSurface, Orient2, Project,
    format::{
        ByteStreams, all_elements,
        omf::{Document, MemoryWriter},
    },
};

const GEO_KEY_DIRECTORY: &str = "mfsd:tiff:geo_key_directory";
const GEO_DOUBLE_PARAMS: &str = "mfsd:tiff:geo_double_params";
const GEO_ASCII_PARAMS: &str = "mfsd:tiff:geo_ascii_params";

/// A TIFF conversion failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write TIFF: {0}")]
    Tiff(#[from] ::tiff::TiffError),
    #[error("could not read or write OMF: {0}")]
    Omf(#[from] ::omf::error::Error),
    #[error("only single-band grayscale TIFF images are OMF grid surfaces; found {0:?}")]
    ColorType(ColorType),
    #[error("multi-page TIFF has no single OMF grid-surface equivalent")]
    MultipleImages,
    #[error("TIFF dimensions must both be at least two pixels; found {0} by {1}")]
    Dimensions(u32, u32),
    #[error("TIFF nodata pixels cannot be represented by OMF grid-surface heights")]
    Nodata,
    #[error("TIFF contains non-finite height data at pixel {0}")]
    NonFinite(usize),
    #[error("TIFF integer sample at pixel {0} cannot be represented exactly by an OMF scalar")]
    IntegerPrecision(usize),
    #[error("the GeoTIFF transformation is not a valid orthogonal OMF grid: {0}")]
    Transformation(String),
    #[error("the OMF document contains no regular grid surface that TIFF can serialize")]
    NothingToSerialize,
    #[error("OMF GeoTIFF metadata field {0:?} has the wrong type")]
    Metadata(String),
}

/// Deserialize single-band TIFF bytes into the shared in-memory OMF representation.
pub fn deserialize(bytes: &[u8]) -> Result<Document, Error> {
    Document::build(|writer| read_project(bytes, writer))
}

fn read_project(bytes: &[u8], writer: &mut MemoryWriter) -> Result<Project, Error> {
    let mut decoder = Decoder::new(Cursor::new(bytes))?;
    let (width, height) = decoder.dimensions()?;
    if width < 2 || height < 2 {
        return Err(Error::Dimensions(width, height));
    }
    let color_type = decoder.colortype()?;
    if !matches!(color_type, ColorType::Gray(_)) {
        return Err(Error::ColorType(color_type));
    }
    if decoder.more_images() {
        return Err(Error::MultipleImages);
    }
    if decoder.find_tag(Tag::GdalNodata)?.is_some() {
        return Err(Error::Nodata);
    }

    let pixel_scale = find_f64_vec(&mut decoder, Tag::ModelPixelScaleTag)?;
    let tiepoint = find_f64_vec(&mut decoder, Tag::ModelTiepointTag)?;
    let transformation = find_f64_vec(&mut decoder, Tag::ModelTransformationTag)?;
    let geo_keys = find_u16_vec(&mut decoder, Tag::GeoKeyDirectoryTag)?;
    let geo_doubles = find_f64_vec(&mut decoder, Tag::GeoDoubleParamsTag)?;
    let geo_ascii = find_ascii(&mut decoder, Tag::GeoAsciiParamsTag)?;

    let (orient, size) = georeference(pixel_scale.as_deref(), tiepoint.as_deref(), transformation.as_deref())?;
    let values = decoding_result_to_f64(decoder.read_image()?)?;
    if let Some(index) = values.iter().position(|value| !value.is_finite()) {
        return Err(Error::NonFinite(index));
    }
    if values.len() != width as usize * height as usize {
        return Err(Error::Transformation("decoded sample count does not match image dimensions".to_owned()));
    }

    let heights = writer.array_scalars(values)?;
    let surface = GridSurface::new(orient, Grid2::from_size_and_count(size, [width - 1, height - 1]), Some(heights));
    let element = Element::new("TIFF grid", surface);
    let mut project = Project::new("TIFF project");
    project.application = format!("mfsd {}", env!("CARGO_PKG_VERSION"));
    project.coordinate_reference_system = epsg_from_geo_keys(geo_keys.as_deref());
    if let Some(values) = geo_keys {
        project.metadata.insert(GEO_KEY_DIRECTORY.to_owned(), json!(values));
    }
    if let Some(values) = geo_doubles {
        project.metadata.insert(GEO_DOUBLE_PARAMS.to_owned(), json!(values));
    }
    if let Some(value) = geo_ascii {
        project.metadata.insert(GEO_ASCII_PARAMS.to_owned(), Value::String(value));
    }
    project.elements.push(element);
    Ok(project)
}

/// Serialize every compatible regular OMF grid surface into TIFF byte streams.
///
/// Each grid surface becomes one standalone TIFF stream. Other elements and
/// fields without a TIFF mapping are omitted.
pub fn serialize(document: &Document) -> Result<ByteStreams, Error> {
    let mut streams = Vec::new();
    for element in all_elements(&document.project().elements) {
        let Geometry::GridSurface(surface) = &element.geometry else {
            continue;
        };
        let Grid2::Regular { size, count } = surface.grid else {
            continue;
        };
        let Some(heights) = &surface.heights else {
            continue;
        };
        let values = document.reader().array_scalars(heights)?.collect::<Result<Vec<_>, _>>()?;
        if values.iter().any(|value| !value.is_finite()) {
            continue;
        }
        let origin = add3(surface.orient.origin, document.project().origin);
        let Ok(transformation) = transformation_from_omf(surface.orient, origin, size) else {
            continue;
        };
        let width = count[0] + 1;
        let height = count[1] + 1;

        let mut bytes = Vec::new();
        {
            let mut encoder = TiffEncoder::new(Cursor::new(&mut bytes))?;
            let mut image = encoder.new_image::<Gray64Float>(width, height)?;
            image.encoder().write_tag(Tag::ModelTransformationTag, &transformation[..])?;

            if let Some(value) = document.project().metadata.get(GEO_KEY_DIRECTORY)
                && let Ok(values) = json_u16_vec(value, GEO_KEY_DIRECTORY)
            {
                image.encoder().write_tag(Tag::GeoKeyDirectoryTag, &values[..])?;
            }
            if let Some(value) = document.project().metadata.get(GEO_DOUBLE_PARAMS)
                && let Ok(values) = json_f64_vec(value, GEO_DOUBLE_PARAMS)
            {
                image.encoder().write_tag(Tag::GeoDoubleParamsTag, &values[..])?;
            }
            if let Some(value) = document.project().metadata.get(GEO_ASCII_PARAMS)
                && let Some(value) = value.as_str()
            {
                image.encoder().write_tag(Tag::GeoAsciiParamsTag, value)?;
            }
            image.write_data(&values)?;
        }
        streams.push(bytes);
    }
    if streams.is_empty() {
        return Err(Error::NothingToSerialize);
    }
    Ok(streams)
}

fn decoding_result_to_f64(result: DecodingResult) -> Result<Vec<f64>, Error> {
    const MAX_EXACT_INTEGER: u64 = 1_u64 << 53;
    Ok(match result {
        DecodingResult::U8(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::U16(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::U32(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::U64(values) => values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (value <= MAX_EXACT_INTEGER).then_some(value as f64).ok_or(Error::IntegerPrecision(index)))
            .collect::<Result<Vec<_>, _>>()?,
        DecodingResult::F16(values) => values.into_iter().map(|value| f64::from(f32::from(value))).collect(),
        DecodingResult::F32(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::F64(values) => values,
        DecodingResult::I8(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::I16(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::I32(values) => values.into_iter().map(f64::from).collect(),
        DecodingResult::I64(values) => values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                ((-(MAX_EXACT_INTEGER as i64))..=MAX_EXACT_INTEGER as i64)
                    .contains(&value)
                    .then_some(value as f64)
                    .ok_or(Error::IntegerPrecision(index))
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn georeference(pixel_scale: Option<&[f64]>, tiepoint: Option<&[f64]>, transformation: Option<&[f64]>) -> Result<(Orient2, [f64; 2]), Error> {
    if let Some(matrix) = transformation {
        if matrix.len() != 16 {
            return Err(Error::Transformation("ModelTransformationTag must contain 16 values".to_owned()));
        }
        let du = [matrix[0], matrix[4], matrix[8]];
        let dv = [matrix[1], matrix[5], matrix[9]];
        let size = [length(du), length(dv)];
        if !size[0].is_finite() || !size[1].is_finite() || size[0] <= 0.0 || size[1] <= 0.0 {
            return Err(Error::Transformation("pixel axes must have finite, non-zero lengths".to_owned()));
        }
        let u = scale3(du, 1.0 / size[0]);
        let v = scale3(dv, 1.0 / size[1]);
        if dot(u, v).abs() > 1.0e-10 {
            return Err(Error::Transformation("pixel axes must be perpendicular".to_owned()));
        }
        return Ok((Orient2::new([matrix[3], matrix[7], matrix[11]], u, v), size));
    }

    match (pixel_scale, tiepoint) {
        (None, None) => Ok((Orient2::default(), [1.0, 1.0])),
        (Some(scale), Some(tie)) if scale.len() >= 2 && tie.len() >= 6 => {
            if !scale[0].is_finite() || !scale[1].is_finite() || scale[0] <= 0.0 || scale[1] <= 0.0 {
                return Err(Error::Transformation("pixel scales must be finite and positive".to_owned()));
            }
            let origin = [
                tie[3] - tie[0] * scale[0],
                tie[4] + tie[1] * scale[1],
                tie[5] - tie[2] * scale.get(2).copied().unwrap_or(0.0),
            ];
            Ok((Orient2::new(origin, [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]), [scale[0], scale[1]]))
        }
        (Some(_), Some(_)) => Err(Error::Transformation("pixel scale or tiepoint tag is too short".to_owned())),
        _ => Err(Error::Transformation("pixel scale and tiepoint tags must occur together".to_owned())),
    }
}

fn transformation_from_omf(orient: Orient2, origin: [f64; 3], size: [f64; 2]) -> Result<[f64; 16], Error> {
    if !size.into_iter().all(|value| value.is_finite() && value > 0.0) {
        return Err(Error::Transformation("OMF cell sizes must be finite and positive".to_owned()));
    }
    if dot(orient.u, orient.v).abs() > 1.0e-10 {
        return Err(Error::Transformation("OMF grid axes must be perpendicular".to_owned()));
    }
    let du = scale3(orient.u, size[0]);
    let dv = scale3(orient.v, size[1]);
    Ok([du[0], dv[0], 0.0, origin[0], du[1], dv[1], 0.0, origin[1], du[2], dv[2], 1.0, origin[2], 0.0, 0.0, 0.0, 1.0])
}

fn find_f64_vec<R: std::io::Read + std::io::Seek>(decoder: &mut Decoder<R>, tag: Tag) -> Result<Option<Vec<f64>>, Error> {
    decoder.find_tag(tag)?.map(|value| value.into_f64_vec()).transpose().map_err(Into::into)
}

fn find_u16_vec<R: std::io::Read + std::io::Seek>(decoder: &mut Decoder<R>, tag: Tag) -> Result<Option<Vec<u16>>, Error> {
    decoder.find_tag(tag)?.map(|value| value.into_u16_vec()).transpose().map_err(Into::into)
}

fn find_ascii<R: std::io::Read + std::io::Seek>(decoder: &mut Decoder<R>, tag: Tag) -> Result<Option<String>, Error> {
    decoder.find_tag(tag)?.map(|value| value.into_string()).transpose().map_err(Into::into)
}

fn epsg_from_geo_keys(keys: Option<&[u16]>) -> String {
    let Some(keys) = keys.filter(|keys| keys.len() >= 4) else {
        return String::new();
    };
    let entries = usize::from(keys[3]);
    let (key_entries, _) = keys[4..].as_chunks::<4>();
    key_entries
        .iter()
        .take(entries)
        .find_map(|entry| {
            let [key, location, count, value] = *entry;
            ((key == 2048 || key == 3072) && location == 0 && count == 1 && (1024..=32766).contains(&value)).then(|| format!("EPSG:{value}"))
        })
        .unwrap_or_default()
}

fn json_u16_vec(value: &Value, name: &str) -> Result<Vec<u16>, Error> {
    value
        .as_array()
        .ok_or_else(|| Error::Metadata(name.to_owned()))?
        .iter()
        .map(|value| value.as_u64().and_then(|v| v.try_into().ok()).ok_or_else(|| Error::Metadata(name.to_owned())))
        .collect()
}

fn json_f64_vec(value: &Value, name: &str) -> Result<Vec<f64>, Error> {
    value
        .as_array()
        .ok_or_else(|| Error::Metadata(name.to_owned()))?
        .iter()
        .map(|value| value.as_f64().ok_or_else(|| Error::Metadata(name.to_owned())))
        .collect()
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(v: [f64; 3], scale: f64) -> [f64; 3] {
    [v[0] * scale, v[1] * scale, v[2] * scale]
}

fn length(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
