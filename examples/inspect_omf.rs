use std::{env, error::Error, fs, path::PathBuf};

use mfsd::{Geometry, deserialize_omf};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/data/sample.omf"));
    let bytes = fs::read(&path)?;
    let document = deserialize_omf(&bytes)?;
    let project = document.project();
    let [major, minor] = document.format_version();

    println!("File: {}", path.display());
    println!("OMF version: {major}.{minor}");
    println!("Project: {}", project.name);
    println!("Description: {}", project.description);
    println!("Coordinate reference system: {}", project.coordinate_reference_system);
    println!("Units: {}", project.units);
    println!("Elements: {}", project.elements.len());

    for element in &project.elements {
        print!("- {}: ", element.name);
        match &element.geometry {
            Geometry::PointSet(points) => {
                println!("point set ({} points)", points.vertices.item_count());
                let first = document.reader().array_vertices(&points.vertices)?.next();
                if let Some(point) = first.transpose()? {
                    println!("  first point: {point:?}");
                }
            }
            Geometry::LineSet(lines) => println!("line set ({} vertices, {} segments)", lines.vertices.item_count(), lines.segments.item_count()),
            Geometry::Surface(surface) => println!("surface ({} vertices, {} triangles)", surface.vertices.item_count(), surface.triangles.item_count()),
            Geometry::GridSurface(_) => println!("grid surface"),
            Geometry::BlockModel(_) => println!("block model"),
            Geometry::Composite(composite) => {
                println!("composite ({} elements)", composite.elements.len())
            }
        }
        println!("  attributes: {}", element.attributes.len());
        for attribute in &element.attributes {
            println!("  - {} at {:?} ({} values)", attribute.name, attribute.location, attribute.data.len());
        }
    }

    if !document.warnings().is_empty() {
        eprintln!("{}", document.warnings());
    }

    Ok(())
}
