using System;
using Microsoft.EntityFrameworkCore.Migrations;

#nullable disable

namespace Styrhous.Licensing.Persistence.Migrations
{
    /// <inheritdoc />
    public partial class RemoveLegacyPublicationTracking : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropIndex(
                name: "ix_outbox_messages_recovery_publication_occurred_id",
                table: "outbox_messages");

            migrationBuilder.DropIndex(
                name: "ix_outbox_messages_unpublished_occurred_id",
                table: "outbox_messages");

            migrationBuilder.DropCheckConstraint(
                name: "ck_outbox_messages_lifecycle",
                table: "outbox_messages");

            migrationBuilder.DropIndex(
                name: "ix_billing_webhook_events_recovery_publication_received_id",
                table: "billing_webhook_events");

            migrationBuilder.DropCheckConstraint(
                name: "ck_billing_webhook_events_publication",
                table: "billing_webhook_events");

            migrationBuilder.DropColumn(
                name: "published_at",
                table: "outbox_messages");

            migrationBuilder.DropColumn(
                name: "last_published_at",
                table: "billing_webhook_events");

            migrationBuilder.AddCheckConstraint(
                name: "ck_outbox_messages_lifecycle",
                table: "outbox_messages",
                sql: "(not_after IS NULL OR not_after > occurred_at) AND (delivered_at IS NULL OR delivered_at >= occurred_at) AND ((discarded_at IS NULL AND discard_reason IS NULL) OR (discarded_at IS NOT NULL AND discard_reason IS NOT NULL AND discarded_at >= occurred_at)) AND NOT (delivered_at IS NOT NULL AND discarded_at IS NOT NULL)");
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropCheckConstraint(
                name: "ck_outbox_messages_lifecycle",
                table: "outbox_messages");

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "published_at",
                table: "outbox_messages",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.AddColumn<DateTimeOffset>(
                name: "last_published_at",
                table: "billing_webhook_events",
                type: "timestamp with time zone",
                nullable: true);

            migrationBuilder.CreateIndex(
                name: "ix_outbox_messages_recovery_publication_occurred_id",
                table: "outbox_messages",
                columns: new[] { "published_at", "occurred_at", "id" },
                filter: "delivered_at IS NULL AND discarded_at IS NULL");

            migrationBuilder.CreateIndex(
                name: "ix_outbox_messages_unpublished_occurred_id",
                table: "outbox_messages",
                columns: new[] { "occurred_at", "id" },
                filter: "published_at IS NULL AND delivered_at IS NULL AND discarded_at IS NULL");

            migrationBuilder.AddCheckConstraint(
                name: "ck_outbox_messages_lifecycle",
                table: "outbox_messages",
                sql: "(not_after IS NULL OR not_after > occurred_at) AND (published_at IS NULL OR published_at >= occurred_at) AND (delivered_at IS NULL OR delivered_at >= occurred_at) AND ((discarded_at IS NULL AND discard_reason IS NULL) OR (discarded_at IS NOT NULL AND discard_reason IS NOT NULL AND discarded_at >= occurred_at)) AND NOT (delivered_at IS NOT NULL AND discarded_at IS NOT NULL)");

            migrationBuilder.CreateIndex(
                name: "ix_billing_webhook_events_recovery_publication_received_id",
                table: "billing_webhook_events",
                columns: new[] { "last_published_at", "received_at", "id" },
                filter: "processed_at IS NULL");

            migrationBuilder.AddCheckConstraint(
                name: "ck_billing_webhook_events_publication",
                table: "billing_webhook_events",
                sql: "last_published_at IS NULL OR last_published_at >= received_at");
        }
    }
}
