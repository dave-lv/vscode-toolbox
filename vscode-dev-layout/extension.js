// @ts-check
// Dev Layout: when a project window opens, split the terminal panel into a small shell and a wide
// terminal running an AI agent: `claude` (the plain CLI, which then connects to the Claude Code IDE
// extension), `codex`, or a command of your own.
//
// Also shares files with the Code Toolbox menu bar app (~/.config/code-toolbox):
//   config.json                   the app's settings; `agents` holds per-project agent choices
//   open/<sha1(uri)>-<pid>.json   this window is open (the app's green dot)
//   skip-layout/<sha1(uri)>       "Open without dev layout" from the app; consumed here once
//                                 (ignored once older than SKIP_MARKER_LIFETIME_MS)
//   close/<sha1(uri)>             "Close Window" from the app; this window closes itself
const vscode = require('vscode');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');

const STATE_DIR = path.join(os.homedir(), '.config', 'code-toolbox');
// VS Code's "resize terminal pane" commands move the divider by 4 character widths (or heights).
const COLUMNS_PER_STEP = 4;
// A skip-layout marker this old is from a launch that never opened its window. Keep in sync with
// SKIP_MARKER_LIFETIME in core/src/bridge.rs.
const SKIP_MARKER_LIFETIME_MS = 120_000;

/** @type {vscode.OutputChannel} */
let log;
/** @type {string | undefined} */
let registryFile;

const sleep = (/** @type {number} */ ms) => new Promise((resolve) => setTimeout(resolve, ms));
const sha1 = (/** @type {string} */ s) => crypto.createHash('sha1').update(s).digest('hex');

/** @param {vscode.ExtensionContext} context */
function activate(context) {
    const channel = vscode.window.createOutputChannel('Dev Layout');
    context.subscriptions.push(channel);
    log = withFileLog(channel);
    context.subscriptions.push(vscode.commands.registerCommand('devLayout.open', () => openLayout()));

    const uri = windowUri();
    if (!uri) return;
    registerWindow(uri);
    watchCloseRequests(uri, context);
    const userPath = (process.env.PATH ?? '').includes(path.join(os.homedir(), '.local', 'bin'));
    log.appendLine(`activated (${vscode.env.remoteName ?? 'local'} window, pid ${process.pid}, `
        + `${userPath ? 'has' : 'no'} ~/.local/bin in PATH)`);
    const run = needsSetup(context) ? offerSetup(context) : autoOpen(uri, context);
    run.catch((err) => log.appendLine(`auto-open failed: ${err}`));
}

/**
 * Freshly installed (e.g. from Code Toolbox's Settings): nothing has set `autoOpen` in this profile and
 * the question hasn't been asked here before.
 * @param {vscode.ExtensionContext} context
 */
function needsSetup(context) {
    const auto = vscode.workspace.getConfiguration('devLayout').inspect('autoOpen');
    const set = [auto?.globalValue, auto?.workspaceValue, auto?.workspaceFolderValue].some((v) => v !== undefined);
    return !set && !context.globalState.get('setupOffered');
}

/**
 * Asks once whether to turn the layout on, and with which agent; VS Code saves the answer in the
 * profile's settings.
 * @param {vscode.ExtensionContext} context
 */
async function offerSetup(context) {
    await context.globalState.update('setupOffered', true);
    const choice = await vscode.window.showInformationMessage(
        'Dev Layout: open a shell and an AI agent side by side whenever a project window opens?',
        'Claude Code', 'Codex', 'Not Now');
    if (choice !== 'Claude Code' && choice !== 'Codex') {
        log.appendLine('setup: declined; turn on devLayout.autoOpen in Settings to use it');
        return;
    }
    const cfg = vscode.workspace.getConfiguration('devLayout');
    await cfg.update('agent', choice === 'Codex' ? 'codex' : 'claude', vscode.ConfigurationTarget.Global);
    await cfg.update('autoOpen', true, vscode.ConfigurationTarget.Global);
    log.appendLine(`setup: turned on with ${choice}`);
    if (vscode.workspace.isTrusted) await openIfFree();
}

