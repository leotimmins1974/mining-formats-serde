//! Regenerates `examples/data/sample.omf`.

use std::{error::Error, fs, path::PathBuf};

use mfsd::{Attribute, Document, Element, Location, OmfError, PointSet, Project, Surface, serialize_omf};

fn main() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/data/sample.omf");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let document = Document::build(|writer| -> Result<Project, OmfError> {
        let mut collar_locations = Element::new(
            "Collar locations",
            PointSet::new(writer.array_vertices([[100.0_f64, 200.0, 12.0], [125.0, 210.0, 11.5], [145.0, 235.0, 13.0]])?),
        );
        collar_locations.attributes.push(Attribute::from_strings(
            "Hole ID",
            Location::Vertices,
            writer.array_text(["DH-001", "DH-002", "DH-003"].map(|value| Some(value.to_owned())))?,
        ));
        collar_locations.attributes.push(Attribute::from_numbers(
            "Collar elevation",
            Location::Vertices,
            writer.array_numbers([Some(12.0_f64), Some(11.5), Some(13.0)])?,
        ));

        let pit_floor = Element::new(
            "Pit floor",
            Surface::new(
                writer.array_vertices([[90.0_f64, 190.0, 0.0], [160.0, 190.0, 0.0], [160.0, 250.0, 0.0], [90.0, 250.0, 0.0]])?,
                writer.array_triangles([[0_u32, 1, 2], [0, 2, 3]])?,
            ),
        );

        let mut project = Project::new("MFSD example mine");
        project.date = omf::date_time::i64_to_date_time(0);
        project.description = "A small OMF project used by the MFSD reader example and tests.".to_owned();
        project.coordinate_reference_system = "EPSG:28350".to_owned();
        project.units = "metres".to_owned();
        project.author = "MFSD contributors".to_owned();
        project.application = "mfsd generate_sample example".to_owned();
        project.elements = vec![collar_locations, pit_floor];

        Ok(project)
    })?;
    if !document.warnings().is_empty() {
        eprintln!("{}", document.warnings());
    }
    let bytes = serialize_omf(&document).pop().expect("OMF always serializes to one stream");
    fs::write(&path, bytes)?;
    println!("Wrote {}", path.display());

    Ok(())
}
