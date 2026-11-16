import { and, eq } from 'drizzle-orm';
import type { Database } from '../env';
import { memberships, organizations, seats, type MemberRole } from '../db/schema';
import { fail } from '../http/errors';
import { isUuid } from '../lib/ids';

export type OrganizationAccess = Readonly<{
  organizationId: string;
  billingAccountId: string;
  name: string;
  membershipId: string;
  role: MemberRole;
}>;

/** The caller's membership; non-members are told the organization does not exist. */
export async function organizationAccess(
  db: Database,
  organizationId: string,
  userId: string
): Promise<OrganizationAccess> {
  if (!isUuid(organizationId)) fail(404, 'organization_not_found');
  const [row] = await db
    .select({
      organizationId: organizations.id,
      billingAccountId: organizations.billingAccountId,
      name: organizations.name,
      membershipId: memberships.id,
      role: memberships.role
    })
    .from(memberships)
    .innerJoin(organizations, eq(organizations.id, memberships.organizationId))
    .where(and(eq(memberships.organizationId, organizationId), eq(memberships.userId, userId)));
  return row ?? fail(404, 'organization_not_found');
}

export function requireRole(access: OrganizationAccess, ...roles: MemberRole[]) {
  if (!roles.includes(access.role)) fail(403, 'insufficient_permission');
}

export type MemberRow = Readonly<{
  membershipId: string;
  userId: string;
  role: MemberRole;
  seatId: string;
  productAccess: boolean;
  deviceLimit: number;
}>;

export async function organizationMember(
  db: Database,
  access: OrganizationAccess,
  membershipId: string
): Promise<MemberRow> {
  if (!isUuid(membershipId)) fail(404, 'organization_member_not_found');
  const [row] = await db
    .select({
      membershipId: memberships.id,
      userId: memberships.userId,
      role: memberships.role,
      seatId: seats.id,
      productAccess: seats.productAccess,
      deviceLimit: seats.deviceLimit
    })
    .from(memberships)
    .innerJoin(
      seats,
      and(eq(seats.userId, memberships.userId), eq(seats.billingAccountId, access.billingAccountId))
    )
    .where(and(eq(memberships.id, membershipId), eq(memberships.organizationId, access.organizationId)));
  return row ?? fail(404, 'organization_member_not_found');
}
