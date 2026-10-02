export function choose<T>(from: T[]): T {
  return from[Math.floor(Math.random() * from.length)];
}