/**
 * A setting, or the name it had before 0.2.0 (`claudeSize` → `agentSize`) while the new one isn't set.
 * @param {vscode.WorkspaceConfiguration} cfg @param {string} key @param {string} oldKey @param {any} fallback
 */
function setting(cfg, key, oldKey, fallback) {
    const i = cfg.inspect(key);
    const set = [i?.globalValue, i?.workspaceValue, i?.workspaceFolderValue].some((v) => v !== undefined);
    return set ? cfg.get(key, fallback) : cfg.get(oldKey, fallback);
}

/** @typedef {{ kind: string, command: string, name: string }} Agent */

/**
 * The agent picked for this project in Code Toolbox (its config.json `agents`, keyed by the window's URI),
 * if any.
 */
function projectAgent() {
    const uri = windowUri();
    if (!uri) return undefined;
    try {
        const agent = JSON.parse(fs.readFileSync(path.join(STATE_DIR, 'config.json'), 'utf8')).agents?.[uri];
        return agent === 'claude' || agent === 'codex' ? agent : undefined;
    } catch {
        return undefined; // no config.json, or one being edited
    }
}

/** @param {vscode.WorkspaceConfiguration} cfg @returns {Agent} */
function agentSettings(cfg) {
    const configured = String(cfg.get('agent', 'claude'));
    const override = projectAgent();
    let kind = override ?? configured;
    if (override) log.appendLine(`agent: ${override}, picked for this project in Code Toolbox`);
    // agentCommand belongs to the profile's agent: a project switched to another one gets its plain command.
    const legacy = configured === 'claude' ? String(cfg.get('claudeCommand', '')).trim() : '';
    let command = override && override !== configured ? '' : String(cfg.get('agentCommand', '')).trim() || legacy;
    if (kind === 'custom' && !command) {
        log.appendLine('agent is "custom" but devLayout.agentCommand is empty; using claude');
        kind = 'claude';
    }
    if (kind !== 'custom') {
        kind = kind === 'codex' ? 'codex' : 'claude';
        command ||= kind;
    }
    // The terminal's name: the program's name, without its folder or Windows extension.
    const name = kind === 'custom' ? path.basename(command.split(/\s+/)[0]).replace(/\.(exe|cmd|bat)$/i, '') : kind;
    return { kind, command, name };
}

function deactivate() {
    if (registryFile) {
        try { fs.unlinkSync(registryFile); } catch { /* already gone */ }
    }
}

/**
 * Also append to ~/.config/code-toolbox/dev-layout.log (kept under ~100 KB) so problems can be
 * looked at from outside VS Code.
 * @param {vscode.OutputChannel} channel
 */
function withFileLog(channel) {
    const file = path.join(STATE_DIR, 'dev-layout.log');
    const append = (/** @type {string} */ line) => {
        try {
            fs.mkdirSync(STATE_DIR, { recursive: true });
            if (fs.existsSync(file) && fs.statSync(file).size > 100_000) fs.renameSync(file, `${file}.1`);
            fs.appendFileSync(file, `${new Date().toISOString()} [${windowUri() ?? 'no folder'}] ${line}\n`);
        } catch { /* logging must never break the extension */ }
    };
    return /** @type {vscode.OutputChannel} */ (new Proxy(channel, {
        get(target, prop) {
            if (prop === 'appendLine') {
                return (/** @type {string} */ line) => { target.appendLine(line); append(line); };
            }
            const value = /** @type {any} */ (target)[prop];
            return typeof value === 'function' ? value.bind(target) : value;
        },
    }));
}

/** The window's folder or workspace URI, in the same form VS Code writes to workspace.json. */
function windowUri() {
    const workspaceFile = vscode.workspace.workspaceFile;
    if (workspaceFile && workspaceFile.scheme !== 'untitled') return workspaceFile.toString();
    const folders = vscode.workspace.workspaceFolders;
    return folders && folders.length ? folders[0].uri.toString() : undefined;
}

