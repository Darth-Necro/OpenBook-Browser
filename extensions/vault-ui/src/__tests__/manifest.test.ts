// SPDX-License-Identifier: MPL-2.0
// Regression guard: the manifest must declare every permission-gated browser
// API the source actually uses. The historical bug this catches: background.ts
// calls browser.idle.* (auto-lock) but the manifest omitted the "idle"
// permission, so browser.idle was undefined and the listener registration threw
// at background-page load — auto-lock silently never armed.

import { readFileSync, readdirSync } from "fs";
import { join } from "path";

const PKG_ROOT = join(__dirname, "..", "..");

const manifest = JSON.parse(
  readFileSync(join(PKG_ROOT, "manifest.json"), "utf8")
) as { permissions?: string[] };
const perms: string[] = manifest.permissions ?? [];

function srcText(): string {
  const dir = join(PKG_ROOT, "src");
  return readdirSync(dir)
    .filter((f) => f.endsWith(".ts"))
    .map((f) => readFileSync(join(dir, f), "utf8"))
    .join("\n");
}

describe("vault-ui manifest permissions cover the browser APIs used", () => {
  const code = srcText();

  it("declares the baseline permissions (nativeMessaging, storage, idle)", () => {
    expect(perms).toEqual(
      expect.arrayContaining(["nativeMessaging", "storage", "idle"])
    );
  });

  // Generalized: a permission-gated namespace used in src must be declared, or
  // it is undefined at runtime and throws on first access.
  const gated: Record<string, string> = {
    "browser.idle": "idle",
    "browser.storage": "storage"
  };
  for (const [api, perm] of Object.entries(gated)) {
    it(`declares '${perm}' because the source uses ${api}.*`, () => {
      if (code.includes(`${api}.`)) {
        expect(perms).toContain(perm);
      }
    });
  }

  it("declares 'nativeMessaging' because the source uses connectNative", () => {
    if (code.includes("connectNative")) {
      expect(perms).toContain("nativeMessaging");
    }
  });
});
