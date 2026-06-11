// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — native messaging protocol types and typed client.
//
// This module defines the wire contract with the `org.openbook.vault_host`
// Rust native host and a `VaultClient` that correlates requests/responses by
// `id` over a `browser.runtime.Port` (connectNative) connection.
//
// SECURITY NOTES:
//   * The host is treated as a native application; the browser only validates
//     the host manifest, it does not install/manage it (see build plan §1/§11).
//   * No secrets are persisted by this client. `secret` values flow straight to
//     the host and are not logged or cached here.

/** Hardware backing reported by the host for the lockout counter / key seal. */
export type Hardware = "tpm2" | "secure-enclave" | "software";

/** Lifecycle state of the vault as reported by the host. */
export type VaultState = "uninitialized" | "locked" | "unlocked" | "erased";

/** Error codes the host may return. Open string union for forward-compat. */
export type VaultErrorCode =
  | "invalid-request"
  | "not-initialized"
  | "already-initialized"
  | "bad-secret"
  | "erased"
  | "weak-secret"
  | "no-recovery-not-acknowledged"
  | "hardware-unavailable"
  | "internal"
  | (string & {});

// ---------------------------------------------------------------------------
// Request messages (extension -> host). Every request carries `type` and `id`.
// ---------------------------------------------------------------------------

export interface BaseRequest {
  /** Discriminator for the request kind. */
  type: RequestType;
  /**
   * Correlation id; the matching response echoes this exact value.
   * MUST be a JSON number: the host parses it as an `i64` and rejects any
   * frame whose id is a string (and host responses always carry a numeric
   * id, which would never match a string-keyed pending map).
   */
  id: number;
}

export type RequestType = "status" | "setup" | "unlock" | "lock" | "erase";

export interface StatusRequest extends BaseRequest {
  type: "status";
}

export interface SetupRequest extends BaseRequest {
  type: "setup";
  /** The PIN/passphrase chosen by the user. */
  secret: string;
  /** Failed-attempt budget before cryptographic erasure. Default 6. */
  maxAttempts: number;
  /** Must be literally true; the host rejects setup otherwise. */
  acknowledgeNoRecovery: true;
}

export interface UnlockRequest extends BaseRequest {
  type: "unlock";
  secret: string;
}

export interface LockRequest extends BaseRequest {
  type: "lock";
}

export interface EraseRequest extends BaseRequest {
  type: "erase";
  /** Must be literally true to perform the irreversible erase. */
  confirm: true;
}

export type VaultRequest =
  | StatusRequest
  | SetupRequest
  | UnlockRequest
  | LockRequest
  | EraseRequest;

/**
 * Distributive `Omit`. The built-in `Omit` collapses a union to its common
 * keys; this maps over each member so per-variant fields (`secret`, `confirm`,
 * `maxAttempts`) survive.
 */
export type DistributiveOmit<T, K extends keyof never> = T extends unknown
  ? Omit<T, K>
  : never;

/** A request variant with `id` optional (the client fills it in). */
export type RequestWithoutId = DistributiveOmit<VaultRequest, "id"> & {
  id?: number;
};

// ---------------------------------------------------------------------------
// Response messages (host -> extension). Every response echoes `id` and `ok`.
// ---------------------------------------------------------------------------

export interface BaseResponse {
  /** Correlation id echoed from the originating request (numeric, i64). */
  id: number;
  ok: boolean;
}

export interface ErrorResponse extends BaseResponse {
  ok: false;
  error: VaultErrorCode;
  /** Optional human-readable detail. Never assume it is safe to render raw. */
  message?: string;
  /** Present on bad-secret responses. */
  attemptsRemaining?: number;
  /** Present on bad-secret responses; escalating backoff in milliseconds. */
  delayMs?: number;
  /** Present on `erased` errors. */
  state?: VaultState;
}

export interface StatusResponse extends BaseResponse {
  ok: true;
  state: VaultState;
  hardware: Hardware;
  maxAttempts: number;
  attemptsRemaining: number;
}

export interface SetupOkResponse extends BaseResponse {
  ok: true;
  state: "locked";
}

export interface UnlockOkResponse extends BaseResponse {
  ok: true;
  state: "unlocked";
}

export interface LockOkResponse extends BaseResponse {
  ok: true;
  state: "locked";
}

export interface EraseOkResponse extends BaseResponse {
  ok: true;
  state: "erased";
}

export type VaultResponse =
  | StatusResponse
  | SetupOkResponse
  | UnlockOkResponse
  | LockOkResponse
  | EraseOkResponse
  | ErrorResponse;

// ---------------------------------------------------------------------------
// Type guards / validation. Used both by the client and the unit tests.
// ---------------------------------------------------------------------------

