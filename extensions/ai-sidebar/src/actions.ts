// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — action gating (build plan §7).
//
// The assistant is READ-ONLY by default. Any capability that takes an action is
// gated behind `requiresConfirmation:true` and an explicit per-action confirm
// callback. Model output is UNTRUSTED and is NEVER auto-executed: an action
// only runs after the human confirms THAT specific action. PURE except for the
// injected `confirm` and `run` callbacks.

/** Describes a single proposed action the user may approve or reject. */
export interface ActionRequest {
  /** Stable action kind, e.g. "copy-to-clipboard", "open-url". */
  kind: string;
  /** Human-readable summary shown in the confirmation prompt. */
  summary: string;
  /** Opaque, validated payload for the action's `run`. */
  payload: unknown;
}

/**
 * An action the assistant can propose. `requiresConfirmation` is ALWAYS true —
 * the type does not permit false, so no action can be defined as auto-run.
 */
export interface Action<P = unknown> {
  kind: string;
  /** Compile-time guarantee: every action requires confirmation. */
  requiresConfirmation: true;
  /** Performs the side effect. Only ever called after a positive confirm. */
  run(payload: P): Promise<ActionResult>;
}

export interface ActionResult {
  ok: boolean;
  message: string;
}

/** Outcome of attempting to perform an action through the gate. */
export type GateOutcome =
  | { status: "executed"; result: ActionResult }
  | { status: "declined" }
  | { status: "error"; message: string };

/**
 * Per-action confirmation gate. The model proposing an action is NOT enough;
 * `confirm(request)` must resolve true for THIS request before `action.run`
 * is ever invoked. If confirm resolves false, nothing runs.
 *
 * @param action  the (confirmation-required) action
 * @param request the concrete request (summary shown to the user)
 * @param confirm async user-confirmation callback (true = approved)
 */
export async function performAction<P>(
  action: Action<P>,
  request: ActionRequest,
  confirm: (request: ActionRequest) => Promise<boolean>
): Promise<GateOutcome> {
  // Defensive: actions are typed to always require confirmation; never bypass.
  if (action.requiresConfirmation !== true) {
    return { status: "error", message: "action is not confirmation-gated" };
  }
  let approved: boolean;
  try {
    approved = await confirm(request);
  } catch (e) {
    return {
      status: "error",
      message: e instanceof Error ? e.message : String(e)
    };
  }
  if (!approved) {
    return { status: "declined" };
  }
  try {
    const result = await action.run(request.payload as P);
    return { status: "executed", result };
  } catch (e) {
    return {
      status: "error",
      message: e instanceof Error ? e.message : String(e)
    };
  }
}
