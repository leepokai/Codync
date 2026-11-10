# MiniMax Code

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Always allow is ignored: it asks again before the next command of the same kind, so one task asked the user 24 times, 23 of them answered Always allow | `host/src/agent/bot/always_allowed.rs`: after an Always allow, Codync answers the session's next requests of that kind of action itself | No issue opened yet: MiniMax Code has no public tracker we found | It stops asking after Always allow |
