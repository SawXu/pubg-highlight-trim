import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(__dirname, "..");

// ui/package.json is the canonical UI version; Tauri.toml is binary metadata
// read by Tauri getVersion() and installer/registry entries. Keep them equal.
describe("version single source", () => {
  it("Tauri.toml version matches package.json", () => {
    const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
    const toml = readFileSync(
      join(root, "src-tauri", "Tauri.toml"),
      "utf8",
    );
    const match = toml.match(/^version\s*=\s*"([^"]+)"/m);
    expect(match?.[1]).toBe(pkg.version);
  });
});
