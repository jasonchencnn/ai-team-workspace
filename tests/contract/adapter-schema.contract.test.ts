import { readdirSync, readFileSync } from "node:fs";
import Ajv2020Module, { type AnySchema, type ValidateFunction } from "ajv/dist/2020.js";
import addFormatsModule from "ajv-formats";
import { describe, expect, it } from "vitest";

const adapterSpecDirectory = new URL("../../docs/specs/adapter/v0/", import.meta.url);
const examplesDirectory = new URL("examples/", adapterSpecDirectory);

interface MessageFixture extends Record<string, unknown> {
  params: Record<string, unknown>;
}

function readJson(url: URL): unknown {
  return JSON.parse(readFileSync(url, "utf8")) as unknown;
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

const Ajv2020 = Ajv2020Module.default;
const addFormats = addFormatsModule.default;

function createValidator() {
  const ajv = new Ajv2020({
    allErrors: true,
    strict: true,
    strictRequired: false,
    strictTypes: false,
  });
  addFormats(ajv, ["date-time", "uuid"]);
  ajv.addKeyword({
    keyword: "x-maxUtf8Bytes",
    schemaType: "number",
    type: "string",
    validate: (limit: number, value: string) => Buffer.byteLength(value, "utf8") <= limit,
  });
  ajv.addKeyword({
    keyword: "x-maxJsonUtf8Bytes",
    schemaType: "number",
    type: "object",
    validate: (limit: number, value: unknown) =>
      Buffer.byteLength(JSON.stringify(value), "utf8") <= limit,
  });
  return ajv;
}

function compile(schemaName: string): ValidateFunction {
  const schema = readJson(new URL(schemaName, adapterSpecDirectory)) as AnySchema;
  return createValidator().compile(schema);
}

function expectValid(validate: ValidateFunction, value: unknown): void {
  expect(validate(value), JSON.stringify(validate.errors, null, 2)).toBe(true);
}

function expectInvalid(validate: ValidateFunction, value: unknown): void {
  expect(validate(value), "invalid fixture unexpectedly passed schema validation").toBe(false);
  expect(validate.errors).not.toBeNull();
}

const validateManifest = compile("adapter-manifest.schema.json");
const validateMessage = compile("adapter-message.schema.json");
const manifest = readJson(new URL("manifest.valid.json", examplesDirectory)) as Record<
  string,
  unknown
>;
const initializeRequest = readJson(
  new URL("initialize.request.json", examplesDirectory),
) as MessageFixture;
const runStartRequest = readJson(
  new URL("run.start.request.json", examplesDirectory),
) as MessageFixture;
const heartbeat = readJson(
  new URL("run.heartbeat.notification.json", examplesDirectory),
) as MessageFixture;

describe("Adapter V0 manifest contract", () => {
  it("validates the frozen manifest example", () => {
    expectValid(validateManifest, manifest);
  });

  it("rejects missing required fields and unknown core fields", () => {
    const missing = clone(manifest);
    delete missing.adapter_id;
    expectInvalid(validateManifest, missing);
    expectInvalid(validateManifest, { ...manifest, future_core_field: true });
  });

  it("rejects invalid protocol versions while allowing namespaced extensions", () => {
    expectInvalid(validateManifest, { ...manifest, protocol_versions: ["0.1"] });
    expectValid(validateManifest, {
      ...manifest,
      extensions: { "vendor.example/future-option": { enabled: true } },
    });
  });

  it("accepts canonical SemVer and rejects leading-zero representations", () => {
    expectValid(validateManifest, {
      ...manifest,
      adapter_version: "1.2.3-alpha.1+build.5",
      protocol_versions: ["0.1.0-rc.1+contract.7"],
    });
    expectInvalid(validateManifest, { ...manifest, adapter_version: "00.1.0" });
    expectInvalid(validateManifest, { ...manifest, protocol_versions: ["00.1.0"] });
    expectInvalid(validateManifest, { ...manifest, adapter_version: "1.2.3-01" });
  });
});

describe("Adapter V0 message examples", () => {
  const messageExampleNames = readdirSync(examplesDirectory)
    .filter((name) => name.endsWith(".json") && name !== "manifest.valid.json")
    .sort();

  it.each(messageExampleNames)("validates %s", (name) => {
    expectValid(validateMessage, readJson(new URL(name, examplesDirectory)));
  });
});

describe("Adapter V0 message negative contract", () => {
  it("rejects missing required fields and unknown core fields", () => {
    const missing = clone(initializeRequest);
    delete missing.params.correlation_id;
    expectInvalid(validateMessage, missing);
    expectInvalid(validateMessage, { ...initializeRequest, future_core_field: true });
    expectInvalid(validateMessage, {
      ...initializeRequest,
      params: { ...initializeRequest.params, future_core_field: true },
    });
  });

  it("rejects invalid JSON-RPC versions, protocol versions, and methods", () => {
    expectInvalid(validateMessage, { ...initializeRequest, jsonrpc: "1.0" });
    expectInvalid(validateMessage, {
      ...initializeRequest,
      params: { ...initializeRequest.params, protocol_version: "0.1" },
    });
    expectInvalid(validateMessage, { ...initializeRequest, method: "initialize.future" });
  });

  it("requires canonical lowercase UUIDv7 for every protocol UUID", () => {
    expectInvalid(validateMessage, {
      ...initializeRequest,
      params: { ...initializeRequest.params, harness_instance_id: "not-a-uuid" },
    });
    expectInvalid(validateMessage, {
      ...initializeRequest,
      params: {
        ...initializeRequest.params,
        harness_instance_id: "550e8400-e29b-41d4-a716-446655440000",
      },
    });
    expectInvalid(validateMessage, {
      ...initializeRequest,
      params: {
        ...initializeRequest.params,
        harness_instance_id: "01992D6D-2F31-7B11-8AA8-C975A632C002",
      },
    });
  });

  it("requires valid UTC RFC 3339 timestamps with exactly three milliseconds", () => {
    expectInvalid(validateMessage, {
      ...heartbeat,
      params: { ...heartbeat.params, observed_at: "2026-02-30T12:00:10.000Z" },
    });
    for (const observedAt of [
      "2026-09-21T20:00:10.000+08:00",
      "2026-09-21T12:00:10Z",
      "2026-09-21T12:00:10.000000Z",
    ]) {
      expectInvalid(validateMessage, {
        ...heartbeat,
        params: { ...heartbeat.params, observed_at: observedAt },
      });
    }
  });

  it("accepts canonical SemVer in message version fields and rejects leading zeros", () => {
    expectValid(validateMessage, {
      ...initializeRequest,
      params: {
        ...initializeRequest.params,
        protocol_version: "0.1.0-rc.1+contract.7",
      },
    });
    expectInvalid(validateMessage, {
      ...initializeRequest,
      params: { ...initializeRequest.params, protocol_version: "00.1.0" },
    });
  });

  it("rejects non-positive binding versions, fencing tokens, and sequences", () => {
    for (const field of ["binding_version", "fencing_token", "sequence"] as const) {
      expectInvalid(validateMessage, {
        ...heartbeat,
        params: { ...heartbeat.params, [field]: 0 },
      });
    }
  });

  it("requires run.output to carry content or an artifact", () => {
    const outputBase = clone(heartbeat);
    outputBase.method = "run.output";
    outputBase.params.kind = "stdout";
    delete outputBase.params.phase;
    delete outputBase.params.progress_percent;

    expectValid(validateMessage, {
      ...outputBase,
      params: { ...outputBase.params, content: "contract output" },
    });
    expectValid(validateMessage, {
      ...outputBase,
      params: {
        ...outputBase.params,
        kind: "artifact_candidate",
        artifact: { kind: "artifact", uri: "artifacts/contract-result.json" },
      },
    });
    expectInvalid(validateMessage, outputBase);
    expectInvalid(validateMessage, {
      ...outputBase,
      params: { ...outputBase.params, content: null },
    });
    expectInvalid(validateMessage, {
      ...outputBase,
      params: { ...outputBase.params, content: "" },
    });
    expectInvalid(validateMessage, {
      ...outputBase,
      params: { ...outputBase.params, kind: "artifact_candidate", artifact: null },
    });
    expectInvalid(validateMessage, {
      ...outputBase,
      params: {
        ...outputBase.params,
        kind: "artifact_candidate",
        content: "not an artifact reference",
      },
    });
    expectInvalid(validateMessage, {
      ...outputBase,
      params: {
        ...outputBase.params,
        kind: "artifact_candidate",
        artifact: { kind: "report", uri: "reports/contract-result.json" },
      },
    });
    expectInvalid(validateMessage, {
      ...outputBase,
      params: {
        ...outputBase.params,
        kind: "stdout",
        artifact: { kind: "artifact", uri: "artifacts/contract-result.json" },
      },
    });
  });

  it("enforces run.output content limits in UTF-8 bytes", () => {
    const outputBase = clone(heartbeat);
    outputBase.method = "run.output";
    outputBase.params.kind = "stdout";
    delete outputBase.params.phase;
    delete outputBase.params.progress_percent;
    const exactly64KiB = `${"界".repeat(21_845)}a`;

    expect(Buffer.byteLength(exactly64KiB, "utf8")).toBe(65_536);
    expectValid(validateMessage, {
      ...outputBase,
      params: { ...outputBase.params, content: exactly64KiB },
    });
    expectInvalid(validateMessage, {
      ...outputBase,
      params: { ...outputBase.params, content: `${exactly64KiB}a` },
    });
  });

  it("enforces the complete JSON message limit in UTF-8 bytes", () => {
    const messageAtLimit = clone(initializeRequest);
    messageAtLimit.params.extensions = { padding: "" };
    const baseBytes = Buffer.byteLength(JSON.stringify(messageAtLimit), "utf8");
    (messageAtLimit.params.extensions as Record<string, unknown>).padding = "a".repeat(
      1_048_576 - baseBytes,
    );

    expect(Buffer.byteLength(JSON.stringify(messageAtLimit), "utf8")).toBe(1_048_576);
    expectValid(validateMessage, messageAtLimit);

    const messageOverLimit = clone(messageAtLimit);
    const overLimitExtensions = messageOverLimit.params.extensions as Record<string, unknown>;
    if (typeof overLimitExtensions.padding !== "string") {
      throw new TypeError("test fixture padding must be a string");
    }
    overLimitExtensions.padding = `${overLimitExtensions.padding}a`;
    expect(Buffer.byteLength(JSON.stringify(messageOverLimit), "utf8")).toBe(1_048_577);
    expectInvalid(validateMessage, messageOverLimit);
  });

  it("enforces the error envelope and rejects unknown error data", () => {
    const errorResponse = {
      jsonrpc: "2.0",
      id: null,
      error: {
        code: -32014,
        message: "Run fenced",
        data: {
          domain_code: "RUN_FENCED",
          retryable: false,
          correlation_id: "01992d6d-2f31-7b11-8aa8-c975a632c016",
          reason_key: "stale_token",
        },
      },
    };
    expectValid(validateMessage, errorResponse);
    for (const [code, domainCode, retryable] of [
      [-32010, "ADAPTER_INCOMPATIBLE", false],
      [-32011, "ADAPTER_CAPABILITY_MISSING", false],
      [-32012, "ADAPTER_AUTHENTICATION_FAILED", false],
      [-32013, "BINDING_STALE_VERSION", false],
      [-32014, "RUN_FENCED", false],
      [-32015, "RUN_OUTPUT_OUT_OF_ORDER", true],
      [-32016, "WORKSPACE_PATH_OUTSIDE_SCOPE", false],
      [-32017, "ADAPTER_UNAVAILABLE", true],
      [-32018, "CONCURRENCY_IDEMPOTENCY_CONFLICT", false],
      [-32099, "ADAPTER_PROTOCOL_VIOLATION", false],
    ] as const) {
      expectValid(validateMessage, {
        ...errorResponse,
        error: {
          ...errorResponse.error,
          code,
          data: { ...errorResponse.error.data, domain_code: domainCode, retryable },
        },
      });
    }

    const missing = clone(errorResponse);
    delete (missing.error.data as Partial<typeof errorResponse.error.data>).domain_code;
    expectInvalid(validateMessage, missing);
    expectInvalid(validateMessage, {
      ...errorResponse,
      error: {
        ...errorResponse.error,
        data: { ...errorResponse.error.data, raw_stderr: "secret" },
      },
    });
    const missingData = clone(errorResponse);
    delete (missingData.error as Partial<typeof errorResponse.error>).data;
    expectInvalid(validateMessage, missingData);
    expectInvalid(validateMessage, {
      ...errorResponse,
      error: { ...errorResponse.error, code: -32098 },
    });
    expectInvalid(validateMessage, {
      ...errorResponse,
      error: {
        ...errorResponse.error,
        data: { ...errorResponse.error.data, domain_code: "ADAPTER_UNAVAILABLE" },
      },
    });
    expectInvalid(validateMessage, {
      ...errorResponse,
      error: {
        ...errorResponse.error,
        data: { ...errorResponse.error.data, retryable: true },
      },
    });
  });

  it("keeps JSON-RPC standard errors on an independent compatible branch", () => {
    expectValid(validateMessage, {
      jsonrpc: "2.0",
      id: null,
      error: { code: -32700, message: "Parse error" },
    });
    expectValid(validateMessage, {
      jsonrpc: "2.0",
      id: "request-1",
      error: {
        code: -32602,
        message: "Invalid params",
        data: { path: "params", expected: "object" },
      },
    });
  });

  it("allows unknown data only inside declared extensions", () => {
    expectValid(validateMessage, {
      ...runStartRequest,
      params: {
        ...runStartRequest.params,
        extensions: { "vendor.example/future-option": { enabled: true } },
      },
    });
    expectInvalid(validateMessage, {
      ...runStartRequest,
      params: { ...runStartRequest.params, future_core_field: true },
    });
  });
});
