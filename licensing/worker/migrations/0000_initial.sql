CREATE TABLE `audit_records` (
	`id` text PRIMARY KEY NOT NULL,
	`correlation_id` text NOT NULL,
	`actor_user_id` text,
	`action` text NOT NULL,
	`target_type` text NOT NULL,
	`target_id` text NOT NULL,
	`occurred_at` text NOT NULL
);
--> statement-breakpoint
CREATE INDEX `audit_records_target` ON `audit_records` (`target_type`,`target_id`);--> statement-breakpoint
CREATE TABLE `billing_accounts` (
	`id` text PRIMARY KEY NOT NULL,
	`kind` text NOT NULL,
	`personal_owner_user_id` text,
	`stripe_customer_id` text,
	`open_checkout_session_id` text,
	`open_checkout_expires_at` text,
	`created_at` text NOT NULL,
	FOREIGN KEY (`personal_owner_user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action,
	CONSTRAINT "billing_accounts_personal_owner_matches_kind" CHECK((kind = 'personal') = (personal_owner_user_id IS NOT NULL))
);
--> statement-breakpoint
CREATE UNIQUE INDEX `billing_accounts_personal_owner` ON `billing_accounts` (`personal_owner_user_id`) WHERE kind = 'personal';--> statement-breakpoint
CREATE TABLE `device_activations` (
	`id` text PRIMARY KEY NOT NULL,
	`seat_id` text NOT NULL,
	`user_id` text NOT NULL,
	`installation_id` text NOT NULL,
	`display_name` text NOT NULL,
	`platform` text NOT NULL,
	`architecture` text NOT NULL,
	`styrhous_version` text NOT NULL,
	`token_hash` text,
	`activated_at` text NOT NULL,
	`last_seen_at` text NOT NULL,
	`revoked_at` text,
	`revocation_reason` text,
	FOREIGN KEY (`seat_id`) REFERENCES `seats`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `device_activations_active_installation` ON `device_activations` (`seat_id`,`installation_id`) WHERE revoked_at IS NULL;--> statement-breakpoint
CREATE UNIQUE INDEX `device_activations_token_hash` ON `device_activations` (`token_hash`);--> statement-breakpoint
CREATE INDEX `device_activations_seat` ON `device_activations` (`seat_id`);--> statement-breakpoint
CREATE TABLE `device_authorizations` (
	`id` text PRIMARY KEY NOT NULL,
	`device_code_hash` text NOT NULL,
	`user_code` text NOT NULL,
	`installation_id` text NOT NULL,
	`display_name` text NOT NULL,
	`platform` text NOT NULL,
	`architecture` text NOT NULL,
	`styrhous_version` text NOT NULL,
	`status` text NOT NULL,
	`activation_id` text,
	`created_at` text NOT NULL,
	`expires_at` text NOT NULL,
	`last_polled_at` text,
	FOREIGN KEY (`activation_id`) REFERENCES `device_activations`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `device_authorizations_device_code_hash` ON `device_authorizations` (`device_code_hash`);--> statement-breakpoint
CREATE UNIQUE INDEX `device_authorizations_user_code` ON `device_authorizations` (`user_code`);--> statement-breakpoint
CREATE TABLE `external_identities` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`provider` text NOT NULL,
	`subject` text NOT NULL,
	`created_at` text NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `external_identities_provider_subject` ON `external_identities` (`provider`,`subject`);--> statement-breakpoint
CREATE UNIQUE INDEX `external_identities_user_provider` ON `external_identities` (`user_id`,`provider`);--> statement-breakpoint
CREATE TABLE `invitations` (
	`id` text PRIMARY KEY NOT NULL,
	`organization_id` text NOT NULL,
	`created_by_user_id` text NOT NULL,
	`email` text COLLATE NOCASE NOT NULL,
	`role` text NOT NULL,
	`assign_product_seat` integer NOT NULL,
	`secret_hash` text NOT NULL,
	`created_at` text NOT NULL,
	`last_sent_at` text NOT NULL,
	`expires_at` text NOT NULL,
	`accepted_at` text,
	`accepted_by_user_id` text,
	`cancelled_at` text,
	`last_delivery_error` text,
	FOREIGN KEY (`organization_id`) REFERENCES `organizations`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`created_by_user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`accepted_by_user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action,
	CONSTRAINT "invitations_single_outcome" CHECK(accepted_at IS NULL OR cancelled_at IS NULL)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `invitations_secret_hash` ON `invitations` (`secret_hash`);--> statement-breakpoint
CREATE INDEX `invitations_pending` ON `invitations` (`organization_id`,`email`,`expires_at`) WHERE accepted_at IS NULL AND cancelled_at IS NULL;--> statement-breakpoint
CREATE TABLE `memberships` (
	`id` text PRIMARY KEY NOT NULL,
	`organization_id` text NOT NULL,
	`user_id` text NOT NULL,
	`role` text NOT NULL,
	`created_at` text NOT NULL,
	FOREIGN KEY (`organization_id`) REFERENCES `organizations`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `memberships_organization_user` ON `memberships` (`organization_id`,`user_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `memberships_single_owner` ON `memberships` (`organization_id`) WHERE role = 'owner';--> statement-breakpoint
CREATE INDEX `memberships_user` ON `memberships` (`user_id`);--> statement-breakpoint
CREATE TABLE `oauth_states` (
	`state_hash` text PRIMARY KEY NOT NULL,
	`code_verifier` text NOT NULL,
	`provider` text NOT NULL,
	`operation` text NOT NULL,
	`return_url` text NOT NULL,
	`session_id` text,
	`expires_at` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `organizations` (
	`id` text PRIMARY KEY NOT NULL,
	`billing_account_id` text NOT NULL,
	`created_by_user_id` text NOT NULL,
	`name` text NOT NULL,
	`created_at` text NOT NULL,
	FOREIGN KEY (`billing_account_id`) REFERENCES `billing_accounts`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`created_by_user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `organizations_billing_account` ON `organizations` (`billing_account_id`);--> statement-breakpoint
CREATE TABLE `seats` (
	`id` text PRIMARY KEY NOT NULL,
	`billing_account_id` text NOT NULL,
	`user_id` text NOT NULL,
	`device_limit` integer DEFAULT 3 NOT NULL,
	`product_access` integer NOT NULL,
	`created_at` text NOT NULL,
	FOREIGN KEY (`billing_account_id`) REFERENCES `billing_accounts`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action,
	CONSTRAINT "seats_device_limit_positive" CHECK(device_limit > 0)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `seats_billing_account_user` ON `seats` (`billing_account_id`,`user_id`);--> statement-breakpoint
CREATE INDEX `seats_user` ON `seats` (`user_id`);--> statement-breakpoint
CREATE TABLE `sessions` (
	`id` text PRIMARY KEY NOT NULL,
	`token_hash` text NOT NULL,
	`user_id` text NOT NULL,
	`provider` text NOT NULL,
	`auth_time` text NOT NULL,
	`created_at` text NOT NULL,
	`expires_at` text NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `sessions_token_hash` ON `sessions` (`token_hash`);--> statement-breakpoint
CREATE INDEX `sessions_user` ON `sessions` (`user_id`);--> statement-breakpoint
CREATE TABLE `subscriptions` (
	`id` text PRIMARY KEY NOT NULL,
	`billing_account_id` text NOT NULL,
	`stripe_customer_id` text NOT NULL,
	`stripe_subscription_id` text NOT NULL,
	`stripe_price_id` text NOT NULL,
	`status` text NOT NULL,
	`seat_quantity` integer NOT NULL,
	`cancel_at_period_end` integer NOT NULL,
	`current_period_start` text NOT NULL,
	`current_period_end` text NOT NULL,
	`synced_at` text NOT NULL,
	FOREIGN KEY (`billing_account_id`) REFERENCES `billing_accounts`(`id`) ON UPDATE no action ON DELETE no action,
	CONSTRAINT "subscriptions_seat_quantity_positive" CHECK(seat_quantity > 0)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `subscriptions_billing_account` ON `subscriptions` (`billing_account_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `subscriptions_stripe_subscription` ON `subscriptions` (`stripe_subscription_id`);--> statement-breakpoint
CREATE TABLE `trials` (
	`id` text PRIMARY KEY NOT NULL,
	`originating_user_id` text NOT NULL,
	`billing_account_id` text NOT NULL,
	`started_at` text NOT NULL,
	`ends_at` text NOT NULL,
	`transferred_at` text,
	`terminated_at` text,
	FOREIGN KEY (`originating_user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`billing_account_id`) REFERENCES `billing_accounts`(`id`) ON UPDATE no action ON DELETE no action,
	CONSTRAINT "trials_ends_after_start" CHECK(ends_at > started_at)
);
--> statement-breakpoint
CREATE UNIQUE INDEX `trials_originating_user` ON `trials` (`originating_user_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `trials_billing_account` ON `trials` (`billing_account_id`);--> statement-breakpoint
CREATE TABLE `users` (
	`id` text PRIMARY KEY NOT NULL,
	`email` text NOT NULL,
	`created_at` text NOT NULL
);
--> statement-breakpoint
CREATE TABLE `verified_emails` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`email` text COLLATE NOCASE NOT NULL,
	`created_at` text NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `verified_emails_email` ON `verified_emails` (`email`);