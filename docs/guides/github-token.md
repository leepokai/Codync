# GitHub token

Workflow outputs act on the user's behalf: they comment, label, assign, request a review and open
issues. They need a GitHub token on the computer where the host runs.

## Creating it

1. GitHub → **Settings → Developer settings → Personal access tokens → Fine-grained tokens**.
2. Limit it to the repositories the workflows act on, and give it **Issues: Read and write**,
   **Pull requests: Read and write**, **Contents: Read** and **Metadata: Read**.
3. Copy it, then paste it in **Settings → GitHub** on the computer.

The host checks the token with `GET /user` before saving it and stores it in the encrypted vault;
the settings screen shows the account and the scopes GitHub reports, never the token. Revoking the
token in GitHub then disconnecting here removes it.

A classic token works too, though it is broader. A token whose account can see only the chosen
repositories keeps a compromised host to those.
