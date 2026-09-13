using Microsoft.EntityFrameworkCore;
using Npgsql;
using Styrhous.Licensing.Application.Accounts;
using Styrhous.Licensing.Application.Organizations;
using Styrhous.Licensing.Application.Signups;
using Styrhous.Licensing.Domain.Accounts;
using Styrhous.Licensing.Domain.Auditing;
using Styrhous.Licensing.Domain.Organizations;
using Styrhous.Licensing.Domain.Signups;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
[Parallelizable(ParallelScope.All)]
public sealed class OrganizationPersistenceTests
{
    private static readonly DateTimeOffset SignupTime =
        new(2026, 8, 30, 10, 15, 0, TimeSpan.Zero);

    [Test]
    public async Task FirstOrganizationAtomicallyCreatesOwnerSeatAndTransfersActiveTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);

        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1));
        var result = await organizationTest.Service
            .CreateAsync(signup.UserId, "Example Organization");

        Assert.Multiple(() =>
        {
            Assert.That(result.TrialWasTransferred, Is.True);
            Assert.That(result.UserId, Is.EqualTo(signup.UserId));
            Assert.That(
                new[]
                {
                    result.OrganizationId,
                    result.BillingAccountId,
                    result.OwnerMembershipId,
                    result.SeatId,
                    result.CorrelationId,
                },
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
        });

        await using var verificationContext = database.CreateContext();
        var organization = await verificationContext.Organizations.SingleAsync();
        var membership = await verificationContext.OrganizationMemberships.SingleAsync();
        var account = await verificationContext.BillingAccounts.SingleAsync(
            candidate => candidate.Id == result.BillingAccountId);
        var seat = await verificationContext.Seats.SingleAsync(
            candidate => candidate.Id == result.SeatId);
        var trial = await verificationContext.Trials.SingleAsync();
        var auditRecords = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == result.CorrelationId)
            .ToArrayAsync();

        Assert.Multiple(() =>
        {
            Assert.That(organization.Id, Is.EqualTo(result.OrganizationId));
            Assert.That(organization.Name, Is.EqualTo("Example Organization"));
            Assert.That(organization.CreatedByUserId, Is.EqualTo(signup.UserId));
            Assert.That(organization.BillingAccountId, Is.EqualTo(account.Id));
            Assert.That(account.Kind, Is.EqualTo(BillingAccountKind.Organization));
            Assert.That(account.PersonalOwnerUserId, Is.Null);
            Assert.That(membership.Id, Is.EqualTo(result.OwnerMembershipId));
            Assert.That(membership.OrganizationId, Is.EqualTo(organization.Id));
            Assert.That(membership.UserId, Is.EqualTo(signup.UserId));
            Assert.That(membership.Role, Is.EqualTo(OrganizationRole.Owner));
            Assert.That(seat.BillingAccountId, Is.EqualTo(account.Id));
            Assert.That(seat.AssignedUserId, Is.EqualTo(signup.UserId));
            Assert.That(trial.Id, Is.EqualTo(signup.TrialId));
            Assert.That(trial.BillingAccountId, Is.EqualTo(account.Id));
            Assert.That(trial.StartedAt, Is.EqualTo(SignupTime));
            Assert.That(trial.EndsAt, Is.EqualTo(SignupTime.AddDays(30)));
            Assert.That(trial.TransferredAt, Is.EqualTo(SignupTime.AddDays(1)));
            Assert.That(trial.BillingAccountId, Is.Not.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(auditRecords, Has.All.Property(nameof(AuditRecord.CorrelationId))
                .EqualTo(result.CorrelationId));
            Assert.That(auditRecords, Has.All.Property(nameof(AuditRecord.ActorUserId))
                .EqualTo(signup.UserId));
            Assert.That(auditRecords, Has.All.Property(nameof(AuditRecord.OccurredAt))
                .EqualTo(SignupTime.AddDays(1)));
            Assert.That(auditRecords.Select(record => record.Id),
                Has.All.Property(nameof(Guid.Version)).EqualTo(7));
            Assert.That(AuditTargets(auditRecords),
                Is.EquivalentTo(ExpectedAuditTargets(result, signup.TrialId)));
        });
    }

    [Test]
    public async Task LaterOrganizationDoesNotReceiveTheAlreadyTransferredTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);

        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1));
        var service = organizationTest.Service;
        var first = await service.CreateAsync(signup.UserId, "First Organization");
        var second = await service.CreateAsync(signup.UserId, "Second Organization");

        Assert.Multiple(() =>
        {
            Assert.That(first.TrialWasTransferred, Is.True);
            Assert.That(second.TrialWasTransferred, Is.False);
        });
        await using var verificationContext = database.CreateContext();
        var trial = await verificationContext.Trials.SingleAsync();
        var correlationIds = new[] { first.CorrelationId, second.CorrelationId };
        var auditRecords = await verificationContext.AuditRecords
            .Where(record => correlationIds.Contains(record.CorrelationId))
            .ToArrayAsync();
        var organizationCount = await verificationContext.Organizations.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(trial.BillingAccountId, Is.EqualTo(first.BillingAccountId));
            Assert.That(organizationCount, Is.EqualTo(2));
            Assert.That(
                auditRecords.Count(record => record.Action == AuditAction.TrialTransferred),
                Is.EqualTo(1));
            Assert.That(
                AuditTargets(auditRecords.Where(
                    record => record.CorrelationId == second.CorrelationId)),
                Is.EquivalentTo(ExpectedAuditTargets(second)));
        });
    }

    [Test]
    public async Task ExpiredTrialStaysOnThePersonalAccountWhenCreatingAnOrganization()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);

        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(30));
        var result = await organizationTest.Service
            .CreateAsync(signup.UserId, "Too Late Organization");

        Assert.That(result.TrialWasTransferred, Is.False);
        await using var verificationContext = database.CreateContext();
        var trial = await verificationContext.Trials.SingleAsync();
        var auditRecords = await verificationContext.AuditRecords
            .Where(record => record.CorrelationId == result.CorrelationId)
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(trial.BillingAccountId, Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(trial.TransferredAt, Is.Null);
            Assert.That(auditRecords, Has.All.Property(nameof(AuditRecord.ActorUserId))
                .EqualTo(signup.UserId));
            Assert.That(auditRecords, Has.All.Property(nameof(AuditRecord.OccurredAt))
                .EqualTo(SignupTime.AddDays(30)));
            Assert.That(AuditTargets(auditRecords),
                Is.EquivalentTo(ExpectedAuditTargets(result)));
        });
    }

    [Test]
    public async Task ConcurrentOrganizationCreationTransfersTheTrialExactlyOnce()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);

        var barrier = new DatabaseCommandBarrier(participantCount: 2);
        await using var organizationTest1 = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        await using var organizationTest2 = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [new DatabaseCommandBarrierInterceptor(barrier, "UPDATE user_accounts")]);
        var results = await Task.WhenAll(
            organizationTest1.Service.CreateAsync(signup.UserId, "First Concurrent Organization"),
            organizationTest2.Service.CreateAsync(signup.UserId, "Second Concurrent Organization"));

        Assert.Multiple(() =>
        {
            Assert.That(results.Count(result => result.TrialWasTransferred), Is.EqualTo(1));
            Assert.That(barrier.ArrivedCount, Is.EqualTo(2));
        });
        await using var verificationContext = database.CreateContext();
        var trial = await verificationContext.Trials.SingleAsync();
        var transferredResult = results.Single(result => result.TrialWasTransferred);
        var organizations = await verificationContext.Organizations.ToArrayAsync();
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync();
        var seatCount = await verificationContext.Seats.CountAsync();
        var ownerConcurrencyVersion = await verificationContext.UserAccounts
            .Where(user => user.Id == signup.UserId)
            .Select(user => user.ConcurrencyVersion)
            .SingleAsync();
        var resultCorrelationIds = results.Select(result => result.CorrelationId).ToArray();
        var auditRecords = await verificationContext.AuditRecords
            .Where(record => resultCorrelationIds.Contains(record.CorrelationId))
            .ToArrayAsync();
        Assert.Multiple(() =>
        {
            Assert.That(organizations, Has.Length.EqualTo(2));
            Assert.That(transferredResult.BillingAccountId, Is.EqualTo(trial.BillingAccountId));
            Assert.That(
                organizations.Select(organization => organization.BillingAccountId),
                Does.Contain(trial.BillingAccountId));
            Assert.That(membershipCount, Is.EqualTo(2));
            Assert.That(seatCount, Is.EqualTo(3));
            Assert.That(ownerConcurrencyVersion, Is.EqualTo(2));
            Assert.That(results.Select(result => result.CorrelationId), Is.Unique);
            Assert.That(
                auditRecords.GroupBy(record => record.CorrelationId)
                    .ToDictionary(group => group.Key, group => group.Count()),
                Is.EquivalentTo(
                    new Dictionary<Guid, int>
                    {
                        [transferredResult.CorrelationId] = 4,
                        [results.Single(result => !result.TrialWasTransferred).CorrelationId] = 3,
                    }));
            Assert.That(
                auditRecords.Single(record => record.Action == AuditAction.TrialTransferred)
                    .CorrelationId,
                Is.EqualTo(transferredResult.CorrelationId));
        });
        foreach (var result in results)
        {
            var transferredTrialId = result.TrialWasTransferred
                ? signup.TrialId
                : (Guid?)null;
            Assert.That(
                AuditTargets(auditRecords.Where(
                    record => record.CorrelationId == result.CorrelationId)),
                Is.EquivalentTo(ExpectedAuditTargets(result, transferredTrialId)));
        }
    }

    [Test]
    public async Task OrganizationCreatedAtTrialStartReceivesTheTrial()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);

        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime);
        var result = await organizationTest.Service
            .CreateAsync(signup.UserId, "Immediate Organization");

        Assert.That(result.TrialWasTransferred, Is.True);
        await using var verificationContext = database.CreateContext();
        var trial = await verificationContext.Trials.SingleAsync();
        Assert.Multiple(() =>
        {
            Assert.That(trial.BillingAccountId, Is.EqualTo(result.BillingAccountId));
            Assert.That(trial.TransferredAt, Is.EqualTo(SignupTime));
        });
    }

    [Test]
    public async Task FailureAfterSavingRollsBackOrganizationGraphAndTrialTransfer()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);
        await using var context = database.CreateContext();
        var baselineAuditCount = await context.AuditRecords.CountAsync();

        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1), interceptors: [new ThrowAfterSaveInterceptor()]);
        Assert.ThrowsAsync<SimulatedPostSaveException>(
            async () => await organizationTest.Service
                .CreateAsync(signup.UserId, "Rolled Back Organization"));

        await using var verificationContext = database.CreateContext();
        var trial = await verificationContext.Trials.SingleAsync();
        var organizationCount = await verificationContext.Organizations.CountAsync();
        var membershipCount = await verificationContext.OrganizationMemberships.CountAsync();
        var billingAccountCount = await verificationContext.BillingAccounts.CountAsync();
        var seatCount = await verificationContext.Seats.CountAsync();
        var auditCount = await verificationContext.AuditRecords.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(organizationCount, Is.Zero);
            Assert.That(membershipCount, Is.Zero);
            Assert.That(billingAccountCount, Is.EqualTo(1));
            Assert.That(seatCount, Is.EqualTo(1));
            Assert.That(auditCount, Is.EqualTo(baselineAuditCount));
            Assert.That(trial.BillingAccountId, Is.EqualTo(signup.PersonalBillingAccountId));
            Assert.That(trial.TransferredAt, Is.Null);
        });
    }

    [Test]
    public async Task DatabaseAllowsOnlyOneOrganizationPerBillingAccount()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);
        OrganizationCreationResult created;
        await using (var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1)))
        {
            created = await organizationTest.Service
                .CreateAsync(signup.UserId, "Existing Organization");
        }
        await using var context = database.CreateContext();
        context.Organizations.Add(Organization.Create(
            created.BillingAccountId,
            signup.UserId,
            "Duplicate",
            SignupTime));

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await context.SaveChangesAsync());

        Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
        Assert.That(
            ((PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ux_organizations_billing_account_id"));
    }

    [TestCase(BillingAccountKind.Organization, true)]
    [TestCase(BillingAccountKind.Personal, false)]
    public async Task DatabaseRequiresPersonalOwnershipOnlyForPersonalAccounts(
        BillingAccountKind kind,
        bool includePersonalOwner)
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);
        await using var context = database.CreateContext();
        var personalOwnerUserId = includePersonalOwner ? signup.UserId : (Guid?)null;
        var account = BillingAccount.CreateOrganization(SignupTime);
        context.Entry(account).Property(item => item.Kind).CurrentValue = kind;
        context.Entry(account).Property(item => item.PersonalOwnerUserId).CurrentValue =
            personalOwnerUserId;
        context.BillingAccounts.Add(account);

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await context.SaveChangesAsync());

        Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
        Assert.That(
            ((PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ck_billing_accounts_personal_owner"));
    }

    [Test]
    public async Task DatabaseAllowsOnlyOneMembershipPerOrganizationAndUser()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var signup = await SignUpOwnerAsync(database);
        await using var context = database.CreateContext();
        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1));
        var created = await organizationTest.Service
            .CreateAsync(signup.UserId, "Existing Organization");

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await InsertMembershipAsync(
                context,
                created.OrganizationId,
                signup.UserId,
                OrganizationRole.Member));

        Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
        Assert.That(
            ((PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ux_organization_memberships_organization_user"));
    }

    [Test]
    public async Task DatabaseAllowsOnlyOneOwnerPerOrganization()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        var owner = await SignUpOwnerAsync(database);
        var secondUser = await SignUpUserAsync(database, "second-user", "second@example.com");
        await using var context = database.CreateContext();
        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime.AddDays(1));
        var created = await organizationTest.Service
            .CreateAsync(owner.UserId, "Existing Organization");

        var exception = Assert.ThrowsAsync<DbUpdateException>(
            async () => await InsertMembershipAsync(
                context,
                created.OrganizationId,
                secondUser.UserId,
                OrganizationRole.Owner));

        Assert.That(exception!.InnerException, Is.TypeOf<PostgresException>());
        Assert.That(
            ((PostgresException)exception.InnerException!).ConstraintName,
            Is.EqualTo("ux_organization_memberships_single_owner"));
    }

    [Test]
    public async Task UnknownUserCannotCreateAnOrganization()
    {
        await using var database = await PostgresTestDatabase.CreateAsync();
        await using var context = database.CreateContext();

        await using var organizationTest = ServiceTestBase<OrganizationCreationService>.ForDatabase(
            database, SignupTime);
        Assert.ThrowsAsync<UserNotFoundException>(
            async () => await organizationTest.Service
                .CreateAsync(Guid.CreateVersion7(), "Orphan Organization"));

        var organizationCount = await context.Organizations.CountAsync();
        var membershipCount = await context.OrganizationMemberships.CountAsync();
        var billingAccountCount = await context.BillingAccounts.CountAsync();
        var seatCount = await context.Seats.CountAsync();
        var auditCount = await context.AuditRecords.CountAsync();
        Assert.Multiple(() =>
        {
            Assert.That(organizationCount, Is.Zero);
            Assert.That(membershipCount, Is.Zero);
            Assert.That(billingAccountCount, Is.Zero);
            Assert.That(seatCount, Is.Zero);
            Assert.That(auditCount, Is.Zero);
        });
    }

    private static Task<SignupResult> SignUpOwnerAsync(PostgresTestDatabase database)
    {
        return SignUpUserAsync(database, "organization-owner", "person@example.com");
    }

    private static async Task<SignupResult> SignUpUserAsync(
        PostgresTestDatabase database,
        string subject,
        string email)
    {
        await using var signupTest = ServiceTestBase<UserSignupService>.ForDatabase(database, SignupTime);
        return await signupTest.Service.SignUpAsync(
            VerifiedExternalIdentity.Create("github", subject, email));
    }

    private static async Task<int> InsertMembershipAsync(
        LicensingDbContext context,
        Guid organizationId,
        Guid userId,
        OrganizationRole role)
    {
        var membership = OrganizationMembership.AcceptInvitation(
            organizationId,
            userId,
            OrganizationRole.Member,
            SignupTime);
        context.Entry(membership).Property(item => item.Role).CurrentValue = role;
        context.OrganizationMemberships.Add(membership);
        return await context.SaveChangesAsync();
    }

    private static IEnumerable<(AuditAction Action, AuditTargetType TargetType, Guid TargetId)>
        AuditTargets(IEnumerable<AuditRecord> records)
    {
        return records.Select(record => (record.Action, record.TargetType, record.TargetId));
    }

    private static IEnumerable<(
        AuditAction Action,
        AuditTargetType TargetType,
        Guid TargetId)> ExpectedAuditTargets(
            OrganizationCreationResult result,
            Guid? transferredTrialId = null)
    {
        yield return (
            AuditAction.OrganizationCreated,
            AuditTargetType.Organization,
            result.OrganizationId);
        yield return (
            AuditAction.OrganizationOwnerAssigned,
            AuditTargetType.OrganizationMembership,
            result.OwnerMembershipId);
        yield return (
            AuditAction.SeatAssigned,
            AuditTargetType.Seat,
            result.SeatId);
        if (transferredTrialId is not null)
        {
            yield return (
                AuditAction.TrialTransferred,
                AuditTargetType.Trial,
                transferredTrialId.Value);
        }
    }
}
