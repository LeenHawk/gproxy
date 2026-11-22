import { gateway } from "../start.mjs";

export default async request => {
  const started = await gateway();
  return started instanceof Response ? started : started.fetch(request);
};

export const config = { path: "/*" };
