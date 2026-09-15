using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Persistence;
using Styrhous.Licensing.Tests.Persistence;

namespace Styrhous.Licensing.Tests.Infrastructure;

internal static class BackgroundWorkTestScenario
{
    public static async Task<Guid> CreateInvitationOutboxAsync(
        PostgresTestDatabase database,
        string subject,
        string queueName = "test-queue")
    {
        var owner = await LicensingPersistenceScenario.SignUpAsync(
            database,
            $"background-{subject}-owner",
            $"background-{subject}-owner@example.com");
        var organization = await LicensingPersistenceScenario.CreateOrganizationAsync(
            database,
            owner.UserId,
            $"Background {subject}",
            LicensingPersistenceScenario.SignupTime.AddDays(1));
        await using var test =
            ServiceTestBase<OrganizationInvitationCreationService>.ForDatabaseWithBackgroundQueue(
                database,
                LicensingPersistenceScenario.SignupTime.AddDays(2),
                queueName);
        var context = test.Services.GetRequiredService<LicensingDbContext>();
        var result = await test.Service.CreateAsync(
            owner.UserId,
            organization.OrganizationId,
            $"background-{subject}-invitee@example.com",
            OrganizationRole.Member);
        Assert.That(result, Is.TypeOf<OrganizationInvitationCreationResult.Success>());
        var invitation = (OrganizationInvitationCreationResult.Success)result;
        return await context.OutboxMessages
            .Where(message => message.SubjectId == invitation.InvitationId)
            .Select(message => message.Id)
            .SingleAsync();
    }
}
