using Microsoft.EntityFrameworkCore;
using Npgsql;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

[TestFixture]
public sealed class DatabaseExceptionExtensionsTests
{
    [Test]
    public void RecognizesWrappedUniqueViolationsAndMatchesTheExactConstraint()
    {
        var postgres = new PostgresException(
            "duplicate", "ERROR", "ERROR", PostgresErrorCodes.UniqueViolation,
            constraintName: "identity_subject");
        var exception = new DbUpdateException("save failed", postgres);

        Assert.Multiple(() =>
        {
            Assert.That(postgres.IsUniqueViolation(), Is.True);
            Assert.That(exception.IsUniqueViolation("identity_subject"), Is.True);
            Assert.That(exception.IsUniqueViolation("other_constraint"), Is.False);
            Assert.That(exception.IsUniqueViolation("IDENTITY_SUBJECT"), Is.False);
            Assert.That(new InvalidOperationException().IsUniqueViolation(), Is.False);
            Assert.That(new PostgresException(
                "foreign key", "ERROR", "ERROR", PostgresErrorCodes.ForeignKeyViolation)
                .IsUniqueViolation(), Is.False);
        });
    }

    [TestCase(PostgresErrorCodes.SerializationFailure)]
    [TestCase(PostgresErrorCodes.DeadlockDetected)]
    public void RecognizesDirectAndNestedRetryableDatabaseFailures(string sqlState)
    {
        var postgres = new PostgresException("transaction failed", "ERROR", "ERROR", sqlState);

        Assert.Multiple(() =>
        {
            Assert.That(EfConcurrencyFailure.IsRetryable(postgres), Is.True);
            Assert.That(
                EfConcurrencyFailure.IsRetryable(new DbUpdateException("save failed", postgres)),
                Is.True);
            Assert.That(
                EfConcurrencyFailure.IsRetryable(
                    new InvalidOperationException(
                        "retry limit reached",
                        new DbUpdateException("save failed", postgres))),
                Is.True);
            Assert.That(
                EfConcurrencyFailure.IsRetryable(
                    new DbUpdateException(
                        "other database failure",
                        new PostgresException(
                            "foreign key", "ERROR", "ERROR", PostgresErrorCodes.ForeignKeyViolation))),
                Is.False);
        });
    }
}
