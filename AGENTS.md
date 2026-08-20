# AGENTS.md

This project runs on pure Node.js with its built-in TypeScript support (type stripping). Run the app directly with `node` — there is no build step.

## Tooling

- **Runtime:** Node.js executing TypeScript files natively.
- **Package manager:** pnpm.
- **Linter / formatter:** Biome.
- **Testing:** the official Node.js test runner (`node --test`).

## Import aliases

Module imports use the official `#` package-imports feature. The `#src/*` alias maps to `./src/*`, so import application code as:

```ts
import { something } from "#src/path/to/module";
```

## Scripts

- `pnpm start` — run the app (`node src/index.ts`)
- `pnpm test` — run the test suite (`node --test`)
- `pnpm check` — lint and format check with Biome
- `pnpm typecheck` — type-check only (`tsc --noEmit`)
