import type { IncomingMessage, ServerResponse } from "node:http";
import {
  buildSchema,
  execute,
  getOperationAST,
  parse,
  specifiedRules,
  validate,
  valueFromASTUntyped,
  type DocumentNode,
  type ArgumentNode,
  type FragmentDefinitionNode,
  type SelectionSetNode,
} from "graphql";
import type { Credential, CredentialStore } from "../credentials/credentialStore.js";
import type { EventStore } from "../bridge/eventStore.js";
import { readRequestBody, RequestBodyTooLargeError } from "../http/requestBody.js";

export const MAX_GRAPHQL_COMPLEXITY = 100;
const MAX_QUERY_BYTES = 64 * 1024;
const MAX_LIST_SIZE = 100;

const schema = buildSchema(`
  scalar JSON

  type Credential {
    id: ID!
    holderId: String!
    type: String!
    issuedAt: Float!
    expiresAt: Float!
    issuer: String!
    status: String!
    metadata: JSON!
    verification: Verification
  }

  type Verification {
    credentialId: String!
    holderId: String!
    documentType: String!
    verificationStatus: String!
    verifiedAt: Float
    verificationExpiresAt: Float!
    verifier: String
    rejectionReason: String
    nextVerificationRequired: Float!
  }

  type CredentialStats {
    total: Int!
    verified: Int!
    pending: Int!
    needsReVerification: Int!
    verificationRate: Float!
  }

  type IndexedEvent {
    id: Int!
    ledger: Int!
    ledgerClosedAt: String!
    txHash: String!
    contractId: String!
    category: String!
    action: String!
    value: JSON!
  }

  type Query {
    credential(id: ID!): Credential
    credentials(holderId: String, limit: Int = 50): [Credential!]!
    verification(credentialId: ID!): Verification
    verificationStats(holderId: ID!): CredentialStats!
    events(limit: Int = 50): [IndexedEvent!]!
  }
`);

export interface GraphqlContext {
  credentials: CredentialStore;
  events: EventStore;
}

export interface GraphqlRequest {
  query: string;
  variables?: Record<string, unknown>;
  operationName?: string;
}

export async function executeGraphqlRequest(
  request: GraphqlRequest,
  context: GraphqlContext
): Promise<{ data?: unknown; errors?: Array<{ message: string }> }> {
  let document: DocumentNode;
  try {
    document = parse(request.query);
  } catch (error) {
    return { errors: [{ message: error instanceof Error ? error.message : "invalid GraphQL query" }] };
  }

  const validationErrors = validate(schema, document, specifiedRules);
  if (validationErrors.length > 0) {
    return { errors: validationErrors.map((error) => ({ message: error.message })) };
  }

  const operation = getOperationAST(document, request.operationName);
  if (!operation) return { errors: [{ message: "operationName is required for this document" }] };
  const complexity = queryComplexity(document, operation.selectionSet, request.variables ?? {});
  if (complexity > MAX_GRAPHQL_COMPLEXITY) {
    return { errors: [{ message: `query complexity ${complexity} exceeds limit ${MAX_GRAPHQL_COMPLEXITY}` }] };
  }

  const result = await execute({
    schema,
    document,
    rootValue: {
      credential: ({ id }: { id: string }) => {
        const credential = context.credentials.getCredential(id);
        return credential ? withVerification(credential, context.credentials) : null;
      },
      credentials: ({ holderId, limit }: { holderId?: string; limit?: number }) => {
        const boundedLimit = checkedLimit(limit);
        const credentials = holderId
          ? context.credentials.getCredentialsForHolder(holderId)
          : context.credentials.getAllCredentials();
        return credentials
          .slice(0, boundedLimit)
          .map((credential) => withVerification(credential, context.credentials));
      },
      verification: ({ credentialId }: { credentialId: string }) =>
        context.credentials.getVerification(credentialId) ?? null,
      verificationStats: ({ holderId }: { holderId: string }) =>
        context.credentials.getVerificationStats(holderId),
      events: ({ limit }: { limit?: number }) => context.events.getRecentEvents(checkedLimit(limit)),
    },
    variableValues: request.variables,
    operationName: request.operationName,
  });
  return {
    ...(result.data === undefined ? {} : { data: result.data }),
    ...(result.errors?.length
      ? { errors: result.errors.map((error) => ({ message: error.message })) }
      : {}),
  };
}

function withVerification(
  credential: Credential,
  store: CredentialStore
) {
  return {
    ...credential,
    verification: store.getVerification(credential.id) ?? null,
  };
}

