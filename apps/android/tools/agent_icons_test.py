"""Self-check for agent_icons.py's path rounding: python3 tools/agent_icons_test.py"""

import importlib.util
import pathlib

spec = importlib.util.spec_from_file_location("agent_icons", pathlib.Path(__file__).with_name("agent_icons.py"))
icons = importlib.util.module_from_spec(spec)
spec.loader.exec_module(icons)

cases = {
    # A decimal that rounds to a whole number keeps the next `.05` a number of its own.
    "M3.392-7.003.05": "M3.39-7 0.05",
    "M1.5.5": "M1.5 0.5",
    # Arc flags are single digits that may touch the next number.
    "a.306.306 0 01.415-.287": "a0.31 0.31 0 0 1 0.41-0.29",
    "A1 1 0 1112 13a1 1 0 0 0 2 3": "A1 1 0 1 1 12 13a1 1 0 0 0 2 3",
    # Exponents, repeated commands and closing paths.
    "L3.3e-05 5zM0 0h24v24H0z": "L0 5zM0 0h24v24H0z",
}
for path, expected in cases.items():
    assert icons.compact(path, 24) == expected, (path, icons.compact(path, 24))
print("agent_icons: ok")
