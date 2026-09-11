using Microsoft.AspNetCore.Identity;
using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Infrastructure.Identity;

public sealed class ApplicationIdentityUser : IdentityUser<Guid>
{
    private ApplicationIdentityUser()
    {
    }

    internal static ApplicationIdentityUser Create(Guid userId, string verifiedEmail)
    {

        ArgumentException.ThrowIfNullOrWhiteSpace(verifiedEmail);
        return new ApplicationIdentityUser
        {
            Id = userId,
            UserName = userId.ToString(),
            Email = verifiedEmail.Trim(),
            EmailConfirmed = true,
            SecurityStamp = Uuid7.Create().ToString(),
            ConcurrencyStamp = Uuid7.Create().ToString(),
        };
    }
}
