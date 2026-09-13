using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Diagnostics;
using Npgsql;
using Styrhous.Licensing.Persistence;

namespace Styrhous.Licensing.Tests.Persistence;

internal sealed class PostgresTestDatabase : IAsyncDisposable
{
    private const string ConnectionStringEnvironmentVariable =
        "STYRHOUS_LICENSING_TEST_POSTGRES";

    private const string DefaultConnectionString =
        "Host=127.0.0.1;Port=55432;Database=postgres;Username=styrhous;Password=local-development-only;Pooling=false";

    private readonly string _adminConnectionString;

    private PostgresTestDatabase(string adminConnectionString, string databaseName)
    {
        _adminConnectionString = adminConnectionString;
        DatabaseName = databaseName;

        var databaseConnection = new NpgsqlConnectionStringBuilder(adminConnectionString)
        {
            Database = databaseName,
            Pooling = false,
        };
        ConnectionString = databaseConnection.ConnectionString;
    }

    public string ConnectionString { get; }

    public string DatabaseName { get; }

    public static async Task<PostgresTestDatabase> CreateAsync()
    {
        return await CreateAsync(createSchema: true);
    }

    public static async Task<PostgresTestDatabase> CreateForMigrationAsync()
    {
        return await CreateAsync(createSchema: false);
    }

    private static async Task<PostgresTestDatabase> CreateAsync(bool createSchema)
    {
        var configuredConnectionString =
            Environment.GetEnvironmentVariable(ConnectionStringEnvironmentVariable)
            ?? DefaultConnectionString;
        var adminConnection = new NpgsqlConnectionStringBuilder(configuredConnectionString)
        {
            Database = "postgres",
            Pooling = false,
        };
        var databaseName = $"licensing_test_{Guid.CreateVersion7():N}";
        var database = new PostgresTestDatabase(adminConnection.ConnectionString, databaseName);

        await using var connection = await OpenAdminConnectionAsync(database._adminConnectionString);

        await using var command = connection.CreateCommand();
        command.CommandText = $"CREATE DATABASE \"{databaseName}\"";
        await command.ExecuteNonQueryAsync();

        try
        {
            if (createSchema)
            {
                await using var context = database.CreateContext();
                await context.Database.EnsureCreatedAsync();
            }

            return database;
        }
        catch (Exception initializationException)
        {
            try
            {
                await database.DropAsync();
            }
            catch (Exception cleanupException)
            {
                throw new AggregateException(
                    "The test database could not be initialized or cleaned up.",
                    initializationException,
                    cleanupException);
            }

            throw;
        }
    }

    public LicensingDbContext CreateContext(params IInterceptor[] interceptors)
    {
        return new LicensingDbContext(CreateOptions(interceptors));
    }

    public async Task<IReadOnlyList<string>> GetPublicTableNamesAsync()
    {
        await using var connection = new NpgsqlConnection(ConnectionString);
        await connection.OpenAsync();
        await using var command = connection.CreateCommand();
        command.CommandText = """
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public'
            """;
        await using var reader = await command.ExecuteReaderAsync();
        var tableNames = new List<string>();
        while (await reader.ReadAsync())
        {
            tableNames.Add(reader.GetString(0));
        }

        return tableNames;
    }

    public LicensingDbContext CreateContextWithCopenhagenTimeZone()
    {
        var connection = new NpgsqlConnectionStringBuilder(ConnectionString)
        {
            Options = "-c TimeZone=Europe/Copenhagen",
        };
        return new LicensingDbContext(CreateOptions(connection.ConnectionString, []));
    }

    public IDbContextFactory<LicensingDbContext> CreateContextFactory(
        params IInterceptor[] interceptors)
    {
        return new TestLicensingDbContextFactory(CreateOptions(interceptors));
    }

    private DbContextOptions<LicensingDbContext> CreateOptions(IInterceptor[] interceptors)
    {
        return CreateOptions(ConnectionString, interceptors);
    }

    private static DbContextOptions<LicensingDbContext> CreateOptions(
        string connectionString,
        IInterceptor[] interceptors)
    {
        return new DbContextOptionsBuilder<LicensingDbContext>()
            .UseNpgsql(connectionString)
            .EnableDetailedErrors()
            .AddInterceptors(interceptors)
            .Options;
    }

    public ValueTask DisposeAsync()
    {
        return new(DropAsync());
    }

    private static async Task<NpgsqlConnection> OpenAdminConnectionAsync(string connectionString)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
        Exception? lastFailure = null;

        while (!timeout.IsCancellationRequested)
        {
            var connection = new NpgsqlConnection(connectionString);
            try
            {
                await connection.OpenAsync(timeout.Token);
                return connection;
            }
            catch (NpgsqlException exception)
            {
                lastFailure = exception;
                await connection.DisposeAsync();
            }
            catch (OperationCanceledException exception) when (timeout.IsCancellationRequested)
            {
                lastFailure = exception;
                await connection.DisposeAsync();
                break;
            }

            try
            {
                await Task.Delay(TimeSpan.FromMilliseconds(200), timeout.Token);
            }
            catch (OperationCanceledException) when (timeout.IsCancellationRequested)
            {
                break;
            }
        }

        throw new InvalidOperationException(
            $"PostgreSQL is unavailable. Start it with 'docker compose -f licensing/compose.yaml up -d postgres' or set {ConnectionStringEnvironmentVariable}.",
            lastFailure);
    }

    private async Task DropAsync()
    {
        await using var connection = await OpenAdminConnectionAsync(_adminConnectionString);
        await using var command = connection.CreateCommand();
        command.CommandText = $"DROP DATABASE IF EXISTS \"{DatabaseName}\" WITH (FORCE)";
        await command.ExecuteNonQueryAsync();
    }

    private sealed class TestLicensingDbContextFactory(
        DbContextOptions<LicensingDbContext> options)
        : IDbContextFactory<LicensingDbContext>
    {

        public LicensingDbContext CreateDbContext()
        {
            return new(options);
        }

        public Task<LicensingDbContext> CreateDbContextAsync(
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            return Task.FromResult(CreateDbContext());
        }
    }
}
