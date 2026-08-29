---
name: sonarcloud-api-workflow
description: "How to bulk-disposition SonarCloud findings for this repo (token, endpoints, transition names)"
metadata: 
  node_type: memory
  type: reference
  originSessionId: 75432df4-df4a-4f5b-877b-4dfb24b0dc4c
  modified: 2026-08-29T09:09:55.637Z
---

The session environment exports `SONARQUBE_TOKEN` (also read by the MCP server
declared in `.mcp.json`). For bulk work the web API beats the MCP tools because
it takes comments and loops cheaply:

- Search: `GET https://sonarcloud.io/api/issues/search?projects=rubentalstra_Veredictum&issueStatuses=OPEN,CONFIRMED&ps=500` (add `additionalFields=comments` to see existing comments).
- Reason first, then status: `POST /api/issues/add_comment` (`issue`, `text`), then `POST /api/issues/do_transition` (`issue`, `transition=accept|falsepositive|reopen`). `accept` works on SonarCloud (returns `issueStatus: ACCEPTED`).
- Auth header: `Authorization: Bearer $SONARQUBE_TOKEN`.

**Why:** issue #154 requires every non-fix to be an in-dashboard acceptance
with its recorded reason; the MCP change-status tool carries no comment field.

**How to apply:** loop add_comment + do_transition per key; make the loop
idempotent (query `issueStatus` and existing comments first) because a 54-item
loop can outlive a 2-minute foreground timeout. The 2026-08-29 adjudication
(75 × rust:S3776: 54 accepted, 21 refactored) is recorded on [[project]] issue
#154 with per-finding comments in the dashboard.
