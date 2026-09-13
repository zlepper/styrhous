using Npgsql;

namespace Styrhous.Licensing.DatabaseProvisioning;

public static class DatabaseRuntimeRoleProvisioner
{
    public const string ConfigurationSection = "DatabaseRuntimeRole";

    public static async Task ProvisionAsync(
        string connectionString,
        string username,
        string password,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(connectionString);
        if (string.IsNullOrWhiteSpace(username)
            || username.Length > 63
            || !username.All(character =>
                character is >= 'a' and <= 'z' or >= '0' and <= '9' or '_')
            || username[0] is >= '0' and <= '9')
        {
            throw new InvalidOperationException(
                $"{ConfigurationSection}:Username must be a lowercase PostgreSQL identifier.");
        }
        if (string.IsNullOrEmpty(password) || password.Contains('\0'))
        {
            throw new InvalidOperationException(
                $"{ConfigurationSection}:Password is required when provisioning the runtime role.");
        }

        await using var dataSource = NpgsqlDataSource.Create(connectionString);
        await using var connection = await dataSource.OpenConnectionAsync(cancellationToken);
        using var commandBuilder = new NpgsqlCommandBuilder();
        var role = commandBuilder.QuoteIdentifier(username);
        var database = commandBuilder.QuoteIdentifier(connection.Database);
        await using var quotePasswordCommand = new NpgsqlCommand(
            "SELECT quote_literal(@password)",
            connection);
        quotePasswordCommand.Parameters.AddWithValue("password", password);
        var passwordLiteral = (string)(
            await quotePasswordCommand.ExecuteScalarAsync(cancellationToken)
            ?? throw new InvalidOperationException(
                "PostgreSQL did not quote the database runtime password."));
        await using var roleAttributesCommand = new NpgsqlCommand(
            "SELECT rolsuper, rolreplication, rolbypassrls FROM pg_roles WHERE rolname = @username",
            connection);
        roleAttributesCommand.Parameters.AddWithValue("username", username);
        bool exists;
        await using (var roleAttributes = await roleAttributesCommand.ExecuteReaderAsync(
            cancellationToken))
        {
            exists = await roleAttributes.ReadAsync(cancellationToken);
            if (exists
                && (roleAttributes.GetBoolean(0)
                    || roleAttributes.GetBoolean(1)
                    || roleAttributes.GetBoolean(2)))
            {
                throw new InvalidOperationException(
                    $"The existing database runtime role '{username}' has elevated PostgreSQL attributes.");
            }
        }

        await using var credentialCommand = connection.CreateCommand();
        credentialCommand.CommandText = exists
            ? $"ALTER ROLE {role} LOGIN NOCREATEDB NOCREATEROLE PASSWORD {passwordLiteral}"
            : $"CREATE ROLE {role} LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD {passwordLiteral}";
        await credentialCommand.ExecuteNonQueryAsync(cancellationToken);

        await using var privilegesCommand = connection.CreateCommand();
        privilegesCommand.CommandText = $"""
            REVOKE CREATE ON SCHEMA public FROM PUBLIC;
            GRANT CONNECT ON DATABASE {database} TO {role};
            GRANT USAGE ON SCHEMA public TO {role};
            GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role};
            GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO {role};
            REVOKE INSERT, UPDATE, DELETE ON TABLE public."__EFMigrationsHistory" FROM {role};
            GRANT SELECT ON TABLE public."__EFMigrationsHistory" TO {role};
            REVOKE EXECUTE ON ALL FUNCTIONS IN SCHEMA public FROM PUBLIC;
            ALTER DEFAULT PRIVILEGES IN SCHEMA public
                GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {role};
            ALTER DEFAULT PRIVILEGES IN SCHEMA public
                GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO {role};
            ALTER DEFAULT PRIVILEGES IN SCHEMA public
                REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;
            """;
        await privilegesCommand.ExecuteNonQueryAsync(cancellationToken);
    }
}
