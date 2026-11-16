import { and, asc, inArray, isNotNull, isNull, lte, or, type SQL } from 'drizzle-orm';
import type { Database } from '../env';
import { deviceActivations } from '../db/schema';
import { dayMs, isoAfter, minuteMs } from '../lib/time';

/** A device unseen for this long may be replaced when its seat is at the device limit. */
const staleDeviceMs = 7 * dayMs;

/**
 * How long a desktop sign-in may take. An approved activation whose installation never
 * collected its token within this window is abandoned and may be replaced right away.
 */
export const desktopSignInWindowMs = 10 * minuteMs;

export type Installation = Readonly<{
  installationId: string;
  displayName: string;
  platform: string;
  architecture: string;
  styrhousVersion: string;
}>;

export function installationOf(row: Installation): Installation {
  const { installationId, displayName, platform, architecture, styrhousVersion } = row;
  return { installationId, displayName, platform, architecture, styrhousVersion };
}

export type ActiveDevice = Installation &
  Readonly<{ activationId: string; activatedAt: string; lastSeenAt: string; signedIn: boolean }>;

export type RevocationReason = 'revoked_by_user' | 'signed_out' | 'replaced_stale';

/** The column values that revoke an activation and invalidate its installation token. */
export function revocation(reason: RevocationReason, now: string) {
  return { revokedAt: now, revocationReason: reason, tokenHash: null };
}

/** Active devices per seat, oldest activation first. */
export async function activeDevicesBySeat(
  db: Database,
  seatIds: readonly string[]
): Promise<Map<string, ActiveDevice[]>> {
  const bySeat = new Map(seatIds.map((seatId) => [seatId, [] as ActiveDevice[]]));
  if (seatIds.length === 0) return bySeat;
  const rows = await db
    .select({
      seatId: deviceActivations.seatId,
      activationId: deviceActivations.id,
      installationId: deviceActivations.installationId,
      displayName: deviceActivations.displayName,
      platform: deviceActivations.platform,
      architecture: deviceActivations.architecture,
      styrhousVersion: deviceActivations.styrhousVersion,
      activatedAt: deviceActivations.activatedAt,
      lastSeenAt: deviceActivations.lastSeenAt,
      signedIn: isNotNull(deviceActivations.tokenHash).mapWith(Boolean)
    })
    .from(deviceActivations)
    .where(and(inArray(deviceActivations.seatId, [...seatIds]), isNull(deviceActivations.revokedAt)))
    .orderBy(asc(deviceActivations.activatedAt), asc(deviceActivations.id));
  for (const { seatId, ...device } of rows) bySeat.get(seatId)!.push(device);
  return bySeat;
}

export async function activeDevices(db: Database, seatId: string): Promise<ActiveDevice[]> {
  return (await activeDevicesBySeat(db, [seatId])).get(seatId)!;
}

function isReplaceable(device: ActiveDevice, now: string): boolean {
  return isoAfter(device.lastSeenAt, device.signedIn ? staleDeviceMs : desktopSignInWindowMs) <= now;
}

/** The write-time form of {@link isReplaceable}, for guarding a replacement. */
export function replaceableActivation(now: string): SQL {
  return or(
    lte(deviceActivations.lastSeenAt, isoAfter(now, -staleDeviceMs)),
    and(isNull(deviceActivations.tokenHash), lte(deviceActivations.lastSeenAt, isoAfter(now, -desktopSignInWindowMs)))
  )!;
}

export type ActivationPlan =
  | { kind: 'reuse'; activationId: string }
  | { kind: 'add' }
  | { kind: 'replace'; staleActivationId: string }
  | { kind: 'limit_reached' };

/**
 * Re-approving an installation reuses its activation. Otherwise a free slot is used, or the
 * least recently seen stale or abandoned device is replaced.
 */
export function planActivation(
  devices: readonly ActiveDevice[],
  installationId: string,
  deviceLimit: number,
  now: string
): ActivationPlan {
  const existing = devices.find((device) => device.installationId === installationId);
  if (existing) return { kind: 'reuse', activationId: existing.activationId };
  if (devices.length < deviceLimit) return { kind: 'add' };
  const stalest = devices
    .filter((device) => isReplaceable(device, now))
    .sort((a, b) => a.lastSeenAt.localeCompare(b.lastSeenAt))[0];
  return stalest ? { kind: 'replace', staleActivationId: stalest.activationId } : { kind: 'limit_reached' };
}
