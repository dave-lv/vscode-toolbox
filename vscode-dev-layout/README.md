# Dev Layout

Part of Code Toolbox, which installs it from its Settings. When a project window opens, it
splits the terminal into:

- **shell**, a plain terminal in the project folder
- **agent** (70% by default), running **Claude Code** (`claude`, or `claude --continue` when the project
  already has a conversation), **Codex** (`codex resume --last`) or a command of your own

It runs when `devLayout.autoOpen` is on and the window has no terminals yet; in a profile that hasn't used it
before, the first window asks whether to turn it on and with which agent. Settings are under `devLayout.*`
(agent, command and flags, resuming, an agent-only layout, focus, sizes). **Dev Layout: Shell + Agent** in the
command palette builds it by hand, and the `Dev Layout` output channel logs what it did.