/** @param {string} uri */
function registerWindow(uri) {
    try {
        const dir = path.join(STATE_DIR, 'open');
        fs.mkdirSync(dir, { recursive: true });
        registryFile = path.join(dir, `${sha1(uri)}-${process.pid}.json`);
        // `started` lets the app tell this process from a later one that reuses the pid.
        const started = Math.round(Date.now() - process.uptime() * 1000);
        fs.writeFileSync(registryFile, JSON.stringify({ uri, pid: process.pid, started }));
    } catch (err) {
        log.appendLine(`couldn't register window: ${err}`);
    }
}

/**
 * The app asks a window to close by writing close/<sha1(uri)>. A marker that already exists when the
 * window opens is stale (its window is gone), so it's removed without closing.
 * @param {string} uri @param {vscode.ExtensionContext} context
 */
function watchCloseRequests(uri, context) {
    const dir = path.join(STATE_DIR, 'close');
    const marker = path.join(dir, sha1(uri));
    try {
        fs.mkdirSync(dir, { recursive: true });
        try { fs.unlinkSync(marker); } catch { /* none */ }
        const watcher = fs.watch(dir, () => {
            try {
                fs.unlinkSync(marker);
            } catch {
                return; // not for this window
            }
            log.appendLine('closing: requested by Code Toolbox');
            vscode.commands.executeCommand('workbench.action.closeWindow');
        });
        context.subscriptions.push({ dispose: () => watcher.close() });
    } catch (err) {
        log.appendLine(`couldn't watch for close requests: ${err}`);
    }
}

/** @param {string} uri */
function consumeSkipMarker(uri) {
    const marker = path.join(STATE_DIR, 'skip-layout', sha1(uri));
    try {
        const age = Date.now() - fs.statSync(marker).mtimeMs;
        fs.unlinkSync(marker);
        if (age > SKIP_MARKER_LIFETIME_MS) {
            log.appendLine(`ignored a stale skip-layout marker (${Math.round(age / 1000)} s old)`);
            return false;
        }
        return true;
    } catch {
        return false;
    }
}

/** @param {string} uri @param {vscode.ExtensionContext} context */
async function autoOpen(uri, context) {
    if (consumeSkipMarker(uri)) {
        log.appendLine('skipped: opened with "Open without dev layout"');
        return;
    }
    // Nothing runs until the folder is trusted. Only then is `autoOpen` read: before that, VS Code hides
    // the folder's own value (restrictedConfigurations), so a folder-only `true` would read as false.
    if (!vscode.workspace.isTrusted) {
        log.appendLine('waiting: the folder is not trusted yet');
        await new Promise((resolve) => {
            const sub = vscode.workspace.onDidGrantWorkspaceTrust(() => {
                sub.dispose();
                resolve(undefined);
            });
            context.subscriptions.push(sub);
        });
    }
    const cfg = vscode.workspace.getConfiguration('devLayout');
    if (!cfg.get('autoOpen', false)) return;
    // Give VS Code time to restore terminals from last time; never add a second set.
    await sleep(cfg.get('restoreDelayMs', 1500));
    await openIfFree();
}

/** Builds the layout unless the window already has terminals of its own. */
async function openIfFree() {
    const terminals = vscode.window.terminals;
    // When the window reopens with the terminal view showing, VS Code starts one plain shell in it by
    // itself. That one becomes the shell pane instead of counting as an existing setup.
    if (terminals.length === 1 && isUntouchedDefaultShell(terminals[0])) {
        log.appendLine(`using the "${terminals[0].name}" terminal VS Code opened by itself as the shell pane`);
        await openLayout(terminals[0]);
        return;
    }
    if (terminals.length > 0) {
        const names = terminals.map((t) => t.name).join(', ');
        log.appendLine(`skipped: window already has ${terminals.length} terminal(s): ${names}`);
        return;
    }
    await openLayout();
}

/**
 * A terminal VS Code created on its own (no name or pty from an extension; VS Code does fill in
 * `shellPath` from the default profile) that nobody has typed into yet.
 * @param {vscode.Terminal} t
 */
