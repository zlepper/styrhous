import { describe, expect, it, vi } from 'vitest';
import {
  authenticationFailureMessage,
  authenticationProviderLabel,
  billingCheckoutErrorDetails,
  billingSeatQuantityErrorDetails,
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

  it('loads the browser session and protects account mutations with antiforgery', async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'account-csrf-token' });
      }
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

    expect(fetcher.mock.calls[3][0]).toBe('/auth/providers/google');
    expect(fetcher.mock.calls[3][1]?.method).toBe('DELETE');
    expect(new Headers(fetcher.mock.calls[3][1]?.headers).get('X-CSRF-TOKEN')).toBe(
      'account-csrf-token'
    );
    expect(fetcher.mock.calls[5][0]).toBe('/auth/sign-out');
    expect(fetcher.mock.calls[5][1]?.method).toBe('POST');
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

  it('fetches a current antiforgery token for every mutation', async () => {
    let tokenCount = 0;
    const fetcher = vi.fn<typeof fetch>(async (input, init) => {
      if (input === '/api/antiforgery') {
        tokenCount += 1;
        return Response.json({ requestToken: `csrf-token-${tokenCount}` });
      }

      return Response.json({ reasonCode: 'ok', request: { input, init } });
    });
    const api = createPortalApi(fetcher);

    await api.createOrganization('Platform team');
    await api.revokeDevice('activation-id');

    expect(fetcher).toHaveBeenCalledTimes(4);
    const organizationRequest = fetcher.mock.calls[1];
    const deviceRequest = fetcher.mock.calls[3];
    expect(organizationRequest[0]).toBe('/api/organizations');
    expect(organizationRequest[1]?.method).toBe('POST');
    expect(organizationRequest[1]?.body).toBe(JSON.stringify({ name: 'Platform team' }));
    expect(new Headers(organizationRequest[1]?.headers).get('X-CSRF-TOKEN')).toBe('csrf-token-1');
    expect(new Headers(organizationRequest[1]?.headers).get('Content-Type')).toBe(
      'application/json'
    );
    expect(deviceRequest[0]).toBe('/api/device-activations/activation-id');
    expect(deviceRequest[1]?.method).toBe('DELETE');
    expect(new Headers(deviceRequest[1]?.headers).get('X-CSRF-TOKEN')).toBe('csrf-token-2');
  });

  it('submits explicit product-seat choices for invitations and existing members', async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) =>
      input === '/api/antiforgery'
        ? Response.json({ requestToken: 'seat-csrf-token' })
        : Response.json({ reasonCode: 'ok' })
    );
    const api = createPortalApi(fetcher);

    await api.createInvitation('organization-id', 'admin@example.com', 'admin', false);
    await api.changeMemberSeat('organization-id', 'membership-id', false);

    const invitationRequest = fetcher.mock.calls[1];
    expect(invitationRequest[0]).toBe('/api/organizations/organization-id/invitations');
    expect(invitationRequest[1]?.body).toBe(
      JSON.stringify({
        email: 'admin@example.com',
        role: 'admin',
        assignProductSeat: false
      })
    );
    const seatRequest = fetcher.mock.calls[3];
    expect(seatRequest[0]).toBe(
      '/api/organizations/organization-id/members/membership-id/seat'
    );
    expect(seatRequest[1]?.body).toBe(JSON.stringify({ assigned: false }));
  });

  it('accepts an invitation through an antiforgery-protected mutation', async () => {
    const acceptance = {
      reasonCode: 'invitation_accepted',
      organizationId: 'organization-id'
    } as const;
    const fetcher = vi.fn<typeof fetch>(async (input) =>
      input === '/api/antiforgery'
        ? Response.json({ requestToken: 'acceptance-csrf-token' })
        : Response.json(acceptance)
    );

    await expect(createPortalApi(fetcher).acceptInvitation('invitation-secret')).resolves.toEqual(
      acceptance
    );

    const request = fetcher.mock.calls[1];
    expect(request[0]).toBe('/api/invitations/accept');
    expect(request[1]?.method).toBe('POST');
    expect(request[1]?.body).toBe(JSON.stringify({ secret: 'invitation-secret' }));
    expect(new Headers(request[1]?.headers).get('X-CSRF-TOKEN')).toBe(
      'acceptance-csrf-token'
    );
  });

  it('loads and submits the stable desktop approval contract as an antiforgery form', async () => {
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
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'approval-csrf-token' });
      }

      if (String(input).startsWith('/desktop/v1/device/approval?')) {
        return Response.json(approval);
      }

      return new Response(null, { status: 204 });
    });
    const api = createPortalApi(fetcher);

    await expect(api.getDeviceAuthorizationApproval('AB CD')).resolves.toEqual(approval);
    await expect(
      api.approveDeviceAuthorization('AB CD', '0191a8f0-1111-7000-8000-000000000072')
    ).resolves.toBeUndefined();
    await expect(api.denyDeviceAuthorization('AB CD')).resolves.toBeUndefined();

    expect(fetcher.mock.calls[0][0]).toBe(
      '/desktop/v1/device/approval?user_code=AB%20CD'
    );
    const approvalRequest = fetcher.mock.calls[2];
    expect(approvalRequest[0]).toBe('/desktop/v1/device/approval');
    expect(approvalRequest[1]?.method).toBe('POST');
    expect(approvalRequest[1]?.body).toBe(
      'user_code=AB+CD&decision=approve&seat_id=0191a8f0-1111-7000-8000-000000000072'
    );
    expect(new Headers(approvalRequest[1]?.headers).get('Content-Type')).toBe(
      'application/x-www-form-urlencoded'
    );
    expect(new Headers(approvalRequest[1]?.headers).get('Accept')).toBe('application/json');
    expect(new Headers(approvalRequest[1]?.headers).get('X-CSRF-TOKEN')).toBe(
      'approval-csrf-token'
    );
    const denialRequest = fetcher.mock.calls[4];
    expect(denialRequest[1]?.body).toBe('user_code=AB+CD&decision=deny');
  });

  it('recovers from an authentication challenge with a newly issued token', async () => {
    let tokenCount = 0;
    let mutationCount = 0;
    const fetcher = vi.fn<typeof fetch>(async (input, init) => {
      if (input === '/api/antiforgery') {
        tokenCount += 1;
        return Response.json({ requestToken: `csrf-token-${tokenCount}` });
      }

      mutationCount += 1;
      return mutationCount === 1
        ? new Response(null, { status: 401 })
        : Response.json({ reasonCode: 'device_revoked', request: { input, init } });
    });
    const api = createPortalApi(fetcher);

    await expect(api.revokeDevice('first-activation')).rejects.toBeInstanceOf(
      PortalAuthenticationRequiredError
    );
    await expect(api.revokeDevice('second-activation')).resolves.toBeTruthy();

    expect(tokenCount).toBe(2);
    const recoveredMutation = fetcher.mock.calls[3];
    expect(new Headers(recoveredMutation[1]?.headers).get('X-CSRF-TOKEN')).toBe('csrf-token-2');
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

  it('creates and retries Checkout with an antiforgery token and stable operation', async () => {
    let mutationCount = 0;
    const operationId = '0191a8f0-1111-7000-8000-000000000051';
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'checkout-csrf-token' });
      }

      mutationCount += 1;
      if (mutationCount === 1) {
        return Response.json(
          { reasonCode: 'checkout_provider_unavailable', billingOperationId: operationId },
          { status: 503 }
        );
      }

      return Response.json({
        reasonCode: 'checkout_session_created',
        billingOperationId: operationId,
        redirectUrl: 'https://checkout.stripe.test/session'
      });
    });
    const api = createPortalApi(fetcher);

    const firstError = await api
      .createCheckoutSession(
        '0191a8f0-1111-7000-8000-000000000041',
        'annual',
        7
      )
      .catch((caught: unknown) => caught);
    expect(firstError).toBeInstanceOf(PortalApiError);
    expect(billingCheckoutErrorDetails(firstError as PortalApiError).billingOperationId).toBe(
      operationId
    );

    await expect(
      api.createCheckoutSession(
        '0191a8f0-1111-7000-8000-000000000041',
        'annual',
        7,
        operationId
      )
    ).resolves.toEqual({
      reasonCode: 'checkout_session_created',
      billingOperationId: operationId,
      redirectUrl: 'https://checkout.stripe.test/session'
    });

    const firstMutation = fetcher.mock.calls[1];
    const retryMutation = fetcher.mock.calls[3];
    expect(firstMutation[0]).toBe(
      '/api/billing-accounts/0191a8f0-1111-7000-8000-000000000041/checkout-sessions'
    );
    expect(firstMutation[1]?.method).toBe('POST');
    expect(firstMutation[1]?.body).toBe(
      JSON.stringify({ cadence: 'annual', seatQuantity: 7, billingOperationId: null })
    );
    expect(new Headers(firstMutation[1]?.headers).get('X-CSRF-TOKEN')).toBe(
      'checkout-csrf-token'
    );
    expect(retryMutation[1]?.body).toBe(
      JSON.stringify({ cadence: 'annual', seatQuantity: 7, billingOperationId: operationId })
    );
  });

  it('creates a Customer Portal session with antiforgery and no browser-owned inputs', async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'portal-csrf-token' });
      }

      return Response.json({
        reasonCode: 'customer_portal_session_created',
        redirectUrl: 'https://billing.stripe.test/session'
      });
    });
    const api = createPortalApi(fetcher);

    await expect(
      api.createCustomerPortalSession('0191a8f0-1111-7000-8000-000000000041')
    ).resolves.toEqual({
      reasonCode: 'customer_portal_session_created',
      redirectUrl: 'https://billing.stripe.test/session'
    });

    const mutation = fetcher.mock.calls[1];
    expect(mutation[0]).toBe(
      '/api/billing-accounts/0191a8f0-1111-7000-8000-000000000041/customer-portal-sessions'
    );
    expect(mutation[1]?.method).toBe('POST');
    expect(mutation[1]?.body).toBeUndefined();
    expect(new Headers(mutation[1]?.headers).get('X-CSRF-TOKEN')).toBe(
      'portal-csrf-token'
    );
  });

  it('maps Customer Portal absence and provider outage to stable messages', async () => {
    let requestCount = 0;
    const api = createPortalApi(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'portal-csrf-token' });
      }

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

  it('changes and safely retries seat quantity with typed operation details', async () => {
    const operationId = '0191a8f0-1111-7000-8000-000000000061';
    let mutationCount = 0;
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'seat-change-csrf-token' });
      }

      mutationCount += 1;
      return mutationCount === 1
        ? Response.json(
            {
              reasonCode: 'seat_quantity_provider_unavailable',
              billingOperationId: operationId,
              previousSeatQuantity: 3,
              seatQuantity: 8
            },
            { status: 503 }
          )
        : Response.json({
            reasonCode: 'seat_quantity_changed',
            billingOperationId: operationId,
            seatQuantity: 8
          });
    });
    const api = createPortalApi(fetcher);

    const failed = await api
      .changeSeatQuantity('billing-account', 8)
      .catch((caught: unknown) => caught);
    expect(failed).toBeInstanceOf(PortalApiError);
    expect(billingSeatQuantityErrorDetails(failed as PortalApiError)).toEqual({
      billingOperationId: operationId,
      previousSeatQuantity: 3,
      seatQuantity: 8,
      requiredSeatQuantity: null
    });
    expect(portalErrorMessage(failed)).toBe(
      'The seat change could not be completed. Retry the same change.'
    );

    await expect(
      api.changeSeatQuantity('billing-account', 8, operationId)
    ).resolves.toEqual({
      reasonCode: 'seat_quantity_changed',
      billingOperationId: operationId,
      seatQuantity: 8
    });

    const initialRequest = fetcher.mock.calls[1];
    const retryRequest = fetcher.mock.calls[3];
    expect(initialRequest[0]).toBe('/api/billing-accounts/billing-account/seat-quantity');
    expect(initialRequest[1]?.method).toBe('PATCH');
    expect(initialRequest[1]?.body).toBe(
      JSON.stringify({ seatQuantity: 8, billingOperationId: null })
    );
    expect(new Headers(initialRequest[1]?.headers).get('X-CSRF-TOKEN')).toBe(
      'seat-change-csrf-token'
    );
    expect(retryRequest[1]?.body).toBe(
      JSON.stringify({ seatQuantity: 8, billingOperationId: operationId })
    );
  });

  it('preserves Checkout capacity details for an actionable correction', async () => {
    const api = createPortalApi(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'checkout-csrf-token' });
      }

      return Response.json(
        { reasonCode: 'seat_quantity_too_small', requiredSeatQuantity: 5 },
        { status: 409 }
      );
    });

    const error = await api
      .createCheckoutSession('account-id', 'monthly', 3)
      .catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(PortalApiError);
    expect(billingCheckoutErrorDetails(error as PortalApiError).requiredSeatQuantity).toBe(5);
    expect(portalErrorMessage(error)).toBe(
      'Choose at least 5 seats to cover product seats in use and seat-bearing invitations.'
    );
  });

  it('preserves one live Checkout attempt as typed recovery details', async () => {
    const operationId = '0191a8f0-1111-7000-8000-000000000051';
    const api = createPortalApi(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'checkout-csrf-token' });
      }

      return Response.json(
        {
          reasonCode: 'checkout_operation_in_progress',
          billingOperationId: operationId,
          cadence: 'monthly',
          seatQuantity: 4,
          expiresAt: '2026-09-01T10:50:00Z'
        },
        { status: 409 }
      );
    });

    const error = await api
      .createCheckoutSession('account-id', 'annual', 12)
      .catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(PortalApiError);
    expect(billingCheckoutErrorDetails(error as PortalApiError)).toEqual({
      billingOperationId: operationId,
      requiredSeatQuantity: null,
      cadence: 'monthly',
      seatQuantity: 4,
      expiresAt: '2026-09-01T10:50:00Z'
    });
    expect(portalErrorMessage(error)).toContain('recent checkout');
  });

  it('preserves reason codes and validation messages', async () => {
    const api = createPortalApi(async (input) => {
      if (input === '/api/antiforgery') {
        return Response.json({ requestToken: 'csrf-token' });
      }

      return Response.json(
        {
          title: 'Validation failed',
          errors: { email: ['A valid email address is required.'] }
        },
        { status: 400 }
      );
    });

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
      'invitation_cancellation_superseded',
      'The invitation changed before it could be cancelled.'
    ],
    ['invitation_resend_superseded', 'A newer invitation email has already been sent.'],
    ['billing_operation_not_found', 'That billing attempt can no longer be retried.'],
    [
      'device_authorization_concurrent_modification',
      'That desktop authorization changed while you were deciding. Review it and try again.'
    ],
    [
      'device_authorization_invalid_request',
      'That desktop approval request is invalid. Reload it and try again.'
    ],
    [
      'seat_quantity_reconciliation_required',
      'This seat change may already have completed. Check again later or contact support; another update will not be submitted automatically.'
    ]
  ])('maps the backend reason code %s to an actionable message', (reasonCode, message) => {
    const error = new PortalApiError(409, 'Conflict', reasonCode, {});

    expect(portalErrorMessage(error)).toBe(message);
  });
});
