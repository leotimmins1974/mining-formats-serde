//! TIFF and GeoTIFF image-texture conversion through the OMF data model.

use std::io::{Cursor, Read, Seek};

use ::image::{DynamicImage, ImageBuffer, Luma, LumaA, Rgb, Rgba};
use ::tiff::{
    ColorType,
    decoder::{Decoder, DecodingResult},
    encoder::{
        TiffEncoder,
        colortype::{Gray8, Gray16, RGB8, RGB16, RGBA8, RGBA16},
    },
    tags::Tag,
};
use serde_json::{Map, Value, json};

use crate::{
    Attribute, AttributeData, Element, Grid2, GridSurface, Orient2, Project,
    format::{
        ByteStreams, all_elements,
        omf::{Document, MemoryWriter},
    },
};

const GEO_KEY_DIRECTORY: &str = "mfsd:tiff:geo_key_directory";
const GEO_DOUBLE_PARAMS: &str = "mfsd:tiff:geo_double_params";
const GEO_ASCII_PARAMS: &str = "mfsd:tiff:geo_ascii_params";
const GDAL_NODATA: &str = "mfsd:tiff:gdal_nodata";

/// A TIFF conversion failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write TIFF: {0}")]
    Tiff(#[from] ::tiff::TiffError),
    #[error("could not read or write OMF: {0}")]
    Omf(#[from] ::omf::error::Error),
    #[error("TIFF color type {0:?} is not supported as an OMF texture; expected 8-bit or 16-bit grayscale, grayscale-alpha, RGB, or RGBA")]
    ColorType(ColorType),
    #[error("TIFF pixel storage does not match its declared color type {0:?}")]
    PixelData(ColorType),
    #[error("the GeoTIFF transformation is not a valid orthogonal OMF grid: {0}")]
    Transformation(String),
    #[error("the OMF document contains no image texture that TIFF can serialize")]
    NothingToSerialize,
    #[error("OMF GeoTIFF metadata field {0:?} has the wrong type")]
    Metadata(String),
}

/// Deserialize TIFF images as OMF projected textures on flat grid surfaces.
///
/// Each TIFF page becomes a separate element. Common 8-bit and 16-bit
/// grayscale, grayscale-alpha, RGB, and RGBA textures are supported.
pub fn deserialize(bytes: &[u8]) -> Result<Document, Error> {
    Document::build(|writer| read_project(bytes, writer))
}

fn read_project(bytes: &[u8], writer: &mut MemoryWriter) -> Result<Project, Error> {
    let mut decoder = Decoder::new(Cursor::new(bytes))?;
    let mut project = Project::new("TIFF project");
    project.application = format!("mfsd {}", env!("CARGO_PKG_VERSION"));

    let mut page = 1;
    loop {
        let (element, coordinate_reference_system) = read_page(&mut decoder, writer, page)?;
        if project.coordinate_reference_system.is_empty() {
            project.coordinate_reference_system = coordinate_reference_system;
        }
        project.elements.push(element);
        if !decoder.more_images() {
            break;
        }
        decoder.next_image()?;
        page += 1;
    }
    Ok(project)
}

fn read_page<R: Read + Seek>(decoder: &mut Decoder<R>, writer: &mut MemoryWriter, page: usize) -> Result<(Element, String), Error> {
    let (width, height) = decoder.dimensions()?;
    let color_type = decoder.colortype()?;
    let pixel_scale = find_f64_vec(decoder, Tag::ModelPixelScaleTag)?;
    let tiepoint = find_f64_vec(decoder, Tag::ModelTiepointTag)?;
    let transformation = find_f64_vec(decoder, Tag::ModelTransformationTag)?;
    let geo_keys = find_u16_vec(decoder, Tag::GeoKeyDirectoryTag)?;
    let geo_doubles = find_f64_vec(decoder, Tag::GeoDoubleParamsTag)?;
    let geo_ascii = find_ascii(decoder, Tag::GeoAsciiParamsTag)?;
    let nodata = find_ascii(decoder, Tag::GdalNodata)?;

    let (orient, pixel_size) = georeference(pixel_scale.as_deref(), tiepoint.as_deref(), transformation.as_deref())?;
    let texture = decode_texture(width, height, color_type, decoder.read_image()?)?;
    let image = writer.image_png(&texture)?;
    let extent = [pixel_size[0] * f64::from(width), pixel_size[1] * f64::from(height)];
    let surface = GridSurface::new(orient, Grid2::from_size_and_count(extent, [1, 1]), None);
    let mut texture = Attribute::from_texture_project("TIFF texture", image, orient, extent[0], extent[1]);
    if let Some(values) = &geo_keys {
        texture.metadata.insert(GEO_KEY_DIRECTORY.to_owned(), json!(values));
    }
    if let Some(values) = geo_doubles {
        texture.metadata.insert(GEO_DOUBLE_PARAMS.to_owned(), json!(values));
    }
    if let Some(value) = geo_ascii {
        texture.metadata.insert(GEO_ASCII_PARAMS.to_owned(), Value::String(value));
    }
    if let Some(value) = nodata {
        texture.metadata.insert(GDAL_NODATA.to_owned(), Value::String(value));
    }

    let name = if page == 1 { "TIFF texture".to_owned() } else { format!("TIFF texture {page}") };
    let mut element = Element::new(name, surface);
    element.attributes.push(texture);
    Ok((element, epsg_from_geo_keys(geo_keys.as_deref())))
}

fn decode_texture(width: u32, height: u32, color_type: ColorType, pixels: DecodingResult) -> Result<DynamicImage, Error> {
    macro_rules! image {
        ($pixel:ty, $values:expr, $variant:ident) => {
            ImageBuffer::<$pixel, _>::from_raw(width, height, $values)
                .map(DynamicImage::$variant)
                .ok_or(Error::PixelData(color_type))
        };
    }

    match (color_type, pixels) {
        (ColorType::Gray(8), DecodingResult::U8(values)) => image!(Luma<u8>, values, ImageLuma8),
        (ColorType::Gray(16), DecodingResult::U16(values)) => image!(Luma<u16>, values, ImageLuma16),
        (ColorType::GrayA(8), DecodingResult::U8(values)) => image!(LumaA<u8>, values, ImageLumaA8),
        (ColorType::GrayA(16), DecodingResult::U16(values)) => image!(LumaA<u16>, values, ImageLumaA16),
        (ColorType::RGB(8), DecodingResult::U8(values)) => image!(Rgb<u8>, values, ImageRgb8),
        (ColorType::RGB(16), DecodingResult::U16(values)) => image!(Rgb<u16>, values, ImageRgb16),
        (ColorType::RGBA(8), DecodingResult::U8(values)) => image!(Rgba<u8>, values, ImageRgba8),
        (ColorType::RGBA(16), DecodingResult::U16(values)) => image!(Rgba<u16>, values, ImageRgba16),
        (ColorType::Gray(8 | 16) | ColorType::GrayA(8 | 16) | ColorType::RGB(8 | 16) | ColorType::RGBA(8 | 16), _) => Err(Error::PixelData(color_type)),
        _ => Err(Error::ColorType(color_type)),
    }
}

/// Serialize every OMF image texture into an independently usable TIFF stream.
///
/// Projected textures retain their spatial placement as GeoTIFF transforms.
/// Mapped textures are written without georeferencing. Other attributes and
/// geometry are omitted.
pub fn serialize(document: &Document) -> Result<ByteStreams, Error> {
    let mut streams = Vec::new();
    for element in all_elements(&document.project().elements) {
        for attribute in &element.attributes {
            let (image, placement) = match &attribute.data {
                AttributeData::ProjectedTexture { image, orient, width, height } => (image, Some((*orient, *width, *height))),
                AttributeData::MappedTexture { image, .. } => (image, None),
                _ => continue,
            };
            let image = document.reader().image(image)?;
            let transformation = placement
                .map(|(orient, width, height)| {
                    let pixel_size = [width / f64::from(image.width()), height / f64::from(image.height())];
                    transformation_from_omf(orient, add3(orient.origin, document.project().origin), pixel_size)
                })
                .transpose()?;
            streams.push(encode_texture(image, transformation.as_ref(), &attribute.metadata, &document.project().metadata)?);
        }
    }
    if streams.is_empty() {
        return Err(Error::NothingToSerialize);
    }
    Ok(streams)
}

enum TexturePixels {
    Gray8(Vec<u8>),
    Gray16(Vec<u16>),
    Rgb8(Vec<u8>),
    Rgb16(Vec<u16>),
    Rgba8(Vec<u8>),
    Rgba16(Vec<u16>),
}

fn texture_pixels(image: DynamicImage) -> TexturePixels {
    match image {
        DynamicImage::ImageLuma8(image) => TexturePixels::Gray8(image.into_raw()),
        DynamicImage::ImageLuma16(image) => TexturePixels::Gray16(image.into_raw()),
        DynamicImage::ImageRgb8(image) => TexturePixels::Rgb8(image.into_raw()),
        DynamicImage::ImageRgb16(image) => TexturePixels::Rgb16(image.into_raw()),
        DynamicImage::ImageRgba8(image) => TexturePixels::Rgba8(image.into_raw()),
        DynamicImage::ImageRgba16(image) => TexturePixels::Rgba16(image.into_raw()),
        image @ DynamicImage::ImageLumaA8(_) => TexturePixels::Rgba8(image.to_rgba8().into_raw()),
        image @ DynamicImage::ImageLumaA16(_) => TexturePixels::Rgba16(image.to_rgba16().into_raw()),
        image => TexturePixels::Rgba8(image.to_rgba8().into_raw()),
    }
}

fn encode_texture(image: DynamicImage, transformation: Option<&[f64; 16]>, metadata: &Map<String, Value>, project_metadata: &Map<String, Value>) -> Result<Vec<u8>, Error> {
    let (width, height) = (image.width(), image.height());
    let pixels = texture_pixels(image);
    let mut bytes = Vec::new();
    {
        let mut encoder = TiffEncoder::new(Cursor::new(&mut bytes))?;

        macro_rules! write_image {
            ($color:ty, $values:expr) => {{
                let mut image = encoder.new_image::<$color>(width, height)?;
                if let Some(transformation) = transformation {
                    image.encoder().write_tag(Tag::ModelTransformationTag, &transformation[..])?;
                }
                if let Some(value) = metadata_value(metadata, project_metadata, GEO_KEY_DIRECTORY) {
                    let values = json_u16_vec(value, GEO_KEY_DIRECTORY)?;
                    image.encoder().write_tag(Tag::GeoKeyDirectoryTag, &values[..])?;
                }
                if let Some(value) = metadata_value(metadata, project_metadata, GEO_DOUBLE_PARAMS) {
                    let values = json_f64_vec(value, GEO_DOUBLE_PARAMS)?;
                    image.encoder().write_tag(Tag::GeoDoubleParamsTag, &values[..])?;
                }
                if let Some(value) = metadata_value(metadata, project_metadata, GEO_ASCII_PARAMS) {
                    let value = value.as_str().ok_or_else(|| Error::Metadata(GEO_ASCII_PARAMS.to_owned()))?;
                    image.encoder().write_tag(Tag::GeoAsciiParamsTag, value)?;
                }
                if let Some(value) = metadata_value(metadata, project_metadata, GDAL_NODATA) {
                    let value = value.as_str().ok_or_else(|| Error::Metadata(GDAL_NODATA.to_owned()))?;
                    image.encoder().write_tag(Tag::GdalNodata, value)?;
                }
                image.write_data($values)?;
            }};
        }

        match &pixels {
            TexturePixels::Gray8(values) => write_image!(Gray8, values),
            TexturePixels::Gray16(values) => write_image!(Gray16, values),
            TexturePixels::Rgb8(values) => write_image!(RGB8, values),
            TexturePixels::Rgb16(values) => write_image!(RGB16, values),
            TexturePixels::Rgba8(values) => write_image!(RGBA8, values),
            TexturePixels::Rgba16(values) => write_image!(RGBA16, values),
        }
    }
    Ok(bytes)
}

fn metadata_value<'a>(metadata: &'a Map<String, Value>, project_metadata: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    metadata.get(name).or_else(|| project_metadata.get(name))
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

#[cfg(test)]
mod tests {
    use ::tiff::encoder::colortype::{Gray8, Gray32, RGB8, RGBA16};

    use super::*;

    const RGB_PIXELS: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    const TRANSFORMATION: [f64; 16] = [2.0, 0.0, 0.0, 100.0, 0.0, -3.0, 0.0, 200.0, 0.0, 0.0, 1.0, 5.0, 0.0, 0.0, 0.0, 1.0];
    const GEO_KEYS: [u16; 8] = [1, 1, 0, 1, 3072, 0, 1, 28350];

    fn rgb_geotiff() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = TiffEncoder::new(Cursor::new(&mut bytes)).unwrap();
            let mut image = encoder.new_image::<RGB8>(2, 2).unwrap();
            image.encoder().write_tag(Tag::ModelTransformationTag, &TRANSFORMATION[..]).unwrap();
            image.encoder().write_tag(Tag::GeoKeyDirectoryTag, &GEO_KEYS[..]).unwrap();
            image.write_data(&RGB_PIXELS).unwrap();
        }
        bytes
    }

    #[test]
    fn rgb_geotiff_round_trip_uses_projected_texture() {
        let document = deserialize(&rgb_geotiff()).unwrap();
        assert_eq!(document.project().coordinate_reference_system, "EPSG:28350");
        assert_eq!(document.project().elements.len(), 1);

        let element = &document.project().elements[0];
        let crate::Geometry::GridSurface(surface) = &element.geometry else {
            panic!("TIFF should create a flat grid surface");
        };
        assert!(surface.heights.is_none());
        assert_eq!(surface.orient.origin, [100.0, 200.0, 5.0]);
        assert_eq!(surface.orient.u, [1.0, 0.0, 0.0]);
        assert_eq!(surface.orient.v, [0.0, -1.0, 0.0]);
        assert_eq!(surface.grid, Grid2::from_size_and_count([4.0, 6.0], [1, 1]));

        let AttributeData::ProjectedTexture { image, width, height, .. } = &element.attributes[0].data else {
            panic!("TIFF should create a projected texture");
        };
        assert_eq!((*width, *height), (4.0, 6.0));
        assert_eq!(document.reader().image(image).unwrap().to_rgb8().into_raw(), RGB_PIXELS);

        let streams = serialize(&document).unwrap();
        assert_eq!(streams.len(), 1);
        let mut decoder = Decoder::new(Cursor::new(&streams[0])).unwrap();
        assert_eq!(decoder.dimensions().unwrap(), (2, 2));
        assert_eq!(decoder.colortype().unwrap(), ColorType::RGB(8));
        assert_eq!(find_f64_vec(&mut decoder, Tag::ModelTransformationTag).unwrap().unwrap(), TRANSFORMATION);
        assert_eq!(find_u16_vec(&mut decoder, Tag::GeoKeyDirectoryTag).unwrap().unwrap(), GEO_KEYS);
        let DecodingResult::U8(pixels) = decoder.read_image().unwrap() else {
            panic!("round-tripped RGB texture should contain u8 samples");
        };
        assert_eq!(pixels, RGB_PIXELS);
    }

    #[test]
    fn multipage_tiff_creates_one_texture_element_per_page() {
        let mut bytes = Vec::new();
        {
            let mut encoder = TiffEncoder::new(Cursor::new(&mut bytes)).unwrap();
            encoder.new_image::<Gray8>(1, 1).unwrap().write_data(&[42]).unwrap();
            encoder.new_image::<RGBA16>(1, 1).unwrap().write_data(&[u16::MAX, 100, 200, u16::MAX]).unwrap();
        }

        let document = deserialize(&bytes).unwrap();
        assert_eq!(document.project().elements.len(), 2);
        assert_eq!(document.project().elements[0].name, "TIFF texture");
        assert_eq!(document.project().elements[1].name, "TIFF texture 2");
        assert_eq!(serialize(&document).unwrap().len(), 2);
    }

    #[test]
    fn height_grid_without_texture_is_not_serialized() {
        let document = Document::build(|writer| {
            let heights = writer.array_scalars([0.0, 1.0, 2.0, 3.0])?;
            let surface = GridSurface::new(Orient2::default(), Grid2::from_size_and_count([1.0, 1.0], [1, 1]), Some(heights));
            let mut project = Project::new("height grid");
            project.elements.push(Element::new("heights", surface));
            Ok::<_, ::omf::error::Error>(project)
        })
        .unwrap();

        assert!(matches!(serialize(&document), Err(Error::NothingToSerialize)));
    }

    #[test]
    fn mapped_texture_serializes_without_georeferencing() {
        let document = Document::build(|writer| {
            let pixels = DynamicImage::ImageRgb8(ImageBuffer::from_raw(2, 2, RGB_PIXELS.to_vec()).unwrap());
            let image = writer.image_png(&pixels)?;
            let texcoords = writer.array_texcoords([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]])?;
            let surface = GridSurface::new(Orient2::default(), Grid2::from_size_and_count([1.0, 1.0], [1, 1]), None);
            let mut element = Element::new("mapped texture", surface);
            element.attributes.push(Attribute::from_texture_map("texture", image, crate::Location::Vertices, texcoords));
            let mut project = Project::new("mapped texture");
            project.elements.push(element);
            Ok::<_, ::omf::error::Error>(project)
        })
        .unwrap();

        let streams = serialize(&document).unwrap();
        let mut decoder = Decoder::new(Cursor::new(&streams[0])).unwrap();
        assert_eq!(decoder.colortype().unwrap(), ColorType::RGB(8));
        assert!(find_f64_vec(&mut decoder, Tag::ModelTransformationTag).unwrap().is_none());
    }

    #[test]
    fn rejects_non_texture_sample_types() {
        let mut bytes = Vec::new();
        {
            let mut encoder = TiffEncoder::new(Cursor::new(&mut bytes)).unwrap();
            encoder.new_image::<Gray32>(1, 1).unwrap().write_data(&[7]).unwrap();
        }
        assert!(matches!(deserialize(&bytes), Err(Error::ColorType(ColorType::Gray(32)))));
    }
}
