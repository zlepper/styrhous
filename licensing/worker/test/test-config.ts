/** Test-only Ed25519 key; it signs nothing outside the test suite and the protocol fixtures. */
export const testLeaseKey = {
  kty: 'OKP',
  crv: 'Ed25519',
  x: 'VO6Sie4zzIeXnOtLO6E12-FcOKvfDzNbzAf5MlRyC24',
  d: 'OF_a1tXMPep6VrqMG9IRn1t4V7WlOaxKJJUwfxLo3dM'
} as const;

export const testBindings = {
  PUBLIC_ORIGIN: 'https://licenses.example.com',
  INVITATION_FROM_ADDRESS: 'noreply@licenses.example.com',
  STRIPE_SECRET_KEY: 'sk_test_styrhous',
  STRIPE_WEBHOOK_SECRET: 'whsec_styrhous_test',
  STRIPE_MONTHLY_PRICE_ID: 'price_monthly',
  STRIPE_ANNUAL_PRICE_ID: 'price_annual',
  STRIPE_PORTAL_CONFIGURATION_ID: 'bpc_styrhous',
  LEASE_SIGNING_KEY: JSON.stringify(testLeaseKey),
  LEASE_PREVIOUS_PUBLIC_KEYS: '[]',
  GITHUB_CLIENT_ID: 'github-client',
  GITHUB_CLIENT_SECRET: 'github-secret',
  GOOGLE_CLIENT_ID: 'google-client',
  GOOGLE_CLIENT_SECRET: 'google-secret',
  MICROSOFT_CLIENT_ID: 'microsoft-client',
  MICROSOFT_CLIENT_SECRET: 'microsoft-secret'
};
