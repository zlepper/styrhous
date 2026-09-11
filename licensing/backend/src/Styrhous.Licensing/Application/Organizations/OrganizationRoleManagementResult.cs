using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Application.Organizations;

public abstract class OrganizationRoleManagementResult
{
    private OrganizationRoleManagementResult(OrganizationRoleManagementStatus status)
    {
        Status = status;
    }

    public OrganizationRoleManagementStatus Status { get; }

    public sealed class RoleChanged : OrganizationRoleManagementResult
    {
        internal RoleChanged(
            Guid organizationId,
            Guid membershipId,
            Guid userId,
            OrganizationRole previousRole,
            OrganizationRole role,
            Guid correlationId,
            DateTimeOffset changedAt)
            : base(OrganizationRoleManagementStatus.RoleChanged)
        {
            OrganizationId = organizationId;
            MembershipId = membershipId;
            UserId = userId;
            PreviousRole = previousRole;
            Role = role;
            CorrelationId = correlationId;
            ChangedAt = changedAt;
        }

        public Guid OrganizationId { get; }

        public Guid MembershipId { get; }

        public Guid UserId { get; }

        public OrganizationRole PreviousRole { get; }

        public OrganizationRole Role { get; }

        public Guid CorrelationId { get; }

        public DateTimeOffset ChangedAt { get; }
    }

    public sealed class OwnershipTransferred : OrganizationRoleManagementResult
    {
        internal OwnershipTransferred(
            Guid organizationId,
            Guid previousOwnerMembershipId,
            Guid previousOwnerUserId,
            Guid ownerMembershipId,
            Guid ownerUserId,
            Guid correlationId,
            DateTimeOffset transferredAt)
            : base(OrganizationRoleManagementStatus.OwnershipTransferred)
        {
            OrganizationId = organizationId;
            PreviousOwnerMembershipId = previousOwnerMembershipId;
            PreviousOwnerUserId = previousOwnerUserId;
            OwnerMembershipId = ownerMembershipId;
            OwnerUserId = ownerUserId;
            CorrelationId = correlationId;
            TransferredAt = transferredAt;
        }

        public Guid OrganizationId { get; }

        public Guid PreviousOwnerMembershipId { get; }

        public Guid PreviousOwnerUserId { get; }

        public Guid OwnerMembershipId { get; }

        public Guid OwnerUserId { get; }

        public Guid CorrelationId { get; }

        public DateTimeOffset TransferredAt { get; }
    }

    public sealed class Rejection : OrganizationRoleManagementResult
    {
        internal Rejection(OrganizationRoleManagementStatus status)
            : base(status)
        {
            if (status is OrganizationRoleManagementStatus.RoleChanged
                or OrganizationRoleManagementStatus.OwnershipTransferred)
            {
                throw new ArgumentException(
                    "A successful role-management operation cannot be a rejection.",
                    nameof(status));
            }
        }
    }

    internal static RoleChanged Changed(
        Guid organizationId,
        Guid membershipId,
        Guid userId,
        OrganizationRole previousRole,
        OrganizationRole role,
        Guid correlationId,
        DateTimeOffset changedAt)
    {
        return new(
            organizationId,
            membershipId,
            userId,
            previousRole,
            role,
            correlationId,
            changedAt);
    }

    internal static OwnershipTransferred Transferred(
        Guid organizationId,
        Guid previousOwnerMembershipId,
        Guid previousOwnerUserId,
        Guid ownerMembershipId,
        Guid ownerUserId,
        Guid correlationId,
        DateTimeOffset transferredAt)
    {
        return new(
            organizationId,
            previousOwnerMembershipId,
            previousOwnerUserId,
            ownerMembershipId,
            ownerUserId,
            correlationId,
            transferredAt);
    }

    internal static Rejection Rejected(OrganizationRoleManagementStatus status)
    {
        return new(status);
    }
}
