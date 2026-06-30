# AI Handoff — Re-clone `vibervn-context-engine` + integrate 9Router OpenAI-compatible embeddings

Date: 2026-06-30

## Goal

Clone and re-study `vibervn-context-engine`, then make/verify it can use the user's local `9router` as an OpenAI-compatible embedding endpoint.

Target result:

- `vibervn-context-engine` can index/query code using embeddings routed through 9Router.
- No Voyage/OpenAI cloud API required if 9Router is configured with a local/free embedding provider.
- Final docs/config explain exact settings: endpoint, model, API key behavior, pitfalls.

## Inputs

### Target repo to clone

- GitHub: `https://github.com/nullmastermind/vibervn-context-engine`

### 9Router local source to inspect

- Path: `C:\Users\user\Desktop\VSCode\NextJS\9router`

### 9Router expected runtime URLs

- Dashboard: `http://localhost:20128/dashboard`
- OpenAI-compatible base URL: `http://localhost:20128/v1`
- Embeddings endpoint: `POST http://localhost:20128/v1/embeddings`

Do not assume secrets. Ask user to provide/copy API key from 9Router dashboard only if required.

## Known 9Router facts already verified

9Router is a Next.js local routing gateway exposing OpenAI-compatible routes.

Relevant files:

- `next.config.mjs`
  - rewrites `/v1/:path*` → `/api/v1/:path*`
  - so external clients should use base URL `http://localhost:20128/v1`
- `src/app/api/v1/embeddings/route.js`
  - implements `POST /v1/embeddings`
  - calls `handleEmbeddings(request)`
- `src/sse/handlers/embeddings.js`
  - validates JSON body
  - requires `model`
  - requires `input`
  - extracts Bearer API key
  - enforces API key only if 9Router setting `requireApiKey` is true
  - resolves model via `getModelInfo(modelStr)`
  - routes to provider credentials + fallback loop
- `open-sse/handlers/embeddingsCore.js`
  - validates `input` as string or string array
  - resolves provider adapter with `getEmbeddingAdapter(provider)`
  - builds URL/headers/body
  - forwards upstream request
  - normalizes response to OpenAI-style JSON
- `open-sse/handlers/embeddingProviders/index.js`
  - supports built-in OpenAI-compatible embedding providers
  - supports provider IDs starting with `openai-compatible-` or `custom-embedding-`
- `open-sse/handlers/embeddingProviders/openaiCompatNode.js`
  - builds URL from `creds.providerSpecificData.baseUrl`
  - strips trailing slash and trailing `/embeddings`
  - final upstream URL = `${baseUrl}/embeddings`
- `src/sse/services/model.js`
  - custom provider node prefixes can resolve model strings like `<prefix>/<model>`
  - custom embedding provider nodes use type `custom-embedding`
- `src/app/api/provider-nodes/route.js`
  - supports `type: "custom-embedding"`
  - accepts `name`, `prefix`, `baseUrl`
  - strips trailing `/embeddings` from base URL

Important 9Router request shape:

```http
POST http://localhost:20128/v1/embeddings
Authorization: Bearer <9router-api-key-or-dummy-if-not-required>
Content-Type: application/json

{
  "model": "<9router-model-string>",
  "input": ["code chunk 1", "code chunk 2"],
  "encoding_format": "float"
}
```

Model string examples depend on 9Router configuration:

- Built-in/provider route: `openai/text-embedding-3-small`, `jina-ai/jina-embeddings-v2-base-code`, etc. if connected/enabled.
- Custom embedding node route: `<custom-prefix>/<embedding-model>`, e.g. `emb/nomic-embed-text`.

Use a real embedding model, not a chat model.

## Known `vibervn-context-engine` facts from prior research

Verify all of this again from the fresh clone.