/** True if the value is a plausible response envelope (has id + boolean ok). */
export function isVaultResponse(value: unknown): value is VaultResponse {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  // The host's id is an i64 and is always a JSON number (error envelopes for
  // unparseable frames use id 0). Anything else is not a host response.
  return typeof v.id === "number" && Number.isInteger(v.id) && typeof v.ok === "boolean";
}

export function isErrorResponse(r: VaultResponse): r is ErrorResponse {
  return r.ok === false;
}

/** The default failed-attempt budget mandated by the spec. */
export const DEFAULT_MAX_ATTEMPTS = 6;

// ---------------------------------------------------------------------------
// Weak-secret rule (software fallback). Mirrors the host's `weak-secret` rule
// so the UI can reject early. Kept here (protocol module) so both setup.ts and
// the unit tests share a single source of truth.
//
// Rule (software mode only): block all-digit secrets and secrets shorter than
// 12 characters. With TPM2 / Secure Enclave the hardware rate-limits, so the
// PIN-strength requirement is relaxed (the host remains authoritative).
// ---------------------------------------------------------------------------

export interface WeakSecretResult {
  ok: boolean;
  /** Stable reason code when not ok. */
  reason?: "too-short" | "all-digits";
  message?: string;
}

export const MIN_SOFTWARE_SECRET_LENGTH = 12;

/**
 * Evaluate a candidate secret against the software-mode weak-secret rule.
 *
 * @param secret    the candidate passphrase
 * @param hardware  the hardware backing reported by `status`
 */
export function evaluateSecretStrength(
  secret: string,
  hardware: Hardware
): WeakSecretResult {
  // With hardware enforcement the keyspace requirement is satisfied by the
  // hardware lockout, so we do not impose the strong-passphrase rule.
  if (hardware !== "software") {
    return { ok: true };
  }
  if (secret.length < MIN_SOFTWARE_SECRET_LENGTH) {
    return {
      ok: false,
      reason: "too-short",
      message: `Without hardware protection, use at least ${MIN_SOFTWARE_SECRET_LENGTH} characters.`
    };
  }
  if (/^[0-9]+$/.test(secret)) {
    return {
      ok: false,
      reason: "all-digits",
      message:
        "Without hardware protection, an all-digit PIN can be brute-forced offline. Use letters and symbols too."
    };
  }
  return { ok: true };
}

// ---------------------------------------------------------------------------
// Port abstraction. We depend only on the minimal surface of a runtime Port so
// the client is unit-testable with a fake (no `browser.*` in tests).
// ---------------------------------------------------------------------------

export interface PortLike {
  postMessage(message: unknown): void;
  onMessage: {
    addListener(cb: (message: unknown) => void): void;
    removeListener(cb: (message: unknown) => void): void;
  };
  onDisconnect: {
    addListener(cb: (port?: unknown) => void): void;
    removeListener(cb: (port?: unknown) => void): void;
  };
  disconnect(): void;
}

/** Factory that opens a native port. Injected so tests can supply a fake. */
export type PortFactory = () => PortLike;

/** Monotonic id generator. Injected for deterministic tests. */
export type IdGenerator = () => number;

let __seq = 0;
export const defaultIdGenerator: IdGenerator = () => {
  // A plain monotonic counter: ids only need to be unique among this
  // client's in-flight requests, and the host requires a JSON integer
  // (i64). Starts at 1 so the host's id-0 "unparseable frame" envelope
  // never collides with a real pending request.
  __seq += 1;
  return __seq;
};

export interface VaultClientOptions {
  /** Opens the native port. Defaults to connectNative(org.openbook.vault_host). */
  portFactory?: PortFactory;
  /** Correlation id source. */
  idGenerator?: IdGenerator;
  /** Per-request timeout in ms. Default 30s. */
  requestTimeoutMs?: number;
}

/** Native host application name (must match the host manifest `name`). */
export const NATIVE_HOST_NAME = "org.openbook.vault_host";

interface Pending {
  resolve: (r: VaultResponse) => void;
  reject: (e: Error) => void;
  timer: ReturnType<typeof setTimeout> | null;
}

/**
 * Typed client over the native messaging port. Correlates each request with its
 * response by `id`. A single long-lived port is used (connectNative) so the
 * host can push lifecycle and so escalating-delay state survives across calls.
 */
export class VaultClient {
  private port: PortLike | null = null;
  private readonly pending = new Map<number, Pending>();
  private readonly portFactory: PortFactory;
  private readonly idGen: IdGenerator;
  private readonly timeoutMs: number;
  private disconnectHandlers = new Set<() => void>();

