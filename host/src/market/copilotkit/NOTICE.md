# CopilotKit OpenBot connector catalogue

The app listings under this folder are ported from the connector catalogue of
[CopilotKit/OpenBot](https://github.com/CopilotKit/OpenBot), `server/src/plugins/catalogue.ts`
(MIT License, Copyright CopilotKit). This is the source of these entries: the `OpenBot` copy next
door (`market::openbot`) came from `nightly-labs/openbot` and is a different project.

What is kept from upstream, because it is the contract and not the branding:

- the entry key, title, vendor, host and path each server is reached at;
- the auth kind (`none`, `deployment-bearer`, `user-oauth`, `builtin`), the header a deployment
  key goes in, the OAuth endpoints, scopes, `authorizationParams`, the dynamic registration
  endpoint, and the issuer a consent must come back from;
- the reviewed `writeTools` / `readTools` lists;
- the vendor documentation link.

What Codync writes itself is only the presentation: category, tagline, description, example
prompts and the sheet's field labels. The importer that regenerates `apps.json` from a CopilotKit
checkout is `tools/import-copilotkit-connectors.mjs`.

The full MIT licence is in `LICENSE-MIT.txt`.
