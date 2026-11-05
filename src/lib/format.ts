/**
 * How a version is shown in the UI: "V0.1.4". Config files keep plain semver
 * ("0.1.4") because Tauri and the updater require it; git tags stay "v0.1.4".
 */
export function displayVersion(v: string | null | undefined): string {
  const s = (v ?? "").trim().replace(/^v/i, "");
  return s ? `V${s}` : "";
}
