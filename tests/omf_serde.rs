use mfsd::{Document, Element, Geometry, OmfError, PointSet, Project, deserialize_omf, serialize_omf};

#[test]
fn deserializes_project_and_geometry_from_bytes() {
    let document = deserialize_omf(include_bytes!("../examples/data/sample.omf")).expect("the example OMF bytes should deserialize");

    assert_eq!(document.format_version(), [2, 0]);
    assert_eq!(document.project().name, "MFSD example mine");
    assert_eq!(document.project().elements.len(), 2);
    assert_eq!(document.project().elements[0].attributes.len(), 2);
    assert_eq!(document.project().elements[0].attributes[0].name, "Hole ID");
    assert!(document.warnings().is_empty());

    let points = match &document.project().elements[0].geometry {
        Geometry::PointSet(points) => points,
        geometry => panic!("expected a point set, found {geometry:?}"),
    };
    let vertices = document
        .reader()
        .array_vertices(&points.vertices)
        .expect("the point array should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("the point array should be valid");

    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0], [100.0, 200.0, 12.0]);
}

#[test]
fn rejects_invalid_omf_bytes() {
    assert!(deserialize_omf(b"not an OMF archive").is_err());
}

#[test]
fn serializes_an_in_memory_document_to_bytes() {
    let document = Document::build(|writer| -> Result<Project, OmfError> {
        let vertices = writer.array_vertices([[1.0_f64, 2.0, 3.0]])?;
        let mut project = Project::new("Serialized by MFSD");
        project.elements.push(Element::new("points", PointSet::new(vertices)));
        Ok(project)
    })
    .expect("the in-memory project should build");

    let bytes = only(serialize_omf(&document));
    let round_trip = deserialize_omf(&bytes).expect("the serialized bytes should deserialize");
    assert_eq!(round_trip.project().name, "Serialized by MFSD");
    assert_eq!(round_trip.project().elements.len(), 1);
}

fn only(mut streams: Vec<Vec<u8>>) -> Vec<u8> {
    assert_eq!(streams.len(), 1);
    streams.remove(0)
}
