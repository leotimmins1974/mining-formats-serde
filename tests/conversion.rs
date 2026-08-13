use std::io::Cursor;

use mfsd::{
    Attribute, Document, Element, Geometry, Grid2, GridSurface, Location, OmfError, Orient2, PointSet, Project, Surface,
    format::{las as las_format, laz as laz_format, obj, omf, tiff as tiff_format},
};

#[test]
fn obj_round_trip_uses_omf_surface_and_vertex_attributes() {
    let document = obj::deserialize(include_bytes!("../examples/data/triangle.obj")).unwrap();
    let [element] = document.project().elements.as_slice() else {
        panic!("expected one element");
    };
    assert!(matches!(element.geometry, Geometry::Surface(_)));
    assert_eq!(
        element.attributes.iter().map(|attribute| attribute.name.as_str()).collect::<Vec<_>>(),
        ["obj:normal", "obj:texcoord", "obj:color"]
    );

    let omf_bytes = only(omf::serialize(&document));
    let reopened = omf::deserialize(&omf_bytes).unwrap();
    let obj_bytes = only(obj::serialize(&reopened).unwrap());
    let round_trip = obj::deserialize(&obj_bytes).unwrap();
    assert_eq!(round_trip.project().elements.len(), 1);
}

#[test]
fn obj_points_and_polylines_map_to_their_omf_geometries() {
    let document = obj::deserialize(include_bytes!("../examples/data/primitives.obj")).unwrap();
    assert_eq!(document.project().elements.len(), 2);
    assert!(matches!(document.project().elements[0].geometry, Geometry::PointSet(_)));
    assert!(matches!(document.project().elements[1].geometry, Geometry::LineSet(_)));

    let obj_bytes = only(obj::serialize(&document).unwrap());
    let round_trip = obj::deserialize(&obj_bytes).unwrap();
    assert_eq!(round_trip.project().elements.len(), 2);
}

#[test]
fn provided_obj_to_omf_roundtrip_files_are_valid() {
    let source = obj::deserialize(include_bytes!("../examples/data/obj_to_omf/source.obj")).unwrap();
    let serialized = omf::deserialize(include_bytes!("../examples/data/obj_to_omf/model.omf")).unwrap();
    let roundtrip = obj::deserialize(include_bytes!("../examples/data/obj_to_omf/roundtrip.obj")).unwrap();

    for document in [&source, &serialized, &roundtrip] {
        assert_eq!(document.project().elements.len(), 3);
        assert!(matches!(document.project().elements[0].geometry, Geometry::PointSet(_)));
        assert!(matches!(document.project().elements[1].geometry, Geometry::LineSet(_)));
        assert!(matches!(document.project().elements[2].geometry, Geometry::Surface(_)));
    }
}

#[test]
fn las_and_laz_round_trip_standard_point_fields() {
    let las_bytes = make_las();
    let document = las_format::deserialize(&las_bytes).unwrap();
    let [element] = document.project().elements.as_slice() else {
        panic!("expected one element");
    };
    assert!(matches!(element.geometry, Geometry::PointSet(_)));
    assert_eq!(element.attributes.len(), 18);

    let laz_bytes = only(laz_format::serialize(&document).unwrap());
    let reopened = laz_format::deserialize(&laz_bytes).unwrap();
    assert_eq!(reopened.project().elements.len(), 1);
    let mut reader = las::Reader::new(Cursor::new(laz_bytes)).unwrap();
    let points = reader.read_all().unwrap().points().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].classification, las::point::Classification::Ground);
    assert_eq!(points[0].gps_time, Some(1234.5));
    assert_eq!(points[0].color, Some(las::Color::new(100, 200, 300)));
}

#[test]
fn las_and_laz_modules_reject_the_other_encoding() {
    let las_bytes = make_las();
    assert!(matches!(
        laz_format::deserialize(&las_bytes),
        Err(laz_format::Error::UnexpectedEncoding { expected: "LAZ", actual: "LAS" })
    ));

    let document = las_format::deserialize(&las_bytes).unwrap();
    let laz_bytes = only(laz_format::serialize(&document).unwrap());
    assert!(matches!(
        las_format::deserialize(&laz_bytes),
        Err(las_format::Error::UnexpectedEncoding { expected: "LAS", actual: "LAZ" })
    ));
}

#[test]
fn geotiff_round_trip_preserves_grid_and_geo_keys() {
    let tiff_bytes = make_tiff();
    let document = tiff_format::deserialize(&tiff_bytes).unwrap();
    assert_eq!(document.project().coordinate_reference_system, "EPSG:28350");
    let Geometry::GridSurface(surface) = &document.project().elements[0].geometry else {
        panic!("expected a grid surface");
    };
    assert_eq!(surface.grid.count(), [2, 1]);
    assert_eq!(surface.orient.origin, [500_000.0, 6_500_000.0, 0.0]);
    assert_eq!(surface.orient.v, [0.0, -1.0, 0.0]);

    let round_trip_bytes = only(tiff_format::serialize(&document).unwrap());
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(round_trip_bytes)).unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (3, 2));
    assert_eq!(decoder.get_tag_u16_vec(tiff::tags::Tag::GeoKeyDirectoryTag).unwrap(), [1, 1, 0, 1, 3072, 0, 1, 28350]);
    let tiff::decoder::DecodingResult::F64(values) = decoder.read_image().unwrap() else {
        panic!("expected 64-bit float samples");
    };
    assert_eq!(values, [10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);
}

