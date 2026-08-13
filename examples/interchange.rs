//! Shows an application whose core only knows about the in-memory OMF document.

use std::{
    env,
    error::Error,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use mfsd::{
    Document, Geometry,
    format::{las, laz, obj, omf, tiff},
};

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let (Some(input), Some(output), None) = (arguments.next(), arguments.next(), arguments.next()) else {
        eprintln!("usage: cargo run --example interchange -- INPUT OUTPUT");
        return ExitCode::FAILURE;
    };

    match run(Path::new(&input), Path::new(&output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    // Format-specific code exists only at the application boundary.
    let document = deserialize(input)?;

    // The application itself handles only OMF.
    inspect(&document);

    // A different boundary adapter can consume the exact same document.
    serialize(output, &document)
}

fn inspect(document: &Document) {
    println!("Project: {}", document.project().name);
    for element in &document.project().elements {
        let geometry = match &element.geometry {
            Geometry::PointSet(_) => "point set",
            Geometry::LineSet(_) => "line set",
            Geometry::Surface(_) => "surface",
            Geometry::GridSurface(_) => "grid surface",
            Geometry::BlockModel(_) => "block model",
            Geometry::Composite(_) => "composite",
        };
        println!("- {}: {geometry}", element.name);
    }
}

fn deserialize(path: &Path) -> Result<Document, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    Ok(match extension(path)?.as_str() {
        "omf" => omf::deserialize(&bytes)?,
        "obj" => obj::deserialize(&bytes)?,
        "las" => las::deserialize(&bytes)?,
        "laz" => laz::deserialize(&bytes)?,
        "tif" | "tiff" => tiff::deserialize(&bytes)?,
        extension => return Err(unsupported(extension).into()),
    })
}

fn serialize(path: &Path, document: &Document) -> Result<(), Box<dyn Error>> {
    let streams = match extension(path)?.as_str() {
        "omf" => omf::serialize(document),
        "obj" => obj::serialize(document)?,
        "las" => las::serialize(document)?,
        "laz" => laz::serialize(document)?,
        "tif" | "tiff" => tiff::serialize(document)?,
        extension => return Err(unsupported(extension).into()),
    };
    for (index, bytes) in streams.into_iter().enumerate() {
        let output = if index == 0 { path.to_owned() } else { numbered_path(path, index + 1) };
        fs::write(&output, bytes)?;
        println!("Wrote {}", output.display());
    }
    Ok(())
}

fn numbered_path(path: &Path, number: usize) -> PathBuf {
    let mut name = path.file_stem().map_or_else(OsString::new, OsString::from);
    name.push(format!("-{number}"));
    if let Some(extension) = path.extension() {
        name.push(".");
        name.push(extension);
    }
    path.with_file_name(name)
}

fn extension(path: &Path) -> Result<String, io::Error> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| unsupported("missing or non-Unicode"))
}

fn unsupported(extension: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("unsupported file extension {extension:?}"))
}
