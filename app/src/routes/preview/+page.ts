import { dev } from "$app/environment";
import { error } from "@sveltejs/kit";

// The component preview is a development tool only; the shipped app answers "Not found".
export function load() {
  if (!dev) error(404, "Not found");
}
