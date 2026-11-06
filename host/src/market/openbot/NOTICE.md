# OpenBot marketplace content

The agents, skills and app listings under this folder are imported from the OpenBot marketplace
(Apache License 2.0, Copyright 2026 Synthetify Labs sp. z o.o.). Codync keeps the text and adds the
fields its own catalog uses. The import script is `tools/import-openbot-catalog.mjs`.

Individual skills carry their own upstream source and licence:

- `secure-code-guidance`, `repository-threat-model`: openai/skills (Apache-2.0).
- `distinctive-frontend-design`, `internal-communications`, `decision-quality-check`: anthropics/skills.
- `pstack-*`: pstack by Lauren Tan, MIT License. The licence and notice sit beside
  `skills/pstack-poteto-mode`.

The full Apache License is in `LICENSE-APACHE-2.0.txt`.
