
export function gproxySleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
