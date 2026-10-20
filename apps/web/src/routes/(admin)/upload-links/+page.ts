import { redirect } from "@sveltejs/kit";
import type { PageLoad } from "./$types";

export type { UploadLink } from "$lib/links";

export const load: PageLoad = async () => {
  throw redirect(302, "/links");
};
