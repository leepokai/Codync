"""Source: Hermes agent/delegation_provider.py and plugins/delegation/default (MIT, 10d25f8a).
Adaptation: user-approved provider/model choices, addressed by opaque per-task route ids.
"""

from __future__ import annotations

import copy

NAME = "codync-routes"


def install(routes: list[dict]) -> None:
    from agent.delegation_provider import DelegationProvider, DelegationRouteError, ROUTE_KEYS
    from agent.delegation_registry import register_provider, get_provider

    if get_provider(NAME) is not None:
        raise ValueError("A native plugin already registered the Codync route name")

    chosen = {route["id"]: copy.deepcopy(route) for route in routes}
    if len(chosen) != len(routes):
        raise ValueError("Subagent route ids must be unique")

    class CodyncRoutes(DelegationProvider):
        @property
        def name(self):
            return NAME

        @property
        def display_name(self):
            return "Codync saved routes"

        def is_available(self):
            return True

        def task_parameters(self, config):
            descriptions = [f"{route['id']}: {route['label']} ({route['provider'] or 'inherited provider'} / {route['model'] or 'inherited model'})"
                            for route in chosen.values()]
            return {"codync_route": {
                "type": "string", "enum": ["default", *chosen],
                "description": "Choose a route saved by the user in this bot's settings. default inherits the configured subagent route. " + "; ".join(descriptions),
            }}

        def description(self, config):
            return "Each task may select codync_route from the user's saved choices. Native Hermes owns credentials, permissions, isolation, child iteration budgets and concurrency."

        def route_task(self, task, config):
            # Raw model/provider/endpoint fields remain unavailable to the model, as required
            # by Hermes's provider contract. The model can select only a user-saved route id.
            result = {key: copy.deepcopy(value) for key, value in config.items()
                      if key in ROUTE_KEYS and key != "service_tier"}
            selected = task.get("codync_route", "default")
            if selected == "default":
                return result
            route = chosen.get(selected)
            if route is None:
                raise DelegationRouteError("Choose one of this bot's saved codync_route ids; no subagent was started")
            result.pop("profile", None)
            if route["provider"]:
                result["provider"] = route["provider"]
                # A different provider must resolve its own endpoint, identity and recovery
                # policy instead of carrying another route's endpoint or credential hints.
                for key in ("api_key", "base_url", "api_mode", "request_overrides", "fallback_providers"):
                    result.pop(key, None)
                result["fallback_providers"] = []
            if route["model"]:
                result["model"] = route["model"]
            result["label"] = route["label"]
            return result

    register_provider(CodyncRoutes())
