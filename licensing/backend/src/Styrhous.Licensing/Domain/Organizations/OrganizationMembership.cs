using Styrhous.Licensing.Domain.Identifiers;

namespace Styrhous.Licensing.Domain.Organizations;

public enum OrganizationRole
{
    Owner,
    Admin,
    Member,
}

public sealed class OrganizationMembership
{
    private OrganizationMembership()
    {
    }

    private OrganizationMembership(
        Guid id,
        Guid organizationId,
        Guid userId,
        OrganizationRole role,
        DateTimeOffset createdAt)
    {
        Id = id;
        OrganizationId = organizationId;
        UserId = userId;
        Role = role;
        CreatedAt = createdAt;
    }

    public Guid Id { get; private set; }

    public Guid OrganizationId { get; private set; }

    public Guid UserId { get; private set; }

    public OrganizationRole Role { get; private set; }

    public DateTimeOffset CreatedAt { get; private set; }

    internal static OrganizationMembership CreateOwner(
        Guid organizationId,
        Guid userId,
        DateTimeOffset createdAt)
    {
        return new OrganizationMembership(
            Uuid7.Create(),
            organizationId,
            userId,
            OrganizationRole.Owner,
            createdAt.ToUniversalTime());
    }

    internal static OrganizationMembership AcceptInvitation(
        Guid organizationId,
        Guid userId,
        OrganizationRole role,
        DateTimeOffset createdAt)
    {

        if (role is not OrganizationRole.Admin and not OrganizationRole.Member)
        {
            throw new ArgumentException(
                "An invited membership role must be Admin or Member.",
                nameof(role));
        }

        return new OrganizationMembership(
            Uuid7.Create(),
            organizationId,
            userId,
            role,
            createdAt.ToUniversalTime());
    }

}