export function queryComplexity(
  document: DocumentNode,
  selectionSet: SelectionSetNode,
  variables: Record<string, unknown>
): number {
  const fragments = new Map<string, FragmentDefinitionNode>();
  for (const definition of document.definitions) {
    if (definition.kind === "FragmentDefinition") {
      fragments.set(definition.name.value, definition);
    }
  }
  return selectionComplexity(selectionSet, 1, variables, fragments, new Set());
}

function selectionComplexity(
  selectionSet: SelectionSetNode,
  multiplier: number,
  variables: Record<string, unknown>,
  fragments: Map<string, FragmentDefinitionNode>,
  fragmentPath: Set<string>
): number {
  return selectionSet.selections.reduce((total, selection) => {
    if (selection.kind === "Field") {
      const childMultiplier =
        selection.name.value === "credentials" || selection.name.value === "events"
          ? multiplier * listLimit(selection.arguments, variables)
          : multiplier;
      return (
        total +
        multiplier +
        (selection.selectionSet
          ? selectionComplexity(
              selection.selectionSet,
              childMultiplier,
              variables,
              fragments,
              fragmentPath
            )
          : 0)
      );
    }
    if (selection.kind === "InlineFragment") {
      return (
        total +
        selectionComplexity(
          selection.selectionSet,
          multiplier,
          variables,
          fragments,
          fragmentPath
        )
      );
    }
    const name = selection.name.value;
    const fragment = fragments.get(name);
    if (!fragment || fragmentPath.has(name)) return total;
    const nextPath = new Set(fragmentPath);
    nextPath.add(name);
    return (
      total +
      selectionComplexity(
        fragment.selectionSet,
        multiplier,
        variables,
        fragments,
        nextPath
      )
    );
  }, 0);
}

function listLimit(
  args: readonly ArgumentNode[] | undefined,
  variables: Record<string, unknown>
): number {
  const argument = args?.find((item) => item.name.value === "limit");
  if (!argument) return 50;
  const value = valueFromASTUntyped(argument.value, variables);
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(MAX_LIST_SIZE, Math.max(1, Math.floor(value)))
    : 50;
}

function checkedLimit(limit: number | undefined): number {
  if (limit === undefined) return 50;
  if (!Number.isInteger(limit) || limit < 1 || limit > MAX_LIST_SIZE) {
    throw new Error(`limit must be an integer between 1 and ${MAX_LIST_SIZE}`);
  }
  return limit;
}

export function handleGraphqlHttpRequest(
  req: IncomingMessage,
  res: ServerResponse,
  context: GraphqlContext
): void {
  if (req.method !== "POST") {
    res.writeHead(405, { "content-type": "application/json", allow: "POST" });
    res.end(JSON.stringify({ errors: [{ message: "only POST is supported" }] }));
    return;
  }
  void (async () => {
    try {
      const body = await readRequestBody(req, MAX_QUERY_BYTES);
      const parsed: unknown = JSON.parse(body.toString("utf8"));
      if (
        typeof parsed !== "object" ||
        parsed === null ||
        Array.isArray(parsed) ||
        typeof (parsed as Record<string, unknown>).query !== "string"
      ) {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ errors: [{ message: "query must be a string" }] }));
        return;
      }
      const requestBody = parsed as Record<string, unknown>;
      if (
        (requestBody.variables !== undefined &&
          (typeof requestBody.variables !== "object" ||
            requestBody.variables === null ||
            Array.isArray(requestBody.variables))) ||
        (requestBody.operationName !== undefined &&
          typeof requestBody.operationName !== "string")
      ) {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            errors: [{ message: "variables must be an object and operationName a string" }],
          })
        );
        return;
      }
      const input = parsed as GraphqlRequest;
      const result = await executeGraphqlRequest(input, context);
      res.writeHead(result.errors ? 400 : 200, { "content-type": "application/json" });
      res.end(JSON.stringify(result));
    } catch (error) {
      const status =
        error instanceof RequestBodyTooLargeError
          ? 413
          : error instanceof SyntaxError
            ? 400
            : 500;
      if (status === 500) {
        console.error("[quorum-credit] GraphQL request failed", error);
      }
      res.writeHead(status, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          errors: [
            {
              message:
                status === 413
                  ? "GraphQL request body is too large"
                  : status === 400
                    ? "invalid GraphQL request body"
                    : "internal server error",
            },
          ],
        })
      );
    }
  })();
}
