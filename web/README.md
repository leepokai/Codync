# Codync website

Next.js static export. `npm run dev` serves it at http://localhost:3000.

## Deploy on Vercel

With the Vercel project's Root Directory set to `web/`, `vercel.json` limits
automatic Git deployments to `main` and `dev`. Other branches, including normal
external PR branches, do not create previews. GitHub Actions tests still run.

This filters branch names, not repository owners. Keep Git Fork Protection
enabled: fork branches named `main` or `dev` still require authorization.
Existing PR checks are not removed; PR branches need to include this
configuration for subsequent events to use it.

See [Vercel Git configuration](https://vercel.com/docs/project-configuration/git-configuration).
