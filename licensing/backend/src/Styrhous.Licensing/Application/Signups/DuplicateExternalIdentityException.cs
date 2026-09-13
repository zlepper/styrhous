namespace Styrhous.Licensing.Application.Signups;

public sealed class DuplicateExternalIdentityException : Exception
{
    public DuplicateExternalIdentityException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