  // Bound listeners so we can remove them on disconnect.
  private readonly onMessage = (raw: unknown): void => this.handleMessage(raw);
  private readonly onDisconnect = (): void => this.handleDisconnect();

  constructor(opts: VaultClientOptions = {}) {
    this.portFactory =
      opts.portFactory ??
      (() =>
        (globalThis as { browser?: typeof browser }).browser!.runtime.connectNative(
          NATIVE_HOST_NAME
        ) as unknown as PortLike);
    this.idGen = opts.idGenerator ?? defaultIdGenerator;
    this.timeoutMs = opts.requestTimeoutMs ?? 30_000;
  }

  /** True when a port is currently open. */
  get connected(): boolean {
    return this.port !== null;
  }

  /** Open the native port (idempotent). */
  connect(): void {
    if (this.port) return;
    const port = this.portFactory();
    port.onMessage.addListener(this.onMessage);
    port.onDisconnect.addListener(this.onDisconnect);
    this.port = port;
  }

  /** Register a callback fired when the port disconnects (host crash/exit). */
  onDisconnected(cb: () => void): () => void {
    this.disconnectHandlers.add(cb);
    return () => this.disconnectHandlers.delete(cb);
  }

  private handleMessage(raw: unknown): void {
    if (!isVaultResponse(raw)) {
      // Unknown frame; drop. The host is the only writer and must conform.
      return;
    }
    const entry = this.pending.get(raw.id);
    if (!entry) return; // late/duplicate/unsolicited response
    this.pending.delete(raw.id);
    if (entry.timer) clearTimeout(entry.timer);
    entry.resolve(raw);
  }

  private handleDisconnect(): void {
    const p = this.port;
    this.port = null;
    if (p) {
      p.onMessage.removeListener(this.onMessage);
      p.onDisconnect.removeListener(this.onDisconnect);
    }
    // Fail every in-flight request rather than hang forever (fail-safe).
    const err = new Error("native host disconnected");
    for (const [, entry] of this.pending) {
      if (entry.timer) clearTimeout(entry.timer);
      entry.reject(err);
    }
    this.pending.clear();
    for (const cb of this.disconnectHandlers) {
      try {
        cb();
      } catch {
        /* listener errors must not break teardown */
      }
    }
  }

  /**
   * Send a request and await its correlated response. Auto-connects if needed.
   * Rejects on timeout or host disconnect.
   */
  private send<T extends VaultResponse>(
    req: RequestWithoutId
  ): Promise<T> {
    if (!this.port) this.connect();
    const port = this.port;
    if (!port) {
      return Promise.reject(new Error("vault port unavailable"));
    }
    const id = req.id ?? this.idGen();
    const full = { ...req, id } as VaultRequest;
    return new Promise<T>((resolve, reject) => {
      const timer =
        this.timeoutMs > 0
          ? setTimeout(() => {
              this.pending.delete(id);
              reject(new Error(`vault request '${full.type}' timed out`));
            }, this.timeoutMs)
          : null;
      this.pending.set(id, {
        resolve: resolve as (r: VaultResponse) => void,
        reject,
        timer
      });
      try {
        port.postMessage(full);
      } catch (e) {
        this.pending.delete(id);
        if (timer) clearTimeout(timer);
        reject(e instanceof Error ? e : new Error(String(e)));
      }
    });
  }

  // --- Typed request methods -------------------------------------------------

  status(): Promise<StatusResponse | ErrorResponse> {
    return this.send<StatusResponse | ErrorResponse>({ type: "status" });
  }

  setup(args: {
    secret: string;
    maxAttempts?: number;
  }): Promise<SetupOkResponse | ErrorResponse> {
    return this.send<SetupOkResponse | ErrorResponse>({
      type: "setup",
      secret: args.secret,
      maxAttempts: args.maxAttempts ?? DEFAULT_MAX_ATTEMPTS,
      acknowledgeNoRecovery: true
    });
  }

  unlock(secret: string): Promise<UnlockOkResponse | ErrorResponse> {
    return this.send<UnlockOkResponse | ErrorResponse>({
      type: "unlock",
      secret
    });
  }

  lock(): Promise<LockOkResponse | ErrorResponse> {
    return this.send<LockOkResponse | ErrorResponse>({ type: "lock" });
  }

  erase(): Promise<EraseOkResponse | ErrorResponse> {
    return this.send<EraseOkResponse | ErrorResponse>({
      type: "erase",
      confirm: true
    });
  }

  /** Close the port and reject any in-flight requests. */
  disconnect(): void {
    const p = this.port;
    if (p) p.disconnect();
    // handleDisconnect may or may not fire synchronously depending on impl;
    // ensure cleanup regardless.
    if (this.port) this.handleDisconnect();
  }
}
