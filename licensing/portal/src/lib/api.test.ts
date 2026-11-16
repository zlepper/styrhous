import { describe, expect, it, vi } from 'vitest';
import {
  authenticationFailureMessage,
  authenticationProviderLabel,
  billingErrorDetails,
  createPortalApi,
  PortalApiError,
  PortalAuthenticationRequiredError,
  portalErrorMessage
} from './api';

describe('portal API', () => {
  it('uses one stable display label for every supported provider', () => {
    expect(
      (['github', 'google', 'microsoft'] as const).map(authenticationProviderLabel)
    ).toEqual(['GitHub', 'Google', 'Microsoft']);
  });

  it('maps authentication callback failures without reflecting unknown reason codes', () => {
    expect(authenticationFailureMessage('verified_email_required')).toBe(
      'Your identity provider did not supply a verified email address.'
    );
    expect(authenticationFailureMessage('untrusted detail from query')).toBe(
      'Authentication could not be completed. Please try again.'
    );
    expect(authenticationFailureMessage(null)).toBeNull();
  });

  it('rejects unsupported providers returned by the runtime API', async () => {
    const api = createPortalApi(async () =>
      Response.json({ providers: ['github', 'unexpected-provider'] })
    );

    await expect(api.listAuthenticationProviders()).rejects.toThrow(
      'invalid authentication providers'
    );
  });

  it('rejects malformed authenticated sessions before account links are rendered', async () => {
    const api = createPortalApi(async () =>
      Response.json({
        authenticated: true,
        userId: '01991f2a-1111-7000-8000-000000000001',
        email: 'person@example.com',
        linkedProviders: [],
        configuredProviders: ['github'],
        recentlyAuthenticated: false
      })
    );

    await expect(api.getAuthenticationSession()).rejects.toThrow(
      'invalid authentication session'
    );
  });

  it('loads the browser session and sends account mutations', async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (input === '/auth/providers') {
        return Response.json({ providers: ['github', 'google'] });
      }
      if (input === '/auth/session') {
        return Response.json({
          authenticated: true,
          userId: '01991f2a-1111-7000-8000-000000000001',
          email: 'person@example.com',
          linkedProviders: ['github'],
          configuredProviders: ['github', 'google'],
          recentlyAuthenticated: true
        });
      }
      return input === '/auth/sign-out'
        ? new Response(null, { status: 204 })
        : Response.json({ reasonCode: 'provider_unlinked' });
    });
    const api = createPortalApi(fetcher);

    await expect(api.listAuthenticationProviders()).resolves.toEqual(['github', 'google']);
    await expect(api.getAuthenticationSession()).resolves.toMatchObject({
      authenticated: true,
      email: 'person@example.com'
    });
    await api.unlinkAuthenticationProvider('google');
    await api.signOut();

    expect(fetcher).toHaveBeenCalledTimes(4);
    expect(fetcher.mock.calls[2][0]).toBe('/auth/providers/google');
    expect(fetcher.mock.calls[2][1]?.method).toBe('DELETE');
    expect(fetcher.mock.calls[3][0]).toBe('/auth/sign-out');
    expect(fetcher.mock.calls[3][1]?.method).toBe('POST');
  });

  it('uses same-origin credentials for authenticated reads', async () => {
    const fetcher = vi.fn<typeof fetch>(async () =>
      Response.json({ reasonCode: 'organizations_listed', organizations: [] })
    );
    const api = createPortalApi(fetcher);

    await expect(api.listOrganizations()).resolves.toEqual([]);

    expect(fetcher).toHaveBeenCalledOnce();
    expect(fetcher).toHaveBeenCalledWith(
      '/api/organizations',
      expect.objectContaining({ credentials: 'same-origin' })
    );
  });

  it('sends JSON mutations as single same-origin requests', async () => {
    const fetcher = vi.fn<typeof fetch>(async (input, init) =>
      Response.json({ reasonCode: 'ok', request: { input, init } })
    );
    const api = createPortalApi(fetcher);

    await api.createOrganization('Platform team');
    await api.revokeDevice('activation-id');

    expect(fetcher).toHaveBeenCalledTimes(2);
    const [organizationRequest, deviceRequest] = fetcher.mock.calls;
    expect(organizationRequest[0]).toBe('/api/organizations');
    expect(organizationRequest[1]?.method).toBe('POST');
    expect(organizationRequest[1]?.credentials).toBe('same-origin');
    expect(organizationRequest[1]?.body).toBe(JSON.stringify({ name: 'Platform team' }));
    expect(new Headers(organizationRequest[1]?.headers).get('Content-Type')).toBe(
      'application/json'
    );
    expect(deviceRequest[0]).toBe('/api/device-activations/activation-id');
    expect(deviceRequest[1]?.method).toBe('DELETE');
  });

  it('submits explicit product-seat choices and reports invitation email delivery', async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) =>
      String(input).endsWith('/invitations')
        ? Response.json(
            {
              reasonCode: 'invitation_created',
              invitationId: 'invitation-id',
              correlationId: 'correlation-id',
              expiresAt: '2026-09-08T10:00:00.000Z',
              emailDelivered: false
            },
            { status: 201 }
          )
        : Response.json({ reasonCode: 'ok' })
    );
    const api = createPortalApi(fetcher);

    await expect(
      api.createInvitation('organization-id', 'admin@example.com', 'admin', false)
    ).resolves.toMatchObject({ emailDelivered: false });
    await api.changeMemberSeat('organization-id', 'membership-id', false);

    const [invitationRequest, seatRequest] = fetcher.mock.calls;
    expect(invitationRequest[0]).toBe('/api/organizations/organization-id/invitations');
    expect(invitationRequest[1]?.body).toBe(
      JSON.stringify({
        email: 'admin@example.com',
        role: 'admin',
        assignProductSeat: false
      })
    );
    expect(seatRequest[0]).toBe(
      '/api/organizations/organization-id/members/membership-id/seat'
    );
    expect(seatRequest[1]?.body).toBe(JSON.stringify({ assigned: false }));
  });

  it('accepts an invitation with its secret', async () => {
    const acceptance = {
      reasonCode: 'invitation_accepted',
      organizationId: 'organization-id'
    } as const;
    const fetcher = vi.fn<typeof fetch>(async () => Response.json(acceptance));

    await expect(createPortalApi(fetcher).acceptInvitation('invitation-secret')).resolves.toEqual(
      acceptance
    );

    const [request] = fetcher.mock.calls;
    expect(request[0]).toBe('/api/invitations/accept');
    expect(request[1]?.method).toBe('POST');
    expect(request[1]?.body).toBe(JSON.stringify({ secret: 'invitation-secret' }));
  });

  it('loads and decides desktop approvals through the device authorization API', async () => {
    const approval = {
      reasonCode: 'device_authorization_awaiting_approval',
      installation: {
        installationId: '0191a8f0-1111-7000-8000-000000000071',
        displayName: 'Rasmus workstation',
        platform: 'linux',
        architecture: 'x86_64',
        styrhousVersion: '0.1.0'
      },
      eligibleSeats: [],
      selectedSeatId: null
    } as const;
    const fetcher = vi.fn<typeof fetch>(async (_input, init) =>
      init?.method === 'POST' ? new Response(null, { status: 204 }) : Response.json(approval)
    );
    const api = createPortalApi(fetcher);

    await expect(api.getDeviceAuthorizationApproval('AB CD')).resolves.toEqual(approval);
    await expect(
      api.approveDeviceAuthorization('AB CD', '0191a8f0-1111-7000-8000-000000000072')
    ).resolves.toBeUndefined();
    await expect(api.denyDeviceAuthorization('AB CD')).resolves.toBeUndefined();

    const [loadRequest, approvalRequest, denialRequest] = fetcher.mock.calls;
    expect(loadRequest[0]).toBe('/api/device-authorizations/AB%20CD');
    expect(approvalRequest[0]).toBe('/api/device-authorizations/AB%20CD');
    expect(approvalRequest[1]?.method).toBe('POST');
    expect(approvalRequest[1]?.body).toBe(
      JSON.stringify({ decision: 'approve', seatId: '0191a8f0-1111-7000-8000-000000000072' })
    );
    expect(new Headers(approvalRequest[1]?.headers).get('Content-Type')).toBe('application/json');
    expect(new Headers(approvalRequest[1]?.headers).get('Accept')).toBe('application/json');
    expect(denialRequest[1]?.body).toBe(JSON.stringify({ decision: 'deny' }));
  });

  it('turns API authentication challenges into a dedicated state', async () => {
    const api = createPortalApi(async () => new Response(null, { status: 401 }));

    await expect(api.listEntitlements()).rejects.toBeInstanceOf(
      PortalAuthenticationRequiredError
    );
  });

  it('lists billing accounts through the authenticated same-origin API', async () => {
    const account = {
      billingAccountId: '0191a8f0-1111-7000-8000-000000000041',
      accountKind: 'personal',
      organizationId: null,
      organizationName: null,
      organizationRole: null,
      canManageBilling: true,
      canStartCheckout: true,
      assignedSeatCount: 1,
      entitlement: {
        seatId: '0191a8f0-1111-7000-8000-000000000042',
        billingAccountId: '0191a8f0-1111-7000-8000-000000000041',
        state: 'evaluation',
        reasonCode: 'no_valid_entitlement',
        isEligible: false,
        validFrom: null,
        validUntil: null
      },
      trial: null,
      subscription: null
    } as const;
    const fetcher = vi.fn<typeof fetch>(async () =>
      Response.json({ reasonCode: 'billing_accounts_listed', accounts: [account] })
    );
    const api = createPortalApi(fetcher);

    await expect(api.listBillingAccounts()).resolves.toEqual([account]);

    expect(fetcher).toHaveBeenCalledOnce();
    expect(fetcher).toHaveBeenCalledWith(
      '/api/billing-accounts',
      expect.objectContaining({ credentials: 'same-origin' })
    );
  });

  it('creates Checkout with the chosen cadence and seat quantity', async () => {
    const fetcher = vi.fn<typeof fetch>(async () =>
      Response.json({
        reasonCode: 'checkout_session_created',
        redirectUrl: 'https://checkout.stripe.test/session'
      })
    );
    const api = createPortalApi(fetcher);

    await expect(
      api.createCheckoutSession('0191a8f0-1111-7000-8000-000000000041', 'annual', 7)
    ).resolves.toEqual({
      reasonCode: 'checkout_session_created',
      redirectUrl: 'https://checkout.stripe.test/session'
    });

    const [request] = fetcher.mock.calls;
    expect(request[0]).toBe(
      '/api/billing-accounts/0191a8f0-1111-7000-8000-000000000041/checkout-sessions'
    );
    expect(request[1]?.method).toBe('POST');
    expect(request[1]?.body).toBe(JSON.stringify({ cadence: 'annual', seatQuantity: 7 }));
  });

  it('creates a Customer Portal session with no browser-owned inputs', async () => {
    const fetcher = vi.fn<typeof fetch>(async () =>
      Response.json({
        reasonCode: 'customer_portal_session_created',
        redirectUrl: 'https://billing.stripe.test/session'
      })
    );
    const api = createPortalApi(fetcher);

    await expect(
      api.createCustomerPortalSession('0191a8f0-1111-7000-8000-000000000041')
    ).resolves.toEqual({
      reasonCode: 'customer_portal_session_created',
      redirectUrl: 'https://billing.stripe.test/session'
    });

    const [mutation] = fetcher.mock.calls;
    expect(mutation[0]).toBe(
      '/api/billing-accounts/0191a8f0-1111-7000-8000-000000000041/customer-portal-sessions'
    );
    expect(mutation[1]?.method).toBe('POST');
    expect(mutation[1]?.body).toBeUndefined();
  });

  it('maps Customer Portal absence and provider outage to stable messages', async () => {
    let requestCount = 0;
    const api = createPortalApi(async () => {
      requestCount += 1;
      return requestCount === 1
        ? Response.json({ reasonCode: 'subscription_not_found' }, { status: 409 })
        : Response.json(
            { reasonCode: 'customer_portal_provider_unavailable' },
            { status: 503 }
          );
    });

    const missing = await api
      .createCustomerPortalSession('account-id')
      .catch((caught: unknown) => caught);
    const unavailable = await api
      .createCustomerPortalSession('account-id')
      .catch((caught: unknown) => caught);

    expect(portalErrorMessage(missing)).toBe(
      'This account does not have a subscription to manage.'
    );
    expect(portalErrorMessage(unavailable)).toBe(
      'Billing management is temporarily unavailable. Try again.'
    );
  });

  it('changes the seat quantity and exposes the authoritative quantity on conflicts', async () => {
    let mutationCount = 0;
    const fetcher = vi.fn<typeof fetch>(async () => {
      mutationCount += 1;
      return mutationCount === 1
        ? Response.json(
            { reasonCode: 'subscription_quantity_changed', seatQuantity: 4 },
            { status: 409 }
          )
        : Response.json({ reasonCode: 'seat_quantity_changed', seatQuantity: 8 });
    });
    const api = createPortalApi(fetcher);

    const failed = await api
      .changeSeatQuantity('billing-account', 8)
      .catch((caught: unknown) => caught);
    expect(failed).toBeInstanceOf(PortalApiError);
    expect(billingErrorDetails(failed as PortalApiError)).toEqual({
      seatQuantity: 4,
      requiredSeatQuantity: null
    });

    await expect(api.changeSeatQuantity('billing-account', 8)).resolves.toEqual({
      reasonCode: 'seat_quantity_changed',
      seatQuantity: 8
    });

    fetcher.mockResolvedValueOnce(
      Response.json({
        reasonCode: 'seat_quantity_pending_payment',
        seatQuantity: 9,
        paymentUrl: 'https://invoice.stripe.test/pay'
      })
    );
    await expect(api.changeSeatQuantity('billing-account', 9)).resolves.toEqual({
      reasonCode: 'seat_quantity_pending_payment',
      seatQuantity: 9,
      paymentUrl: 'https://invoice.stripe.test/pay'
    });

    const [initialRequest] = fetcher.mock.calls;
    expect(initialRequest[0]).toBe('/api/billing-accounts/billing-account/seat-quantity');
    expect(initialRequest[1]?.method).toBe('PATCH');
    expect(initialRequest[1]?.body).toBe(JSON.stringify({ seatQuantity: 8 }));
    expect(fetcher.mock.calls[2]?.[1]?.body).toBe(JSON.stringify({ seatQuantity: 9 }));
  });

  it('preserves Checkout capacity details for an actionable correction', async () => {
    const api = createPortalApi(async () =>
      Response.json(
        { reasonCode: 'seat_quantity_too_small', requiredSeatQuantity: 5 },
        { status: 409 }
      )
    );

    const error = await api
      .createCheckoutSession('account-id', 'monthly', 3)
      .catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(PortalApiError);
    expect(billingErrorDetails(error as PortalApiError).requiredSeatQuantity).toBe(5);
    expect(portalErrorMessage(error)).toBe(
      'Choose at least 5 seats to cover product seats in use and seat-bearing invitations.'
    );
  });

  it('preserves reason codes and validation messages', async () => {
    const api = createPortalApi(async () =>
      Response.json(
        {
          title: 'Validation failed',
          errors: { email: ['A valid email address is required.'] }
        },
        { status: 400 }
      )
    );

    const error = await api
      .createInvitation('organization-id', 'invalid', 'member', true)
      .catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(PortalApiError);
    expect(portalErrorMessage(error)).toBe('A valid email address is required.');
  });

  it.each([
    ['organization_member_not_found', 'That organization member is no longer available.'],
    ['ownership_transfer_target_invalid', 'Choose another organization member as the new owner.'],
    ['organization_member_role_unchanged', 'That member already has the selected role.'],
    [
      'organization_member_seat_unchanged',
      'That member already has the selected product seat access.'
    ],
    [
      'organization_members_changed',
      'The roster changed while you were editing it. Review it and try again.'
    ],
    [
      'device_authorization_concurrent_modification',
      'That desktop authorization changed while you were deciding. Review it and try again.'
    ],
    [
      'checkout_operation_in_progress',
      'Another checkout was just started for this account. Reload and try again.'
    ],
    [
      'cross_origin_request_rejected',
      'The request was rejected because it did not come from the Styrhous portal. Reload and try again.'
    ]
  ])('maps the backend reason code %s to an actionable message', (reasonCode, message) => {
    const error = new PortalApiError(409, 'Conflict', reasonCode, {});

    expect(portalErrorMessage(error)).toBe(message);
  });
});
