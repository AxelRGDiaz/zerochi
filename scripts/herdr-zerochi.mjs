#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDirectory, "..");
const definitionPath = join(projectRoot, ".agent", "herdr-agents.json");
const args = new Set(process.argv.slice(2));
const dryRun = args.has("--dry-run");
const setupOnly = args.has("--setup-only") || dryRun;
const promptExisting = args.has("--prompt-existing");

function fail(message) {
  process.stderr.write(`\nHerdr Zerochi: ${message}\n`);
  process.exit(1);
}

function execute(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: projectRoot,
    encoding: "utf8",
    stdio: options.inherit ? "inherit" : "pipe",
  });
  if (result.error) fail(`no se pudo ejecutar ${command}: ${result.error.message}`);
  if (result.status !== 0 && !options.allowFailure) {
    const detail = result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status}`;
    fail(`${command} ${commandArgs.join(" ")} falló: ${detail}`);
  }
  return result;
}

function executeJson(commandArgs) {
  const result = execute("herdr", commandArgs);
  try {
    return JSON.parse(result.stdout);
  } catch {
    fail(`Herdr no devolvió JSON válido para: herdr ${commandArgs.join(" ")}`);
  }
}

function wait(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

function startAgentWhenShellIsReady(agent, paneId) {
  const commandArgs = [
    "agent", "start", agent.name, "--kind", "codex", "--pane", paneId,
    "--timeout", "120000", "--", "--sandbox", "workspace-write", "--ask-for-approval", "on-request",
  ];
  const deadline = Date.now() + 20_000;
  let attempts = 0;

  while (true) {
    attempts += 1;
    const result = execute("herdr", commandArgs, { allowFailure: true });
    if (result.status === 0) {
      try {
        return JSON.parse(result.stdout);
      } catch {
        fail(`Herdr inició ${agent.name}, pero no devolvió JSON válido`);
      }
    }

    const output = `${result.stderr ?? ""}\n${result.stdout ?? ""}`;
    const shellIsStarting = output.includes('"code":"agent_pane_busy"');
    if (!shellIsStarting || Date.now() >= deadline) {
      const detail = result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status}`;
      fail(`herdr ${commandArgs.join(" ")} falló después de ${attempts} intento(s): ${detail}`);
    }

    if (attempts === 1) process.stdout.write(`… esperando que el shell de ${agent.workspace} quede disponible\n`);
    wait(500);
  }
}

function snapshot() {
  return executeJson(["api", "snapshot"]).result.snapshot;
}

function validateDefinition(definition) {
  if (definition.version !== 1 || definition.project !== "zerochi") {
    fail(".agent/herdr-agents.json tiene una versión o proyecto no soportado");
  }
  if (!Array.isArray(definition.agents) || definition.agents.length !== 6) {
    fail("la definición debe contener exactamente seis agentes");
  }
  const names = new Set();
  const roles = new Set();
  for (const agent of definition.agents) {
    if (!/^zerochi-[a-z][a-z0-9_-]{0,23}$/.test(agent.name)) fail(`nombre inválido: ${agent.name}`);
    if (names.has(agent.name) || roles.has(agent.role)) fail(`agente duplicado: ${agent.name}`);
    if (!existsSync(join(projectRoot, agent.prompt))) fail(`prompt inexistente: ${agent.prompt}`);
    if (agent.checkout === "worktree" && !agent.branch?.startsWith("codex/")) {
      fail(`la rama de ${agent.name} debe usar el prefijo codex/`);
    }
    names.add(agent.name);
    roles.add(agent.role);
  }
}

function ensureRepositoryHasCommit() {
  const head = execute("git", ["rev-parse", "--verify", "HEAD"], { allowFailure: true });
  if (head.status !== 0) {
    fail("el repositorio aún no tiene commit inicial; crea un commit verde antes de generar worktrees");
  }
}

function ensureRepositoryClean() {
  const status = execute("git", ["status", "--porcelain"]);
  if (status.stdout.trim()) {
    fail("el worktree principal tiene cambios; haz commit o guárdalos antes de iniciar los agentes");
  }
}

function findWorkspace(current, label) {
  return current.workspaces.find((workspace) => workspace.label === label);
}

function findAvailablePane(current, workspaceId) {
  const agentsByPane = new Set(current.agents.map((agent) => agent.pane_id));
  return current.panes.find((pane) => pane.workspace_id === workspaceId && !agentsByPane.has(pane.pane_id));
}

function checkoutPathsByBranch() {
  const result = execute("git", ["worktree", "list", "--porcelain"]);
  const paths = new Map();
  for (const block of result.stdout.trim().split(/\n\n+/)) {
    const worktree = block.match(/^worktree (.+)$/m)?.[1];
    const branch = block.match(/^branch refs\/heads\/(.+)$/m)?.[1];
    if (worktree && branch) paths.set(branch, worktree);
  }
  return paths;
}

function expectedCheckout(agent, branchPaths) {
  return agent.checkout === "main" ? projectRoot : branchPaths.get(agent.branch);
}

function recoverAgentName(agent, current, branchPaths) {
  if (current.agents.some((candidate) => candidate.name === agent.name)) return false;

  const checkout = expectedCheckout(agent, branchPaths);
  if (!checkout) return false;
  const candidates = current.agents.filter((candidate) => {
    if (candidate.name) return false;
    return candidate.cwd === checkout || candidate.foreground_cwd === checkout;
  });
  if (candidates.length !== 1) return false;

  process.stdout.write(`↻ restaurando nombre ${agent.name} en ${candidates[0].pane_id}\n`);
  executeJson(["agent", "rename", candidates[0].pane_id, agent.name]);
  return true;
}

