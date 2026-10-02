export function between(n: number, n1: number, n2: number): boolean {
  const min = Math.min(n1, n2);
  const max = Math.max(n1, n2);
  return n >= min && n <= max;
}
