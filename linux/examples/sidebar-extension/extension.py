#!/usr/bin/env python3

import json
import sys


def text(value, font=None, color=None, weight=None):
    node = {"type": "text", "text": value}
    if font:
        node["font"] = font
    if color:
        node["color"] = color
    if weight:
        node["weight"] = weight
    return node


request = json.load(sys.stdin)
snapshot = request.get("snapshot", {})
workspaces = snapshot.get("workspaces", [])
selected_id = snapshot.get("selectedWorkspaceID")

children = [
    text("Extension Workspaces", font="headline"),
    {
        "type": "hstack",
        "spacing": 6,
        "children": [
            {
                "type": "button",
                "title": "Previous",
                "action": {
                    "type": "extension",
                    "params": {"action": "selectPreviousWorkspace"},
                },
            },
            {
                "type": "button",
                "title": "Next",
                "action": {
                    "type": "extension",
                    "params": {"action": "selectNextWorkspace"},
                },
            },
        ],
    },
    {"type": "divider"},
]

for workspace in workspaces:
    workspace_id = workspace.get("id", "")
    title = workspace.get("title") or "Workspace"
    unread = int(workspace.get("unreadCount", 0))
    row = {
        "type": "hstack",
        "spacing": 6,
        "children": [
            text(title, weight="bold" if workspace_id == selected_id else None),
            {"type": "spacer"},
        ],
    }
    if unread:
        row["children"].append(text(str(unread), color="#d1495b"))
    children.append(
        {
            "type": "button",
            "children": [row],
            "action": {
                "type": "extension",
                "params": {
                    "action": "selectWorkspace",
                    "workspace_id": workspace_id,
                },
            },
        }
    )

json.dump(
    {
        "protocolVersion": 1,
        "document": {
            "version": 1,
            "root": {
                "type": "vstack",
                "spacing": 6,
                "children": children,
            },
        },
    },
    sys.stdout,
)
