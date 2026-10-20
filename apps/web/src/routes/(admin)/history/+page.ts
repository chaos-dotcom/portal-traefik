import { redirect } from "@sveltejs/kit";
import type { PageLoad } from "./$types";

// History lives on the dashboard now.
export const load: PageLoad = async () => {
  throw redirect(308, "/#history");
};
