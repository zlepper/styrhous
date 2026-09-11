namespace Styrhous.Licensing.Application.Signups;

public sealed class AccountLinkRequiredException : Exception
{
    public AccountLinkRequiredException()
        : base("Sign in with an existing provider before linking this identity.")
    {
    }

    public AccountLinkRequiredException(Exception innerException)
        : base("Sign in with an existing provider before linking this identity.", innerException)
    {
    }
}
