export function classNames(
  ...parts: (string | false | null | undefined)[]
): string {
  return parts.filter(Boolean).join(" ");
}
