using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Api.Organizations;

internal static class OrganizationRoleNames
{
    public static bool TryParseNonOwner(string? candidate, out OrganizationRole role)
    {
        role = candidate?.Trim().ToLowerInvariant() switch
        {
            "admin" => OrganizationRole.Admin,
            "member" => OrganizationRole.Member,
            _ => OrganizationRole.Owner,
        };
        return role != OrganizationRole.Owner;
    }

    public static string ToApiValue(OrganizationRole role)
    {
        return role switch
        {
            OrganizationRole.Owner => "owner",
            OrganizationRole.Admin => "admin",
            OrganizationRole.Member => "member",
            _ => throw new InvalidOperationException($"Unsupported organization role: {role}."),
        };
    }
}
