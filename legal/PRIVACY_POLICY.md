# Styrhous Privacy Policy

Effective date: [DATE OF PUBLICATION]

This policy explains what personal data Styrhous collects, why, and what your
rights are. Styrhous is built to collect as little as possible: the desktop
application works with your Kubernetes clusters locally, and the licensing
service only stores what it needs to know who may use a license.

## 1. Who is responsible

The data controller is Styrhous, CVR [CVR NUMBER], Denmark ("we", "us").
Contact: `styrhous@zlepper.dk`.

## 2. What the desktop application does not collect

The Styrhous desktop application never sends your Kubernetes data to us. Cluster
names, kubeconfig files, credentials, resources, logs, and anything else you view
or edit in a cluster stay on your computer and travel only between your computer
and your clusters. The application contains no analytics, tracking, or crash
reporting.

The application contacts two services:

- **The licensing service** at `styrhous.app`, if you activate a license on the
  device (see section 3).
- **GitHub**, to check for and download updates from the public Styrhous release
  page. GitHub receives your IP address and the requested file, as with any
  download, under [GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## 3. What the licensing service stores

| Data | Why |
| --- | --- |
| **Account:** your email address, the sign-in provider you used (GitHub, Google, or Microsoft), and that provider's identifier for your account. | To sign you in and identify your account. We receive these from the provider when you sign in; we never receive your provider password. |
| **Sessions:** a hashed session token, the sign-in time, and the expiry time. | To keep you signed in. |
| **Organizations:** organization names, members and their roles, and product seat assignments. | To let organizations share and manage licenses. |
| **Invitations:** the invited email address, role, who invited them, and when the invitation was sent, accepted, cancelled, or expired. | To deliver and track invitations. Invitation emails are sent through Cloudflare. |
| **Devices:** a random installation identifier, a device name (by default "Styrhous on" followed by the operating system, and changeable by you), the operating system, processor architecture, Styrhous version, and when the device was activated, last checked in, or revoked. | To enforce the per-seat device limit and to let you see and revoke your devices. |
| **Billing status:** Stripe customer and subscription identifiers, subscription status, seat count, and billing-period dates. | To know which accounts hold a paid license. We never see or store card or bank details. |
| **Audit records:** which account performed which licensing action (for example, changing seats or revoking a device), on what, and when. | To secure the service and investigate problems or disputes. |
| **Technical request logs:** IP address, browser user agent, and request details, kept by our hosting provider for a short period. | To operate and secure the service. |

The licensing service uses two cookies, both strictly necessary: a session
cookie that keeps you signed in, and a short-lived cookie that protects the
sign-in process. It uses no advertising, analytics, or third-party tracking
cookies.

## 4. Payments

Purchases are processed through Stripe's Managed Payments, where Stripe (through
its Link service) acts as the reseller and merchant of record. Stripe collects
your payment details, billing address, and the information it needs for tax,
fraud prevention, and customer support, and processes them as an independent
controller under the [Stripe Privacy Policy](https://stripe.com/privacy) and the
[Link Privacy Policy](https://link.com/privacy). Stripe shares with us only what
we list under "Billing status" above.

## 5. Legal basis

We process your data because it is necessary to provide the service you signed
up for (GDPR art. 6(1)(b)). We keep technical logs and audit records to secure
the service, which is our legitimate interest (art. 6(1)(f)). We keep records we
are required to retain by law (art. 6(1)(c)).

## 6. Who processes data for us

- **Cloudflare** hosts the licensing service and its database, and sends
  invitation emails.
- **GitHub, Google, and Microsoft** sign you in, if you choose them.
- **Stripe** handles payments, as described in section 4.

We do not sell your data or share it for advertising. Some of these providers
process data outside the EU/EEA, including in the United States. Those transfers
rely on the EU–US Data Privacy Framework or the European Commission's standard
contractual clauses.

## 7. How long we keep data

- **Sessions** are deleted when they expire.
- **Invitations** are deleted 30 days after they expire.
- **Account, organization, device, and audit data** are kept while your account
  exists and deleted when you ask us to delete your account. We may keep records
  that the law requires us to retain, such as bookkeeping records, for as long as
  the law requires.

## 8. Your rights

Under the GDPR you may ask us for access to, correction of, deletion of, or a copy
of your personal data, and you may object to or ask us to restrict processing.
Email `styrhous@zlepper.dk` to use these rights, including to delete your account.
You may also complain to the Danish Data Protection Agency (Datatilsynet,
[datatilsynet.dk](https://www.datatilsynet.dk)).

## 9. Changes

We will publish any changes to this policy here, with a new effective date. If a
change materially affects how we use your data, we will notify account holders
by email before it takes effect.
