# Local Meetings MCP

OpenFlow stores meeting transcripts and summaries at:

- macOS: `~/Library/Application Support/OpenFlow/meetings.json`
- Windows: `%APPDATA%/OpenFlow/meetings.json`
- Linux: `$XDG_DATA_HOME/OpenFlow/meetings.json` (or `~/.local/share/OpenFlow/meetings.json`)

The Jac MCP server reads the same file and serves `list_meetings`,
`search_meetings`, `read_meeting`, and `ask_meetings` over stdio. It never opens a
network connection. `ask_meetings` uses OpenFlow's local language model if
installed; otherwise it returns the relevant meeting sources so the connected
MCP client can answer from them. Install the pinned Jac runtime used by the app
and launch the server script:

```sh
python3.13 -m pip install jaclang==0.13.5 jac-client==0.3.11
scripts/meetings-mcp.sh
```

Example MCP client configuration (adjust the absolute path to this checkout):

```json
{
  "mcpServers": {
    "openflow-meetings": {
      "command": "/absolute/path/to/OpenFlow/scripts/meetings-mcp.sh",
      "args": []
    }
  }
}
```
