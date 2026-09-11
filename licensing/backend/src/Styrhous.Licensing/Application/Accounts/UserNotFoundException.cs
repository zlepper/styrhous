namespace Styrhous.Licensing.Application.Accounts;

public sealed class UserNotFoundException : Exception
{
    public UserNotFoundException()
        : base("The user does not exist.")
    {
    }
}
