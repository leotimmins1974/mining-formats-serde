using System.Runtime.InteropServices;

namespace Mfsd;

internal static partial class NativeMethods
{
    private const string LibraryName = "mfsd";
    private const int StatusSuccess = 0;

    [StructLayout(LayoutKind.Sequential)]
    internal struct Version
    {
        internal uint Major;
        internal uint Minor;
        internal uint Patch;
    }

    [LibraryImport(LibraryName)]
    internal static partial Version version();

    [LibraryImport(LibraryName)]
    internal static partial nint mfsd_last_error_message();

    [LibraryImport(LibraryName)]
    internal static unsafe partial int mfsd_document_deserialize(
        uint format,
        byte* data,
        nuint length,
        out nint document);

    [LibraryImport(LibraryName)]
    internal static partial int mfsd_document_element_count(
        NativeDocumentHandle document,
        out nuint count);

    [LibraryImport(LibraryName)]
    internal static partial int mfsd_document_serialize(
        NativeDocumentHandle document,
        uint format,
        out nint buffers);

    [LibraryImport(LibraryName)]
    internal static partial int mfsd_buffers_count(
        NativeBuffersHandle buffers,
        out nuint count);

    [LibraryImport(LibraryName)]
    internal static partial int mfsd_buffers_get(
        NativeBuffersHandle buffers,
        nuint index,
        out nint data,
        out nuint length);

    [LibraryImport(LibraryName)]
    internal static partial void mfsd_document_free(nint document);

    [LibraryImport(LibraryName)]
    internal static partial void mfsd_buffers_free(nint buffers);

    internal static void ThrowIfError(int status)
    {
        if (status == StatusSuccess)
        {
            return;
        }

        var pointer = mfsd_last_error_message();
        var message = pointer == nint.Zero ? null : Marshal.PtrToStringUTF8(pointer);
        if (string.IsNullOrWhiteSpace(message))
        {
            message = $"MFSD native call failed with status {status}.";
        }

        throw new MfsdException(status, message);
    }
}

public static class MfsdRuntime
{
    public static (uint Major, uint Minor, uint Patch) Version
    {
        get
        {
            var version = NativeMethods.version();
            return (version.Major, version.Minor, version.Patch);
        }
    }
}
