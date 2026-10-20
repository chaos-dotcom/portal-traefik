import { redirect } from "@sveltejs/kit";
import type { PageLoad } from "./$types";

// Destinations folded into links: a link now carries its own Frame.io folder.
export const load: PageLoad = async () => {
  throw redirect(308, "/links");
};
