using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.AspNetCore.DataProtection;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Organizations;

namespace Styrhous.Licensing.Infrastructure.Messaging;

public sealed class DataProtectionOrganizationInvitationDeliveryProtector

{
    private const int CurrentSchemaVersion = 1;

    private const string ProtectionPurpose =
        "Styrhous.Licensing.OrganizationInvitationDelivery";

    private readonly IDataProtector _protector;

    public DataProtectionOrganizationInvitationDeliveryProtector(
        IDataProtectionProvider provider)
    {
        ArgumentNullException.ThrowIfNull(provider);
        _protector = provider.CreateProtector(ProtectionPurpose);
    }

    public string Protect(OrganizationInvitationDelivery delivery)
    {
        ArgumentNullException.ThrowIfNull(delivery);
        var payload = new SerializablePayload(
            CurrentSchemaVersion,
            delivery.Kind.ToString(),
            delivery.InvitationId,
            delivery.OrganizationId,
            delivery.Email,
            delivery.Role.ToString(),
            delivery.Secret.Reveal(),
            delivery.ExpiresAt);
        return _protector.Protect(JsonSerializer.Serialize(payload));
    }

    public OrganizationInvitationDelivery Unprotect(string protectedPayload)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(protectedPayload);
        var json = _protector.Unprotect(protectedPayload);
        var payload = JsonSerializer.Deserialize<SerializablePayload>(json)
            ?? throw new JsonException("The invitation delivery payload is empty.");
        if (payload.SchemaVersion != CurrentSchemaVersion)
        {
            throw new InvalidOperationException(
                $"Unsupported invitation delivery schema version: {payload.SchemaVersion}.");
        }

        if (!Enum.TryParse<OrganizationInvitationDeliveryKind>(
                payload.Kind,
                ignoreCase: false,
                out var kind)
            || !Enum.IsDefined(kind))
        {
            throw new InvalidOperationException(
                $"Unsupported invitation delivery kind: {payload.Kind}.");
        }

        if (!Enum.TryParse<OrganizationRole>(
                payload.Role,
                ignoreCase: false,
                out var role)
            || !Enum.IsDefined(role))
        {
            throw new InvalidOperationException(
                $"Unsupported invitation delivery role: {payload.Role}.");
        }

        return OrganizationInvitationDelivery.Restore(
            kind,
            payload.InvitationId,
            payload.OrganizationId,
            payload.Email,
            role,
            payload.Secret,
            payload.ExpiresAt);
    }

    public bool TryUnprotect(
        string protectedPayload,
        out OrganizationInvitationDelivery? delivery)
    {
        try
        {
            delivery = Unprotect(protectedPayload);
            return true;
        }
        catch (Exception exception) when (IsUnreadablePayload(exception))
        {
            delivery = null;
            return false;
        }
    }

    private static bool IsUnreadablePayload(Exception exception)
    {
        return exception is CryptographicException
            or JsonException
            or InvalidOperationException
            or ArgumentException;
    }

    private sealed record SerializablePayload(
        int SchemaVersion,
        string Kind,
        Guid InvitationId,
        Guid OrganizationId,
        string Email,
        string Role,
        string Secret,
        DateTimeOffset ExpiresAt);
}
