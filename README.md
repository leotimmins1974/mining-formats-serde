# Mining Formats Serialise Deserialise

MFSD is a Rust library for serialising and deserialising mining and geospatial
file formats through one common representation based on OMF 2.

## How it works

Every format deserializer accepts raw bytes and produces the same in-memory
`Document`. That document contains the OMF project, elements, geometries,
attributes, typed arrays, and images. Application code can inspect and process
it without knowing which file format it originally came from.

Serializers return `ByteStreams`, a `Vec<Vec<u8>>` containing independently
usable destination streams. A container format returns one stream when it can
natively hold every compatible element; a single-entity format returns one
stream per compatible OMF element.

Serialization is best-effort: unsupported elements and fields are silently
omitted. An error is returned when the document contains no compatible element,
or when compatible data is malformed and cannot be encoded. Stream order
follows OMF element order, including elements nested in composites.

**Example Rust Implementation**

```rust
use mfsd::format::{obj, omf};

fn obj_to_omf_bytes(input: &[u8]) -> Result<Vec<Vec<u8>>, obj::Error> {
    // Deserialize raw OBJ bytes into the in-memory OMF document.
    let document = obj::deserialize(input)?;

    // Application logic works only with the OMF representation.
    println!("{} elements", document.project().elements.len());

    // OMF is a container, so this vector contains exactly one byte stream.
    Ok(omf::serialize(&document))
}
```

**Example C# Implementation**

```csharp
using Mfsd;

static byte[][] ObjToOmfBytes(byte[] input)
{
    // Deserialize raw OBJ bytes into the in-memory OMF document.
    using var document = Document.Deserialize(Format.Obj, input);

    // Application logic works only with the OMF representation.
    Console.WriteLine($"{document.Elements.Count} elements");

    // OMF is a container, so the result contains one byte array.
    return document.Serialize(Format.Omf);
}
```

## We'd like our proprietary format supported

To request support for a proprietary file format, please contact `leo@inclinedesign.net`. Implementation may require format specifications, technical documentation, sample files, test data and documentation for previous versions of the format (“Materials”).

By providing any Materials, you represent and warrant that:

you own or control the rights necessary to provide the Materials and grant the rights below, or you are duly authorised by the relevant rights holder to do so;
providing the Materials does not breach any confidentiality agreement, non-disclosure agreement, employment obligation, trade-secret obligation or other obligation owed to a third party; and
you are authorised to request and permit implementation of support for the relevant file format.

To the extent of the rights you own or control, you grant Leo Timmins a worldwide, perpetual, irrevocable, royalty-free, non-exclusive licence to use, reproduce, store, analyse, modify and adapt the Materials, and to use the information contained in them, for the purpose of developing, testing, maintaining, distributing and commercialising software that reads, writes, converts, imports, exports or otherwise interoperates with the relevant file format and any versions of that format identified in the Materials.

This licence includes the right to permit employees, contributors, contractors and service providers to exercise those rights on our behalf.

Unless separately agreed in writing before disclosure, Materials submitted under this process must not contain information that you are required to keep confidential, and the Materials will not be treated as confidential by us.

The licence and permissions granted above survive withdrawal of the support request and cannot subsequently be revoked.
