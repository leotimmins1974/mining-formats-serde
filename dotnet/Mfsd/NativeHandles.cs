using Microsoft.Win32.SafeHandles;

namespace Mfsd;

internal sealed class NativeDocumentHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    internal NativeDocumentHandle(nint handle)
        : base(ownsHandle: true)
    {
        SetHandle(handle);
    }

    protected override bool ReleaseHandle()
    {
        NativeMethods.mfsd_document_free(handle);
        return true;
    }
}

internal sealed class NativeBuffersHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    internal NativeBuffersHandle(nint handle)
        : base(ownsHandle: true)
    {
        SetHandle(handle);
    }

    protected override bool ReleaseHandle()
    {
        NativeMethods.mfsd_buffers_free(handle);
        return true;
    }
}
