using Microsoft.EntityFrameworkCore.Migrations;
using Npgsql.EntityFrameworkCore.PostgreSQL.Metadata;

#nullable disable

namespace Styrhous.Licensing.Persistence.Migrations
{
    /// <inheritdoc />
    public partial class NativeRebusOutbox : Migration
    {
        /// <inheritdoc />
        protected override void Up(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.AddColumn<bool>(
                name: "NativeOutboxEnqueued",
                table: "outbox_messages",
                type: "boolean",
                nullable: false,
                defaultValue: false);

            migrationBuilder.AddColumn<bool>(
                name: "NativeOutboxEnqueued",
                table: "billing_webhook_events",
                type: "boolean",
                nullable: false,
                defaultValue: false);

            migrationBuilder.CreateTable(
                name: "RebusOutbox",
                columns: table => new
                {
                    Id = table.Column<long>(type: "bigint", nullable: false)
                        .Annotation("Npgsql:ValueGenerationStrategy", NpgsqlValueGenerationStrategy.IdentityByDefaultColumn),
                    CorrelationId = table.Column<string>(type: "character varying(16)", maxLength: 16, nullable: true),
                    MessageId = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: true),
                    SourceQueue = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: true),
                    DestinationAddress = table.Column<string>(type: "character varying(255)", maxLength: 255, nullable: false),
                    Headers = table.Column<string>(type: "text", nullable: true),
                    Body = table.Column<byte[]>(type: "bytea", nullable: true),
                    Sent = table.Column<bool>(type: "boolean", nullable: false, defaultValue: false)
                },
                constraints: table =>
                {
                    table.PrimaryKey("PK_RebusOutbox", x => x.Id);
                });
        }

        /// <inheritdoc />
        protected override void Down(MigrationBuilder migrationBuilder)
        {
            migrationBuilder.DropTable(
                name: "RebusOutbox");

            migrationBuilder.DropColumn(
                name: "NativeOutboxEnqueued",
                table: "outbox_messages");

            migrationBuilder.DropColumn(
                name: "NativeOutboxEnqueued",
                table: "billing_webhook_events");
        }
    }
}