function isUntouchedDefaultShell(t) {
    const options = /** @type {vscode.TerminalOptions & vscode.ExtensionTerminalOptions} */ (t.creationOptions);
    return !t.state.isInteractedWith && !options.name && !options.pty && !t.exitStatus;
}

/** @param {vscode.Terminal} [existing] an untouched shell to use as the first pane instead of creating one */
async function openLayout(existing) {
    const cfg = vscode.workspace.getConfiguration('devLayout');
    const folder = vscode.workspace.workspaceFolders?.[0];
    const agent = agentSettings(cfg);
    if (!cfg.get('shellPane', true)) {
        await openAgentOnly(agent, cfg, folder, existing);
        return;
    }
    const agentFirst = setting(cfg, 'agentSide', 'claudeSide', 'right') === 'left';
    const size = Math.min(0.9, Math.max(0.1, Number(setting(cfg, 'agentSize', 'claudeSize', 0.7))));

    // Same steps as by hand: open a terminal, focus it, run "Split Terminal", rename the new pane.
    // (`createTerminal({ location: { parentTerminal } })` opened a second tab instead of a split.)
    const firstName = agentFirst ? agent.name : 'shell';
    const secondName = agentFirst ? 'shell' : agent.name;
    const first = existing
        ?? vscode.window.createTerminal({ name: firstName, cwd: folder?.uri, location: vscode.TerminalLocation.Panel });
    first.show(false);
    await Promise.race([first.processId, sleep(5000)]);
    const single = await stableDims([first], 3000);

    // "Split Terminal" splits the active terminal, and other terminals can appear meanwhile (a late
    // restore, another extension). Only go on when exactly one new terminal appeared and it's active.
    first.show(false);
    if (!await waitFor(() => vscode.window.activeTerminal === first, 2000)) {
        log.appendLine('split skipped: another terminal took focus');
        return;
    }
    if (existing) await vscode.commands.executeCommand('workbench.action.terminal.renameWithArg', { name: firstName });
    const before = new Set(vscode.window.terminals);
    await vscode.commands.executeCommand('workbench.action.terminal.split');
    const added = await newTerminals(before, 5000);
    const second = added.length === 1 ? added[0] : undefined;
    if (!second || !await waitFor(() => vscode.window.activeTerminal === second, 2000)) {
        log.appendLine(added.length
            ? `split ambiguous: ${added.length} new terminal(s) appeared; left them alone`
            : 'split failed: no new terminal appeared');
        return;
    }
    // renameWithArg renames the active terminal, which is the new pane.
    await vscode.commands.executeCommand('workbench.action.terminal.renameWithArg', { name: secondName });
    const shell = agentFirst ? second : first;
    const agentTerminal = agentFirst ? first : second;

    await resize(shell, agentTerminal, agentFirst, size, single?.[0], cfg);

    const shellCommand = String(cfg.get('shellCommand', '')).trim();
    if (shellCommand) shell.sendText(shellCommand, true);
    const command = await agentCommand(cfg, agent, folder, agentTerminal);
    agentTerminal.sendText(command, true);
    (String(cfg.get('focus', 'agent')) === 'shell' ? shell : agentTerminal).show(false);
    log.appendLine(`layout ready; ${agent.name} terminal runs: ${command}`);
}

/**
 * `shellPane` off: just the agent's terminal (VS Code's own untouched shell becomes it, if there is one).
 * @param {Agent} agent @param {vscode.WorkspaceConfiguration} cfg
 * @param {vscode.WorkspaceFolder | undefined} folder @param {vscode.Terminal} [existing]
 */
async function openAgentOnly(agent, cfg, folder, existing) {
    const terminal = existing
        ?? vscode.window.createTerminal({ name: agent.name, cwd: folder?.uri, location: vscode.TerminalLocation.Panel });
    terminal.show(false);
    await Promise.race([terminal.processId, sleep(5000)]);
    if (existing) {
        if (!await waitFor(() => vscode.window.activeTerminal === terminal, 2000)) {
            log.appendLine('skipped: another terminal took focus');
            return;
        }
        await vscode.commands.executeCommand('workbench.action.terminal.renameWithArg', { name: agent.name });
    }
    const command = await agentCommand(cfg, agent, folder, terminal);
    terminal.sendText(command, true);
    log.appendLine(`layout ready (no shell pane); ${agent.name} terminal runs: ${command}`);
}

