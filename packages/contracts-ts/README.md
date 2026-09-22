# Generated TypeScript contracts

`index.ts` and `schema/*.json` are generated from Rust DTOs in
`crates/contracts` through Schemars. Do not edit generated files by hand.

- Regenerate: `pnpm contracts:generate`
- Verify committed output has no drift: `pnpm contracts:check`

Frontend code must import these types instead of declaring duplicate DTOs.
