using System.Text.Json;
using Styrhous.Licensing.Runtime;

namespace Styrhous.Licensing.DatabaseProvisioning;

public static class ProvisioningProgram
{
    public static async Task Main()
    {
        try
        {
            var configuration = await ApplicationConfigurationLoader.LoadAsync()
                ?? throw new InvalidOperationException("Database provisioning configuration is required.");
            await RunAsync(configuration);
            Console.WriteLine("Licensing database runtime role provisioned.");
        }
        catch (Exception)
        {
            // Database exceptions may embed role-management SQL containing credentials.
            Console.Error.WriteLine("Database runtime role provisioning failed.");
            Environment.ExitCode = 1;
        }
    }

    public static async Task RunAsync(string configuration, CancellationToken cancellationToken = default)
    {
        using var document = JsonDocument.Parse(configuration);
        var root = document.RootElement;
        var connectionString = root.GetProperty("ConnectionStrings").GetProperty("Licensing").GetString();
        var role = root.GetProperty(DatabaseRuntimeRoleProvisioner.ConfigurationSection);
        await DatabaseRuntimeRoleProvisioner.ProvisionAsync(
            connectionString ?? string.Empty,
            role.GetProperty("Username").GetString() ?? string.Empty,
            role.GetProperty("Password").GetString() ?? string.Empty,
            cancellationToken);
    }
}
