// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — thin UI <-> background messaging helpers.
// Browser-touching glue only; no testable business logic lives here.

import type { StatusResponse, ErrorResponse } from "./protocol.js";

export async function getStatus(): Promise<StatusResponse | ErrorResponse> {
  return browser.runtime.sendMessage({ cmd: "status" }) as Promise<
    StatusResponse | ErrorResponse
  >;
}

export async function sendCmd<T = unknown>(msg: Record<string, unknown>): Promise<T> {
  return browser.runtime.sendMessage(msg) as Promise<T>;
}

/** Replace text content safely (never innerHTML for host/host-derived data). */
export function setText(id: string, text: string): void {
  const el = document.getElementById(id);
  if (el) el.textContent = text;
}

export function show(id: string, visible: boolean): void {
  const el = document.getElementById(id);
  if (el) el.hidden = !visible;
}

export function byId<T extends HTMLElement = HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}
