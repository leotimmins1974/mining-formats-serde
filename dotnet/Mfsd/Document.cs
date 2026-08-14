using System.Runtime.InteropServices;

namespace Mfsd;

/// <summary>An owned, in-memory OMF document used for format conversion.</summary>
public sealed class Document : IDisposable
{
    private readonly NativeDocumentHandle _handle;

    private Document(NativeDocumentHandle handle)
    {
        _handle = handle;
        Elements = new ElementCollection(this);
    }

    /// <summary>The top-level elements in the document.</summary>
    public ElementCollection Elements { get; }

    /// <summary>Deserializes a complete file held in memory.</summary>
    public static Document Deserialize(Format format, byte[] input)
    {
        ArgumentNullException.ThrowIfNull(input);
        return Deserialize(format, input.AsSpan());
    }

    /// <summary>Deserializes a complete file held in memory.</summary>
    public static Document Deserialize(Format format, ReadOnlySpan<byte> input)
    {
        ValidateFormat(format);

        nint nativeDocument;
        int status;
        unsafe
        {
            fixed (byte* data = input)
            {
                status = NativeMethods.mfsd_document_deserialize((uint)format, data, (nuint)input.Length, out nativeDocument);
            }
        }

        NativeMethods.ThrowIfError(status);
        if (nativeDocument == nint.Zero)
        {
            throw new MfsdException(3, "MFSD returned a null document after a successful native call.");
        }

        return new Document(new NativeDocumentHandle(nativeDocument));
    }

    /// <summary>Serializes the document into independently usable streams.</summary>
    public byte[][] Serialize(Format format)
    {
        ThrowIfDisposed();
        ValidateFormat(format);

        var status = NativeMethods.mfsd_document_serialize(_handle, (uint)format, out var nativeBuffers);
        NativeMethods.ThrowIfError(status);
        if (nativeBuffers == nint.Zero)
        {
            throw new MfsdException(3, "MFSD returned null buffers after a successful native call.");
        }

        using var buffers = new NativeBuffersHandle(nativeBuffers);
        status = NativeMethods.mfsd_buffers_count(buffers, out var nativeCount);
        NativeMethods.ThrowIfError(status);

        var output = new byte[checked((int)nativeCount)][];
        for (var index = 0; index < output.Length; index++)
        {
            status = NativeMethods.mfsd_buffers_get(buffers, (nuint)index, out var data, out var nativeLength);
            NativeMethods.ThrowIfError(status);

            var length = checked((int)nativeLength);
            var stream = GC.AllocateUninitializedArray<byte>(length);
            if (length != 0)
            {
                Marshal.Copy(data, stream, 0, length);
            }
            output[index] = stream;
        }

        GC.KeepAlive(buffers);
        return output;
    }

    internal int GetElementCount()
    {
        ThrowIfDisposed();
        var status = NativeMethods.mfsd_document_element_count(_handle, out var count);
        NativeMethods.ThrowIfError(status);
        return checked((int)count);
    }

    private static void ValidateFormat(Format format)
    {
        if (format is < Format.Omf or > Format.Tiff)
        {
            throw new ArgumentOutOfRangeException(nameof(format), format, "The file format is not supported.");
        }
    }

    private void ThrowIfDisposed()
    {
        if (_handle.IsClosed)
        {
            throw new ObjectDisposedException(nameof(Document));
        }
    }

    public void Dispose()
    {
        _handle.Dispose();
        GC.SuppressFinalize(this);
    }
}

/// <summary>The top-level elements owned by an MFSD document.</summary>
public sealed class ElementCollection
{
    private readonly Document _document;

    internal ElementCollection(Document document)
    {
        _document = document;
    }

    /// <summary>The number of top-level elements.</summary>
    public int Count => _document.GetElementCount();
}
