// A Scheduled Function: netlify.toml says when. Netlify does not serve it on
// a URL.

import { renewDue } from "../lite/gateway.mjs";
import { gateway } from "../start.mjs";

export default async () => {
  await renewDue(await gateway());
};