/**
 * VS Code has no API to set a split's size, so measure both panes and press "resize pane" enough
 * times, then re-measure and correct (the window may still be settling when we first measure).
 *
 * A bottom/top panel puts split terminals side by side (sized by columns, divider moved with
 * resize left/right); a left/right panel stacks them (sized by rows, resize up/down). In a stacked
 * split, left/right would resize the whole panel instead.
 * @param {vscode.Terminal} shell @param {vscode.Terminal} agent @param {boolean} agentFirst
 * @param {number} size @param {Dims | undefined} single size of the first terminal before the split
 * @param {vscode.WorkspaceConfiguration} cfg
 */
async function resize(shell, agent, agentFirst, size, single, cfg) {
    /** @type {boolean | undefined} */
    let stacked;
    for (let attempt = 1; attempt <= 4; attempt++) {
        const dims = await stableDims([shell, agent], 6000);
        if (!dims) {
            if (attempt === 1) await press(agentFirst, false, Number(cfg.get('fallbackResizeSteps', 6)), agent);
            log.appendLine("couldn't measure the terminals" + (attempt === 1 ? '; used the fallback step count' : ''));
            return;
        }
        const [sh, cl] = dims;
        if (stacked === undefined) {
            // Side by side: both keep the full height. Stacked: both keep the full width.
            stacked = single ? sh.rows + cl.rows <= single.rows + 2 && sh.columns === cl.columns
                             : sh.columns === cl.columns && sh.rows !== cl.rows;
            log.appendLine(`split is ${stacked ? 'stacked (side panel)' : 'side by side'}`
                + (single ? `; single terminal was ${single.columns}x${single.rows}` : ''));
            const share = Number(cfg.get('sidebarWidth', 0));
            if (stacked && share > 0) {
                await fitSidebarWidth(agent, Math.min(0.9, share));
                continue; // re-measure rows after the width change
            }
        }
        const [shellLen, agentLen] = stacked ? [sh.rows, cl.rows] : [sh.columns, cl.columns];
        // Resize moves the divider by 4 character widths/heights per step.
        const target = Math.round((shellLen + agentLen) * size);
        const steps = Math.round((target - agentLen) / COLUMNS_PER_STEP);
        log.appendLine(`attempt ${attempt}: shell=${shellLen} agent=${agentLen} ${stacked ? 'rows' : 'cols'}, `
            + `target agent=${target}, ${steps} step(s)`);
        if (!steps) return;
        await press(agentFirst, stacked, steps, agent);
    }
}

/**
 * Terminals in the secondary sidebar: make the sidebar `share` of the window width. VS Code doesn't
 * tell extensions the window width, so maximize the sidebar briefly and read the terminal's columns.
 * With stacked terminals, "resize left/right" resizes the sidebar itself, 4 columns per step.
 * @param {vscode.Terminal} agent @param {number} share
 */