#[test]
fn serializers_silently_omit_unrepresentable_elements_and_attributes() {
    let document = make_mixed_document();

    let obj_streams = obj::serialize(&document).unwrap();
    assert_eq!(obj_streams.len(), 1);
    let obj_bytes = only(obj_streams);
    let obj_text = std::str::from_utf8(&obj_bytes).unwrap();
    assert!(!obj_text.contains("ignored_grid"));
    assert!(obj_text.contains("o mesh"));
    assert!(obj_text.contains("o points"));

    let las_streams = las_format::serialize(&document).unwrap();
    assert_eq!(las_streams.len(), 2);
    let mut first_las = las::Reader::new(Cursor::new(las_streams[0].clone())).unwrap();
    let mut second_las = las::Reader::new(Cursor::new(las_streams[1].clone())).unwrap();
    assert_eq!(first_las.read_all().unwrap().len(), 2);
    assert_eq!(second_las.read_all().unwrap().len(), 1);

    let tiff_streams = tiff_format::serialize(&document).unwrap();
    assert_eq!(tiff_streams.len(), 2);
    let mut first_tiff = tiff::decoder::Decoder::new(Cursor::new(&tiff_streams[0])).unwrap();
    let mut second_tiff = tiff::decoder::Decoder::new(Cursor::new(&tiff_streams[1])).unwrap();
    assert_eq!(first_tiff.dimensions().unwrap(), (2, 2));
    assert_eq!(second_tiff.dimensions().unwrap(), (3, 2));
}

#[test]
fn serializers_error_when_no_element_is_representable() {
    let grid_document = tiff_format::deserialize(&make_tiff()).unwrap();
    assert!(matches!(obj::serialize(&grid_document), Err(obj::Error::NothingToSerialize)));
    assert!(matches!(las_format::serialize(&grid_document), Err(las_format::Error::NothingToSerialize)));

    let mesh_document = obj::deserialize(include_bytes!("../examples/data/triangle.obj")).unwrap();
    assert!(matches!(tiff_format::serialize(&mesh_document), Err(tiff_format::Error::NothingToSerialize)));
}

fn make_mixed_document() -> Document {
    Document::build(|writer| -> Result<Project, OmfError> {
        let heights = writer.array_scalars([1.0, 2.0, 3.0, 4.0])?;
        let grid = GridSurface::new(Orient2::default(), Grid2::from_size_and_count([1.0, 1.0], [1, 1]), Some(heights));
        let second_heights = writer.array_scalars([5.0, 6.0, 7.0, 8.0, 9.0, 10.0])?;
        let second_grid = GridSurface::new(Orient2::default(), Grid2::from_size_and_count([1.0, 1.0], [2, 1]), Some(second_heights));

        let surface_vertices = writer.array_vertices([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])?;
        let triangles = writer.array_triangles([[0, 1, 2]])?;
        let mut mesh = Element::new("mesh", Surface::new(surface_vertices, triangles));
        mesh.attributes.push(Attribute::from_numbers(
            "not-an-obj-field",
            Location::Vertices,
            writer.array_numbers([Some(1_i64), Some(2), Some(3)])?,
        ));

        let point_vertices = writer.array_vertices([[10.0, 20.0, 30.0], [11.0, 21.0, 31.0]])?;
        let mut points = Element::new("points", PointSet::new(point_vertices));
        points.attributes.push(Attribute::from_numbers(
            "not-a-las-field",
            Location::Vertices,
            writer.array_numbers([Some(5_i64), Some(6)])?,
        ));

        let second_point_vertices = writer.array_vertices([[40.0, 50.0, 60.0]])?;
        let second_points = Element::new("points_2", PointSet::new(second_point_vertices));

        let mut project = Project::new("mixed geometries");
        project.elements = vec![Element::new("ignored_grid", grid), mesh, points, Element::new("ignored_grid_2", second_grid), second_points];
        Ok(project)
    })
    .unwrap()
}

fn only(mut streams: Vec<Vec<u8>>) -> Vec<u8> {
    assert_eq!(streams.len(), 1);
    streams.remove(0)
}

fn make_las() -> Vec<u8> {
    let mut builder = las::Builder::from(las::Version::new(1, 4));
    builder.point_format = las::point::Format::new(3).unwrap();
    let header = builder.into_header().unwrap();
    let mut writer = las::Writer::new(Cursor::new(Vec::new()), header).unwrap();

    for (i, coordinates) in [[100.0, 200.0, 10.0], [101.0, 201.0, 11.0]].into_iter().enumerate() {
        let point = las::Point {
            x: coordinates[0],
            y: coordinates[1],
            z: coordinates[2],
            intensity: 42 + i as u16,
            return_number: 1,
            number_of_returns: 1,
            classification: las::point::Classification::Ground,
            gps_time: Some(1234.5 + i as f64),
            color: Some(las::Color::new(100 + i as u16, 200, 300)),
            ..Default::default()
        };
        writer.write_point(point).unwrap();
    }
    writer.close().unwrap();
    writer.into_inner().unwrap().into_inner()
}

fn make_tiff() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(Cursor::new(&mut bytes)).unwrap();
        let mut image = encoder.new_image::<tiff::encoder::colortype::Gray32Float>(3, 2).unwrap();
        image.encoder().write_tag(tiff::tags::Tag::ModelPixelScaleTag, &[2.0, 3.0, 0.0][..]).unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::ModelTiepointTag, &[0.0, 0.0, 0.0, 500_000.0, 6_500_000.0, 0.0][..])
            .unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::GeoKeyDirectoryTag, &[1_u16, 1, 0, 1, 3072, 0, 1, 28350][..])
            .unwrap();
        image.write_data(&[10.0_f32, 11.0, 12.0, 13.0, 14.0, 15.0]).unwrap();
    }
    bytes
}
