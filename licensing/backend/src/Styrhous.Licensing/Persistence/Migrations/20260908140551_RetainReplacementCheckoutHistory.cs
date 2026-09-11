using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Styrhous.Licensing.Persistence.Migrations
{
    /// <inheritdoc />
    public partial class RetainReplacementCheckoutHistory : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_billing_operations_lifecycle",
                table: "billing_operations");

            migrationBuilder.AddColumn<string>(
                name: "external_subscription_id",
                table: "billing_operations",
                type: "character varying(255)",
                maxLength: 255,
                nullable: true);

            migrationBuilder.AddColumn<string>(
                name: "previous_subscription",
                table: "billing_operations",
                type: "jsonb",
                nullable: true);

            migrationBuilder.AddCheckConstraint(
                name: "ck_billing_operations_lifecycle",
                table: "billing_operations",
                sql: "(kind = 'InitialCheckout' AND expires_at IS NOT NULL AND provider_mutation_replay_started_at IS NULL AND seat_quantity_outcome IS NULL AND ((status = 'Pending' AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND closed_at IS NULL) OR (status = 'ProviderSessionCreated' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NULL) OR (status = 'Completed' AND closed_at IS NOT NULL AND ((provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL) OR (provider_session_recorded_at IS NULL AND external_session_id IS NULL AND external_subscription_id IS NOT NULL))) OR (status = 'Failed' AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND closed_at IS NOT NULL) OR (status = 'Expired' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NOT NULL))) OR (kind = 'SeatQuantityChange' AND expires_at IS NULL AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND ((status = 'Pending' AND closed_at IS NULL AND seat_quantity_outcome IS NULL) OR (status = 'Completed' AND closed_at IS NOT NULL AND seat_quantity_outcome = 'Applied') OR (status = 'Failed' AND closed_at IS NOT NULL AND seat_quantity_outcome IN ('Superseded', 'ProviderRejected'))))");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_billing_operations_lifecycle",
                table: "billing_operations");

            migrationBuilder.DropColumn(
                name: "external_subscription_id",
                table: "billing_operations");

            migrationBuilder.DropColumn(
                name: "previous_subscription",
                table: "billing_operations");

            migrationBuilder.AddCheckConstraint(
                name: "ck_billing_operations_lifecycle",
                table: "billing_operations",
                sql: "(kind = 'InitialCheckout' AND expires_at IS NOT NULL AND provider_mutation_replay_started_at IS NULL AND seat_quantity_outcome IS NULL AND ((status = 'Pending' AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND closed_at IS NULL) OR (status = 'ProviderSessionCreated' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NULL) OR (status = 'Completed' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NOT NULL) OR (status = 'Failed' AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND closed_at IS NOT NULL) OR (status = 'Expired' AND provider_session_recorded_at IS NOT NULL AND external_session_id IS NOT NULL AND closed_at IS NOT NULL))) OR (kind = 'SeatQuantityChange' AND expires_at IS NULL AND provider_session_recorded_at IS NULL AND external_session_id IS NULL AND ((status = 'Pending' AND closed_at IS NULL AND seat_quantity_outcome IS NULL) OR (status = 'Completed' AND closed_at IS NOT NULL AND seat_quantity_outcome = 'Applied') OR (status = 'Failed' AND closed_at IS NOT NULL AND seat_quantity_outcome IN ('Superseded', 'ProviderRejected'))))");
        }
    }
}
