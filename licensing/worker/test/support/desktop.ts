import { exports } from 'cloudflare:workers';
import { expect } from 'vitest';
import type { Browser } from './portal.ts';
import { origin, readJson } from './portal.ts';

export type DesktopInstallation = Readonly<{
  installation_id: string;
  display_name: string;
  platform: string;
  architecture: string;
  styrhous_version: string;
}>;

export function newInstallation(displayName = 'Workstation'): DesktopInstallation {
  return {
    installation_id: crypto.randomUUID(),
    display_name: displayName,
    platform: 'linux',
    architecture: 'x86_64',
    styrhous_version: '1.0.0'
  };
}

/** Calls the desktop protocol the way the Rust client does. */
export async function desktop(path: string, init: { form?: Record<string, string>; token?: string; method?: string } = {}) {
  const headers = new Headers();
  if (init.form) headers.set('Content-Type', 'application/x-www-form-urlencoded');
  if (init.token) headers.set('Authorization', `Bearer ${init.token}`);
  const response = await exports.default.fetch(`${origin}/desktop/v2${path}`, {
    method: init.method ?? (init.form || init.token ? 'POST' : 'GET'),
    headers,
    ...(init.form ? { body: new URLSearchParams(init.form).toString() } : {})
  });
  return readJson(response);
}

export function pollToken(deviceCode: string) {
  return desktop('/token', { form: { device_code: deviceCode } });
}

export async function startAuthorization(installation = newInstallation()) {
  const response = await desktop('/device/authorize', { form: { ...installation } });
  expect(response.status).toBe(200);
  return response.body as { device_code: string; user_code: string; verification_uri_complete: string; interval: number };
}

/** Approves a pending sign-in in the portal, on the given seat or the portal's suggestion. */
export async function approveDesktop(browser: Browser, userCode: string, seatId?: string) {
  const approval = await browser.json<{ selectedSeatId: string }>(`/api/device-authorizations/${userCode}`);
  expect(approval.status).toBe(200);
  const chosenSeatId = seatId ?? approval.body.selectedSeatId;
  const decision = await browser.json(`/api/device-authorizations/${userCode}`, 'POST', {
    decision: 'approve',
    seatId: chosenSeatId
  });
  expect(decision.status).toBe(204);
  return chosenSeatId;
}

/** Approves a pending sign-in in the portal and exchanges the device code for a token. */
export async function signInDesktop(browser: Browser, installation = newInstallation(), seatId?: string) {
  const authorization = await startAuthorization(installation);
  const chosenSeatId = await approveDesktop(browser, authorization.user_code, seatId);
  const token = await pollToken(authorization.device_code);
  expect(token.status).toBe(200);
  return { installation, token: token.body.installation_token as string, seatId: chosenSeatId };
}
