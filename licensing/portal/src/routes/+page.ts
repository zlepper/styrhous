import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
  redirect(307, `/billing${url.search}${url.hash}`);
};