- Rust app/crate; package previously observed as `context-engine-rs`.
- Server stack: Axum/Tokio.
- Storage: embedded SurrealDB/RocksDB per repo.
- Parsing/indexing: Tree-sitter, file watcher, incremental indexing.
- Retrieval: semantic embeddings + vector index + optional graph expansion/rerank.
- MCP: exposes tools such as codebase/file retrieval.
- Prior files to inspect carefully:
  - `Cargo.toml`
  - `src/main.rs`
  - `src/server.rs`
  - `src/config.rs`
  - `src/embedding/voyage.rs`
  - `src/indexing/**`
  - `src/query/**`
  - `src/store/**`
  - `src/mcp.rs`
  - `README*.md`
- Prior observation: embedding code had Voyage/OpenAI-compatible concepts and constants similar to `VOYAGE_ENDPOINT` / `OPENAI_ENDPOINT`. Verify current source.

## Required investigation

1. Clone `https://github.com/nullmastermind/vibervn-context-engine` fresh.
2. Read repo docs and source, not only README.
3. Map exact embedding config flow:
   - UI/API config fields
   - persisted config format
   - env vars
   - default endpoints
   - model name handling
   - request URL construction
   - request body format
   - response parsing
   - vector dimension assumptions
4. Inspect local 9Router source at `C:\Users\user\Desktop\VSCode\NextJS\9router`, especially files listed above.
5. Determine whether `vibervn` already supports arbitrary OpenAI-compatible embedding endpoint.
6. If already supported:
   - add concise docs/config example for 9Router
   - add validation/smoke test if feasible
7. If not fully supported:
   - implement minimal support without breaking Voyage/OpenAI defaults
   - prefer config/env support over hardcoding
   - keep endpoint base URL configurable
   - avoid adding secrets to repo

## Integration target

Preferred user-facing config for `vibervn`:

```text
Embedding provider/type: OpenAI-compatible
Embedding base URL: http://localhost:20128/v1
Embedding endpoint used by code: http://localhost:20128/v1/embeddings
Embedding API key: <9Router key from dashboard> OR dummy if 9Router requireApiKey=false
Embedding model: <9Router embedding model string, e.g. emb/nomic-embed-text>
```

Critical: if `vibervn` appends `/embeddings`, user config should be base URL `http://localhost:20128/v1`, not `http://localhost:20128/v1/embeddings`.

## Acceptance criteria

- Fresh clone builds/tests per repo instructions.
- Embedding request from `vibervn` to 9Router is OpenAI-compatible:
  - `POST /v1/embeddings`
  - `Authorization: Bearer ...`
  - JSON body includes `model` and `input`
  - response parser handles OpenAI embedding response `{ object, data: [{ embedding, index }], usage }`
- User can configure 9Router endpoint without editing source code.
- Docs explain exact 9Router setup and model string examples.
- Changing embedding model/dimensions warns user to re-index.
- Existing Voyage/OpenAI behavior remains backward-compatible.
- No credentials/API keys committed.

## Suggested tests

At minimum:

1. Unit test URL construction:
   - base `http://localhost:20128/v1` → `http://localhost:20128/v1/embeddings`
   - avoid double `/embeddings/embeddings`
2. Unit/integration test using a mock OpenAI-compatible embedding server:
   - return fixed embedding vectors
   - verify parse + index path works
3. Manual smoke:
   - Start 9Router locally.
   - Configure one embedding provider/model in 9Router.
   - Configure `vibervn` to base URL `http://localhost:20128/v1`.
   - Index a small repo.
   - Query through MCP/codebase retrieval.

## Pitfalls

- Chat completions compatibility is not enough; embeddings require `POST /v1/embeddings`.
- Wrong model type causes bad vectors/errors. Use embedding model.
- Wrong base URL can create double path:
  - Bad if app appends `/embeddings`: `http://localhost:20128/v1/embeddings/embeddings`
  - Good: `http://localhost:20128/v1`
- If 9Router `requireApiKey=true`, dummy key fails. Use dashboard-generated key.
- If embedding dimension changes, existing vector index may become invalid. Re-index.
- Do not expose 9Router publicly without auth.

## Deliverables

- Summary of source-code findings.
- Code changes only if required.
- Config/docs example for using 9Router.
- Test or smoke-test evidence.
- Clear final instructions for the user.
