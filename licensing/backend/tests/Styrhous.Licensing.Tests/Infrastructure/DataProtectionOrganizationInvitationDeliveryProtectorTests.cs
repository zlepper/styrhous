using System.Security.Cryptography;
using System.Text.Json;
using Microsoft.AspNetCore.DataProtection;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Styrhous.Licensing.Application.Messaging;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Infrastructure.DataProtection;
using Styrhous.Licensing.Infrastructure.Messaging;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class DataProtectionOrganizationInvitationDeliveryProtectorTests
{
    [Test]
    public void TamperedPayloadCannotBeUnprotected()
    {
        var protector = CreateProtector(new EphemeralDataProtectionProvider());
        var protectedPayload = protector.Protect(CreateDelivery());
        var tamperedIndex = protectedPayload.Length / 2;
        var replacement = protectedPayload[tamperedIndex] == 'A' ? 'B' : 'A';
        var tamperedPayload = protectedPayload[..tamperedIndex]
            + replacement
            + protectedPayload[(tamperedIndex + 1)..];

        Assert.Multiple(() =>
        {
            Assert.Throws<CryptographicException>(() => protector.Unprotect(tamperedPayload));
            Assert.That(protector.TryUnprotect(tamperedPayload, out var delivery), Is.False);
            Assert.That(delivery, Is.Null);
        });
    }

    [Test]
    public void DifferentKeyRingCannotUnprotectPayload()
    {
        var first = CreateProtector(new EphemeralDataProtectionProvider());
        var second = CreateProtector(new EphemeralDataProtectionProvider());
        var protectedPayload = first.Protect(CreateDelivery());

        Assert.Throws<CryptographicException>(() => second.Unprotect(protectedPayload));
    }

    [Test]
    public async Task IndependentHostsShareCertificateProtectedPostgresKeyRing()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        using var firstServices = CreateHostServices(database.ConnectionString);
        var first = CreateProtector(
            firstServices.GetRequiredService<IDataProtectionProvider>());
        var delivery = CreateDelivery();
        var protectedPayload = first.Protect(delivery);

        using var secondServices = CreateHostServices(database.ConnectionString);
        var second = CreateProtector(
            secondServices.GetRequiredService<IDataProtectionProvider>());
        var restored = second.Unprotect(protectedPayload);

        await using var context = database.CreateContext();
        var persistedKeys = await context.DataProtectionKeys.ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(restored.InvitationId, Is.EqualTo(delivery.InvitationId));
            Assert.That(restored.Secret.Reveal(), Is.EqualTo(delivery.Secret.Reveal()));
            Assert.That(persistedKeys, Is.Not.Empty);
            Assert.That(
                persistedKeys.Select(key => key.Xml),
                Has.All.Contain("encryptedSecret"));
            Assert.That(
                persistedKeys.Select(key => key.Xml),
                Has.None.Contain(delivery.Secret.Reveal()));
        });
    }

    [Test]
    public async Task RotatedHostUsesPreviousCertificateToReadExistingPayloads()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var previousCertificate = TestDataProtectionCertificate.CreateRsaCertificate();
        using var originalServices = CreateHostServices(
            database.ConnectionString,
            previousCertificate);
        var original = CreateProtector(
            originalServices.GetRequiredService<IDataProtectionProvider>());
        var delivery = CreateDelivery();
        var protectedPayload = original.Protect(delivery);

        var activeCertificate = TestDataProtectionCertificate.CreateRsaCertificate();
        using var rotatedServices = CreateHostServices(
            database.ConnectionString,
            activeCertificate,
            [previousCertificate]);
        var rotated = CreateProtector(
            rotatedServices.GetRequiredService<IDataProtectionProvider>());

        Assert.That(
            rotated.Unprotect(protectedPayload).Secret.Reveal(),
            Is.EqualTo(delivery.Secret.Reveal()));
    }

    [Test]
    public void StableOuterPurposeCanUnprotectVersionOnePayload()
    {
        var provider = new EphemeralDataProtectionProvider();
        var delivery = CreateDelivery();
        var versionOneJson = JsonSerializer.Serialize(
            new
            {
                SchemaVersion = 1,
                Kind = delivery.Kind.ToString(),
                delivery.InvitationId,
                delivery.OrganizationId,
                delivery.Email,
                Role = delivery.Role.ToString(),
                Secret = delivery.Secret.Reveal(),
                delivery.ExpiresAt,
            });
        var protectedPayload = provider
            .CreateProtector(
                "Styrhous.Licensing.OrganizationInvitationDelivery")
            .Protect(versionOneJson);

        var restored = CreateProtector(provider).Unprotect(protectedPayload);

        Assert.Multiple(() =>
        {
            Assert.That(restored.InvitationId, Is.EqualTo(delivery.InvitationId));
            Assert.That(restored.Secret.Reveal(), Is.EqualTo(delivery.Secret.Reveal()));
        });
    }

    [Test]
    public void TryUnprotectRejectsUnsupportedInnerSchemaWithoutLeakingAdapterException()
    {
        var provider = new EphemeralDataProtectionProvider();
        var unsupportedJson = JsonSerializer.Serialize(
            new
            {
                SchemaVersion = 2,
                Kind = OrganizationInvitationDeliveryKind.Created.ToString(),
                InvitationId = Guid.CreateVersion7(),
                OrganizationId = Guid.CreateVersion7(),
                Email = "invitee@example.com",
                Role = OrganizationRole.Member.ToString(),
                Secret = "secret",
                ExpiresAt = DateTimeOffset.UtcNow.AddDays(1),
            });
        var protectedPayload = provider
            .CreateProtector(
                "Styrhous.Licensing.OrganizationInvitationDelivery")
            .Protect(unsupportedJson);
        var protector = CreateProtector(provider);

        var result = protector.TryUnprotect(protectedPayload, out var delivery);

        Assert.Multiple(() =>
        {
            Assert.That(result, Is.False);
            Assert.That(delivery, Is.Null);
        });
    }

    [TestCase("999")]
    [TestCase("not-a-role")]
    public void TryUnprotectRejectsUnsupportedInnerRoleWithoutLeakingAdapterException(
        string role)
    {
        var provider = new EphemeralDataProtectionProvider();
        var unsupportedJson = JsonSerializer.Serialize(
            new
            {
                SchemaVersion = 1,
                Kind = OrganizationInvitationDeliveryKind.Created.ToString(),
                InvitationId = Guid.CreateVersion7(),
                OrganizationId = Guid.CreateVersion7(),
                Email = "invitee@example.com",
                Role = role,
                Secret = "secret",
                ExpiresAt = DateTimeOffset.UtcNow.AddDays(1),
            });
        var protectedPayload = provider
            .CreateProtector(
                "Styrhous.Licensing.OrganizationInvitationDelivery")
            .Protect(unsupportedJson);
        var protector = CreateProtector(provider);

        var result = protector.TryUnprotect(protectedPayload, out var delivery);

        Assert.Multiple(() =>
        {
            Assert.That(result, Is.False);
            Assert.That(delivery, Is.Null);
        });
    }

    [Test]
    public void MissingCertificateConfigurationFailsBeforeDataProtectionCanStart()
    {
        var services = new ServiceCollection();
        var configuration = new ConfigurationBuilder().Build();

        var exception = Assert.Throws<InvalidOperationException>(
            () => DataProtectionConfiguration.Configure(
                services,
                configuration));

        Assert.That(
            exception!.Message,
            Does.Contain(DataProtectionConfiguration.CertificateConfigurationKey));
    }

    [Test]
    public void NonRsaCertificateConfigurationFailsBeforeDataProtectionCanStart()
    {
        var services = new ServiceCollection();
        var configuration = new ConfigurationBuilder()
            .AddInMemoryCollection(
                new Dictionary<string, string?>
                {
                    [DataProtectionConfiguration.CertificateConfigurationKey] =
                        TestDataProtectionCertificate.CreateEcdsaCertificate(),
                    [DataProtectionConfiguration.CertificatePasswordConfigurationKey] =
                        TestDataProtectionCertificate.Password,
                })
            .Build();

        var exception = Assert.Throws<InvalidOperationException>(
            () => DataProtectionConfiguration.Configure(
                services,
                configuration));

        Assert.That(exception!.Message, Does.Contain("RSA"));
    }

    private static DataProtectionOrganizationInvitationDeliveryProtector CreateProtector(
        IDataProtectionProvider provider)
    {
        return new(provider);
    }

    private static ServiceProvider CreateHostServices(
        string connectionString,
        string? activeCertificate = null,
        IReadOnlyList<string>? previousCertificates = null)
    {
        var settings = new Dictionary<string, string?>
        {
            [DataProtectionConfiguration.CertificateConfigurationKey] =
                activeCertificate ?? TestDataProtectionCertificate.EncodedCertificate,
            [DataProtectionConfiguration.CertificatePasswordConfigurationKey] =
                TestDataProtectionCertificate.Password,
        };
        for (var index = 0; index < previousCertificates?.Count; index++)
        {
            settings[
                $"{DataProtectionConfiguration.PreviousCertificatesConfigurationKey}:"
                    + $"{index}:Certificate"] = previousCertificates[index];
            settings[
                $"{DataProtectionConfiguration.PreviousCertificatesConfigurationKey}:"
                    + $"{index}:CertificatePassword"] = TestDataProtectionCertificate.Password;
        }

        var configuration = new ConfigurationBuilder()
            .AddInMemoryCollection(settings)
            .Build();
        var services = new ServiceCollection();
        services.AddLogging();
        services.AddDbContextFactory<LicensingDbContext>(
            options => options.UseNpgsql(connectionString));
        DataProtectionConfiguration.Configure(services, configuration);
        return services.BuildServiceProvider();
    }

    private static OrganizationInvitationDelivery CreateDelivery()
    {
        return OrganizationInvitationDelivery.Restore(
            OrganizationInvitationDeliveryKind.Created,
            Guid.CreateVersion7(),
            Guid.CreateVersion7(),
            "invitee@example.com",
            OrganizationRole.Member,
            "protected-secret",
            new DateTimeOffset(2026, 9, 7, 10, 0, 0, TimeSpan.Zero));
    }
}