function requireLiveAgent(current, name) {
  const agent = current.agents.find((candidate) => candidate.name === name);
  if (!agent) fail(`el agente ${name} no está activo para construir la vista`);
  return agent;
}

function ensureBoardLayout(definition) {
  const leadName = "zerochi-lead";
  const placements = [
    { name: "zerochi-gmail", target: leadName, split: "right", ratio: "0.5" },
    { name: "zerochi-tauri", target: leadName, split: "down", ratio: "0.333" },
    { name: "zerochi-pet", target: "zerochi-tauri", split: "down", ratio: "0.5" },
    { name: "zerochi-ui", target: "zerochi-gmail", split: "down", ratio: "0.333" },
    { name: "zerochi-qa", target: "zerochi-ui", split: "down", ratio: "0.5" },
  ];

  for (const placement of placements) {
    const current = snapshot();
    const lead = requireLiveAgent(current, leadName);
    const source = requireLiveAgent(current, placement.name);
    const target = requireLiveAgent(current, placement.target);
    if (source.tab_id === lead.tab_id) continue;

    process.stdout.write(`▦ moviendo ${placement.name} a la vista 2×3\n`);
    executeJson([
      "pane", "move", source.pane_id,
      "--tab", lead.tab_id,
      "--split", placement.split,
      "--target-pane", target.pane_id,
      "--ratio", placement.ratio,
      "--no-focus",
    ]);
  }

  const arranged = snapshot();
  for (const agent of definition.agents) {
    const live = requireLiveAgent(arranged, agent.name);
    executeJson(["pane", "rename", live.pane_id, agent.name]);
  }
  executeJson(["agent", "focus", leadName]);
}

function createWorkspace(agent, definition) {
  if (agent.checkout === "main") {
    executeJson(["workspace", "create", "--cwd", projectRoot, "--label", agent.workspace, "--no-focus"]);
    return;
  }

  const openResult = execute("herdr", [
    "worktree", "open", "--cwd", projectRoot, "--branch", agent.branch,
    "--label", agent.workspace, "--no-focus",
  ], { allowFailure: true });
  if (openResult.status === 0) return;

  executeJson([
    "worktree", "create", "--cwd", projectRoot, "--branch", agent.branch,
    "--base", definition.base, "--label", agent.workspace, "--no-focus",
  ]);
}

function onboardingPrompt(agent) {
  const rolePrompt = readFileSync(join(projectRoot, agent.prompt), "utf8").trim();
  const identity = `Your Herdr name is ${agent.name} and your Zerochi role is ${agent.role}. Read every required coordination document now.`;
  if (agent.role === "lead") {
    return `${rolePrompt}\n\nHERDR LEAD STARTUP\n${identity} Inspect the current Git/worktree state plus TASKS.md and STATUS.md. When the baseline and contracts permit it, mark the next bounded parallel wave active and dispatch exactly one ownership-safe task to each needed specialist with \`herdr agent prompt <name> <prompt>\`. Do not duplicate assignments already active or completed, and do not implement specialist work yourself. Keep coordinating blockers, reviews, and integration until the active wave is accounted for.`;
  }
  return `${rolePrompt}\n\nHERDR SPECIALIST ONBOARDING\n${identity} Do not edit files, start a milestone, create commits, or assign yourself work during onboarding. Reply exactly: READY ${agent.name} — awaiting Lead assignment, then wait. When Lead assigns one bounded task, implement and test only that task in your worktree, report files/checks/risks/integration needs, and commit the green result on your configured branch.`;
}

const definition = JSON.parse(readFileSync(definitionPath, "utf8"));
validateDefinition(definition);

if (dryRun) {
  process.stdout.write("Zerochi Herdr plan (no changes):\n");
  for (const agent of definition.agents) {
    const location = agent.checkout === "main" ? "main checkout" : agent.branch;
    process.stdout.write(`- ${agent.name}: ${agent.role}, ${agent.workspace}, ${location}, ${agent.prompt}\n`);
  }
  process.exit(0);
}

execute("herdr", ["--version"]);
execute("codex", ["--version"]);
ensureRepositoryHasCommit();
const branchPaths = checkoutPathsByBranch();

let initial = snapshot();
for (const agent of definition.agents) recoverAgentName(agent, initial, branchPaths);
initial = snapshot();
if (!definition.agents.every((agent) => initial.agents.some((candidate) => candidate.name === agent.name))) {
  ensureRepositoryClean();
}

for (const agent of definition.agents) {
  let current = snapshot();
  recoverAgentName(agent, current, branchPaths);
  current = snapshot();
  const live = current.agents.find((candidate) => candidate.name === agent.name);
  if (live) {
    process.stdout.write(`✓ ${agent.name} ya está activo (${live.agent_status})\n`);
    if (promptExisting) executeJson(["agent", "prompt", agent.name, onboardingPrompt(agent)]);
    continue;
  }

  let workspace = findWorkspace(current, agent.workspace);
  if (!workspace) {
    process.stdout.write(`＋ creando ${agent.workspace}\n`);
    createWorkspace(agent, definition);
    current = snapshot();
    workspace = findWorkspace(current, agent.workspace);
    if (!workspace) fail(`Herdr no registró el workspace ${agent.workspace}`);
  }

  const pane = findAvailablePane(current, workspace.workspace_id);
  if (!pane) fail(`${agent.workspace} no tiene un pane de shell disponible`);

  process.stdout.write(`▶ iniciando ${agent.name}\n`);
  startAgentWhenShellIsReady(agent, pane.pane_id);
  executeJson(["agent", "prompt", agent.name, onboardingPrompt(agent)]);
}

ensureBoardLayout(definition);
process.stdout.write("\n✓ Los seis agentes de Zerochi están iniciados en una vista 2×3 con Lead enfocado.\n");
if (!setupOnly) execute("herdr", [], { inherit: true });
