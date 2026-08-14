namespace Mfsd;

/// <summary>A native MFSD validation or conversion failure.</summary>
public sealed class MfsdException : Exception
{
    internal MfsdException(int statusCode, string message)
        : base(message)
    {
        StatusCode = statusCode;
    }

    /// <summary>The native status code associated with this failure.</summary>
    public int StatusCode { get; }
}
