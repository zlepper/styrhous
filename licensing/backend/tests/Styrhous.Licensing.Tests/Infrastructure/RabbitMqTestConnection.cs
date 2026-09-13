namespace Styrhous.Licensing.Tests.Infrastructure;

internal static class RabbitMqTestConnection
{
    private const string ConnectionEnvironmentVariable =
        "STYRHOUS_LICENSING_TEST_RABBITMQ";

    public static string Value =>
        Environment.GetEnvironmentVariable(ConnectionEnvironmentVariable)
        ?? "amqp://styrhous:local-development-only@127.0.0.1:55672";
}
