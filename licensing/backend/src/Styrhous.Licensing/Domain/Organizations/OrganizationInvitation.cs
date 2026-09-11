using Styrhous.Licensing.Domain.Identifiers;
using Styrhous.Licensing.Domain.Validation;

namespace Styrhous.Licensing.Domain.Organizations;

public sealed class OrganizationInvitation
{
    public const int MaximumEmailLength = EmailAddress.MaximumLength;

    public const int SecretHashLength = 64;

    public static readonly TimeSpan Lifetime = TimeSpan.FromDays(7);

    private OrganizationInvitation()
    {
    }

    private OrganizationInvitation(
        Guid id,
        Guid organizationId,
        Guid createdByUserId,
        string email,
        string normalizedEmail,
        OrganizationRole role,
        string secretHash,
        DateTimeOffset createdAt,
        DateTimeOffset lastSentAt,
        DateTimeOffset expiresAt,
        bool assignProductSeat)
    {
        Id = id;
        OrganizationId = organizationId;
        CreatedByUserId = createdByUserId;
        Email = email;
        NormalizedEmail = normalizedEmail;
        Role = role;
        SecretHash = secretHash;
        CreatedAt = createdAt;
        LastSentAt = lastSentAt;
        ExpiresAt = expiresAt;
        AssignProductSeat = assignProductSeat;
    }

    public Guid Id { get; private set; }

    public Guid OrganizationId { get; private set; }

    public Guid CreatedByUserId { get; private set; }

    public string Email { get; private set; } = string.Empty;

    public string NormalizedEmail { get; private set; } = string.Empty;

    public OrganizationRole Role { get; private set; }

    public string SecretHash { get; private set; } = string.Empty;

    public DateTimeOffset CreatedAt { get; private set; }

    public DateTimeOffset LastSentAt { get; private set; }

    public DateTimeOffset ExpiresAt { get; private set; }

    public int ReservedSeatCapacity { get; private set; }

    public bool AssignProductSeat { get; private set; }

    public DateTimeOffset? AcceptedAt { get; private set; }

    public Guid? AcceptedByUserId { get; private set; }

    public DateTimeOffset? CancelledAt { get; private set; }

    public void ReserveSeatCapacity(int seatCapacity)
    {
        if (!AssignProductSeat)
        {
            throw new InvalidOperationException(
                "A seatless invitation cannot reserve product-seat capacity.");
        }

        if (seatCapacity <= 0)
        {
            throw new ArgumentOutOfRangeException(
                nameof(seatCapacity),
                "An invitation reservation requires positive seat capacity.");
        }

        if (AcceptedAt is not null || CancelledAt is not null)
        {
            throw new InvalidOperationException(
                "A terminal invitation cannot change its seat reservation.");
        }

        ReservedSeatCapacity = seatCapacity;
    }

    public static OrganizationInvitation Create(
        Guid organizationId,
        Guid createdByUserId,
        string email,
        OrganizationRole role,
        string secretHash,
        DateTimeOffset createdAt,
        bool assignProductSeat = true)
    {

        if (role is not OrganizationRole.Admin and not OrganizationRole.Member)
        {
            throw new ArgumentException(
                "An invitation role must be Admin or Member.",
                nameof(role));
        }

        var normalizedSecretHash = NormalizeSecretHash(secretHash);

        if (!EmailAddress.IsValid(email))
        {
            throw new ArgumentException("A valid email address is required.", nameof(email));
        }

        var (trimmedEmail, normalizedEmail) = EmailAddress.Normalize(email, nameof(email));
        var utcCreatedAt = createdAt.ToUniversalTime();
        return new OrganizationInvitation(
            Uuid7.Create(),
            organizationId,
            createdByUserId,
            trimmedEmail,
            normalizedEmail,
            role,
            normalizedSecretHash,
            utcCreatedAt,
            utcCreatedAt,
            utcCreatedAt.Add(Lifetime),
            assignProductSeat);
    }

    public bool TryCancel(DateTimeOffset cancelledAt)
    {
        var utcCancelledAt = cancelledAt.ToUniversalTime();
        if (utcCancelledAt < LastSentAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(cancelledAt),
                "An invitation cannot be cancelled before it was last sent.");
        }

        if (AcceptedAt is not null || CancelledAt is not null || utcCancelledAt >= ExpiresAt)
        {
            return false;
        }

        CancelledAt = utcCancelledAt;
        return true;
    }

    public bool TryResend(string secretHash, DateTimeOffset resentAt)
    {
        var utcResentAt = resentAt.ToUniversalTime();
        if (utcResentAt < LastSentAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(resentAt),
                "An invitation cannot move its send window backwards.");
        }

        var normalizedSecretHash = NormalizeSecretHash(secretHash);
        if (AcceptedAt is not null || CancelledAt is not null)
        {
            return false;
        }

        SecretHash = normalizedSecretHash;
        LastSentAt = utcResentAt;
        ExpiresAt = utcResentAt.Add(Lifetime);
        return true;
    }

    public bool TryAccept(Guid userId, DateTimeOffset acceptedAt)
    {

        var utcAcceptedAt = acceptedAt.ToUniversalTime();
        if (utcAcceptedAt < LastSentAt)
        {
            throw new ArgumentOutOfRangeException(
                nameof(acceptedAt),
                "An invitation cannot be accepted before it was last sent.");
        }

        if (AcceptedAt is not null || CancelledAt is not null || utcAcceptedAt >= ExpiresAt)
        {
            return false;
        }

        AcceptedAt = utcAcceptedAt;
        AcceptedByUserId = userId;
        return true;
    }

    private static string NormalizeSecretHash(string secretHash)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(secretHash);
        var normalizedSecretHash = secretHash.Trim().ToLowerInvariant();
        if (normalizedSecretHash.Length != SecretHashLength
            || normalizedSecretHash.Any(character => !Uri.IsHexDigit(character)))
        {
            throw new ArgumentException(
                $"The invitation secret hash must contain {SecretHashLength} hexadecimal characters.",
                nameof(secretHash));
        }

        return normalizedSecretHash;
    }
}