async function fitSidebarWidth(agent, share) {
    agent.show(false);
    const before = await stableDims([agent], 3000);
    await vscode.commands.executeCommand('workbench.action.maximizeAuxiliaryBar');
    const full = await stableDims([agent], 3000);
    // After "maximize" the sidebar is maximized either way, so this always restores it.
    await vscode.commands.executeCommand('workbench.action.toggleMaximizedAuxiliaryBar');
    const now = await stableDims([agent], 3000);
    // Maximizing widened the terminal: normal case. It didn't, but restoring narrowed it: the sidebar
    // was already maximized (e.g. the window reopened that way). Neither: not in the secondary sidebar.
    const grew = before && full && full[0].columns > before[0].columns + 2;
    const wasMaximized = !grew && full && now && now[0].columns < full[0].columns - 2;
    if (!full || !now || (!grew && !wasMaximized)) {
        log.appendLine('sidebar width: terminal is not in the secondary sidebar (or not measurable); left as is');
        return;
    }
    if (wasMaximized) log.appendLine('sidebar width: the sidebar was maximized; restored it');
    const target = Math.round(full[0].columns * share);
    let current = now[0].columns;
    log.appendLine(`sidebar width: window ${full[0].columns} cols, sidebar ${current} → ${target} cols`);

    // Which direction widens the sidebar depends on its side; take one step and check.
    let widen = 'workbench.action.terminal.resizePaneLeft';
    let narrow = 'workbench.action.terminal.resizePaneRight';
    for (let attempt = 0; attempt < 3; attempt++) {
        const steps = Math.round((target - current) / COLUMNS_PER_STEP);
        if (!steps) return;
        agent.show(false);
        await vscode.commands.executeCommand(steps > 0 ? widen : narrow);
        const probe = await stableDims([agent], 2000);
        if (!probe) return;
        if ((probe[0].columns - current) * steps < 0) [widen, narrow] = [narrow, widen]; // went the wrong way
        current = probe[0].columns;
        const rest = Math.round((target - current) / COLUMNS_PER_STEP);
        for (let i = 0; i < Math.abs(rest); i++) {
            await vscode.commands.executeCommand(rest > 0 ? widen : narrow);
        }
        const after = await stableDims([agent], 3000);
        if (!after) return;
        current = after[0].columns;
        log.appendLine(`sidebar width: now ${current} cols`);
    }
}

/**
 * Press "resize pane" `steps` times with the agent focused; positive grows the agent.
 * With the focused pane second (right/below), left/up moves the divider toward the first pane.
 * @param {boolean} agentFirst @param {boolean} stacked @param {number} steps @param {vscode.Terminal} agent
 */
async function press(agentFirst, stacked, steps, agent) {
    if (!steps) return;
    const [towardStart, towardEnd] = stacked ? ['Up', 'Down'] : ['Left', 'Right'];
    const grow = agentFirst ? towardEnd : towardStart;
    const shrink = agentFirst ? towardStart : towardEnd;
    agent.show(false); // the resize commands act on the focused pane
    await sleep(50);
    const command = `workbench.action.terminal.resizePane${steps > 0 ? grow : shrink}`;
    for (let i = 0; i < Math.abs(steps); i++) {
        await vscode.commands.executeCommand(command);
    }
}

/**
 * Terminals that weren't in `before`: waits up to `timeoutMs` for the first, then a little longer so
 * that any others opened at the same time are counted too.
 * @param {Set<vscode.Terminal>} before @param {number} timeoutMs @returns {Promise<vscode.Terminal[]>}
 */
async function newTerminals(before, timeoutMs) {
    const added = () => vscode.window.terminals.filter((t) => !before.has(t));
    if (await waitFor(() => added().length > 0, timeoutMs)) await sleep(300);
    return added();
}

/**
 * Polls `condition` every 50 ms; false if it's still false after `timeoutMs`.
 * @param {() => boolean} condition @param {number} timeoutMs
 */
async function waitFor(condition, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    while (!condition()) {
        if (Date.now() >= deadline) return false;
        await sleep(50);
    }
    return true;
}

/** @typedef {{ columns: number, rows: number }} Dims */

/**
 * Terminal sizes once they have rendered and stopped changing for ~0.6 s (undefined on timeout).
 * `Terminal.dimensions` is set by VS Code whenever a terminal is laid out.
 * @param {vscode.Terminal[]} terminals @param {number} timeoutMs
 * @returns {Promise<Dims[] | undefined>}
 */
async function stableDims(terminals, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    let last = '';
    let stableFor = 0;
    while (Date.now() < deadline) {
        const dims = terminals.map((t) => /** @type {any} */ (t).dimensions);
        if (dims.every((d) => d && d.columns > 0 && d.rows > 0)) {
            const key = dims.map((d) => `${d.columns}x${d.rows}`).join(',');
            stableFor = key === last ? stableFor + 1 : 0;
            last = key;
            if (stableFor >= 6) return dims.map((d) => ({ columns: d.columns, rows: d.rows }));
        }
        await sleep(100);
    }
    return undefined;
}

