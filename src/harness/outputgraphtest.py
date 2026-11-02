#!/usr/bin/env python3
"""Check actual PipeWire links, rather than only the bridge's selected label."""
import json
import subprocess
import sys

graph = json.loads(subprocess.check_output(["pw-dump"], text=True))
names = {
    item["id"]: item.get("info", {}).get("props", {}).get("node.name")
    for item in graph
    if item["type"] == "PipeWire:Interface:Node"
}
renders = {ident for ident, name in names.items() if name == "saucefx.render"}
targets = {
    names.get(item["info"]["input-node-id"])
    for item in graph
    if item["type"] == "PipeWire:Interface:Link"
    and item["info"]["output-node-id"] in renders
}
expected = sys.argv[1]
if expected == "none":
    sys.exit(0 if not renders and not targets else 1)
sys.exit(0 if targets == {expected} else 1)
