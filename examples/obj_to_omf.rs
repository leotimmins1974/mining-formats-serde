//! Reads OBJ into the shared OMF document, serializes it, then writes OBJ again.

use std::{error::Error, fs, path::PathBuf};

use mfsd::format::{obj, omf};

fn main() -> Result<(), Box<dyn Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("data").join("obj_to_omf");
    fs::create_dir_all(&directory)?;

    let source_path = directory.join("source.obj");
    let omf_path = directory.join("model.omf");
    let roundtrip_path = directory.join("roundtrip.obj");

    // The application owns file I/O and gives raw bytes to MFSD.
    let source_bytes = fs::read(&source_path)?;

    // The OBJ adapter produces the common in-memory OMF representation.
    let document = obj::deserialize(&source_bytes)?;
    println!("Read {} OMF elements from {}", document.project().elements.len(), source_path.display());

    // OMF serialization is one possible destination for that representation.
    let omf_bytes = omf::serialize(&document).pop().expect("OMF always serializes to one stream");
    fs::write(&omf_path, &omf_bytes)?;
    println!("Wrote {}", omf_path.display());

    // Prove the serialized OMF is sufficient by opening it as a new document.
    let reopened = omf::deserialize(&omf_bytes)?;

    // The OBJ writer only consumes OMF; it has no dependency on the OBJ reader.
    let roundtrip_bytes = obj::serialize(&reopened)?.pop().expect("OBJ always serializes to one stream");
    fs::write(&roundtrip_path, roundtrip_bytes)?;
    println!("Wrote {}", roundtrip_path.display());

    Ok(())
}