/**
 * The agent's command line. Resuming: `claude --continue` when this folder already has a Claude
 * conversation (checked up front rather than `claude --continue || claude`, which would start a new
 * session whenever Claude exits non-zero, e.g. Ctrl+C); `codex resume --last`, which picks the folder's
 * latest session itself and starts a new one when there's none.
 * @param {vscode.WorkspaceConfiguration} cfg @param {Agent} agent
 * @param {vscode.WorkspaceFolder | undefined} folder
 * @param {vscode.Terminal} terminal the terminal the command will be typed into
 */
async function agentCommand(cfg, agent, folder, terminal) {
    const base = agent.command;
    if (!cfg.get('continueSession', true) || !folder || agent.kind === 'custom') return base;
    // `resume` is a subcommand: it goes right after the program, before any flags from agentCommand.
    if (agent.kind === 'codex') return base.replace(/^(\S+)/, '$1 resume --last');
    if (!vscode.env.remoteName) return hasClaudeSession(folder.uri.fsPath) ? `${base} --continue` : base;

    // Container/SSH window: the terminal runs there, so the check has to run in that machine's shell,
    // in that shell's syntax. Unknown shells get plain `claude`.
    await waitFor(() => terminalShell(terminal) !== undefined, 3000);
    const shell = terminalShell(terminal);
    const command = remoteContinueCommand(shell, base);
    if (!command) log.appendLine(`no session check for shell "${shell ?? 'unknown'}"; starting a new session`);
    return command ?? base;
}

/** @param {vscode.Terminal} terminal @returns {string | undefined} */
function terminalShell(terminal) {
    return /** @type {{ shell?: string }} */ (terminal.state).shell;
}

/**
 * A one-line "continue if $CLAUDE_CONFIG_DIR (or ~/.claude)/projects/<cwd as dashes> has a *.jsonl" for the given shell type
 * (`Terminal.state.shell`), or undefined if we don't know its syntax.
 * @param {string | undefined} shell @param {string} base
 */
function remoteContinueCommand(shell, base) {
    switch (shell) {
        case 'bash': case 'zsh': case 'sh': case 'ksh': case 'gitbash':
            return `if ls "\${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects/$(pwd | sed 's/[^A-Za-z0-9]/-/g')" 2>/dev/null `
                + `| grep -q '\\.jsonl$'; then ${base} --continue; else ${base}; fi`;
        case 'fish':
            // `count` is one of the commands fish runs even when the glob matches nothing.
            return `begin; set -l d ~/.claude; test -n "$CLAUDE_CONFIG_DIR"; and set d $CLAUDE_CONFIG_DIR; `
                + `if count $d/projects/(pwd | string replace -ra -- '[^A-Za-z0-9]' '-')/*.jsonl >/dev/null; `
                + `${base} --continue; else; ${base}; end; end`;
        case 'pwsh':
            return `if (Get-ChildItem -Path (Join-Path $(if ($env:CLAUDE_CONFIG_DIR) { $env:CLAUDE_CONFIG_DIR } else { Join-Path $HOME '.claude' }) `
                + `('projects/' + ((Get-Location).Path -replace '[^A-Za-z0-9]','-'))) `
                + `-Filter *.jsonl -ErrorAction SilentlyContinue) { ${base} --continue } else { ${base} }`;
        default:
            return undefined;
    }
}

/** Claude Code keeps conversations in ~/.claude/projects/<path with every non-alphanumeric as "-">/*.jsonl. */
function hasClaudeSession(/** @type {string} */ dir) {
    const configDir = process.env.CLAUDE_CONFIG_DIR || path.join(os.homedir(), '.claude');
    const projectDir = path.join(configDir, 'projects', dir.replace(/[^A-Za-z0-9]/g, '-'));
    try {
        return fs.readdirSync(projectDir).some((f) => f.endsWith('.jsonl'));
    } catch {
        return false;
    }
}

module.exports = { activate, deactivate };
