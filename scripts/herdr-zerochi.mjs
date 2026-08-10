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

function ensureRepositoryReady() {
  const head = execute("git", ["rev-parse", "--verify", "HEAD"], { allowFailure: true });
  if (head.status !== 0) {
    fail("el repositorio aún no tiene commit inicial; crea un commit verde antes de generar worktrees");
  }
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
  return `${rolePrompt}\n\nHERDR ONBOARDING\nYour Herdr name is ${agent.name} and your Zerochi role is ${agent.role}. Read every required coordination document now. Do not edit files, start a milestone, create commits, or assign yourself work during onboarding. When ready, reply exactly: READY ${agent.name} — awaiting Lead assignment.`;
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
ensureRepositoryReady();

for (const agent of definition.agents) {
  let current = snapshot();
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
  executeJson([
    "agent", "start", agent.name, "--kind", "codex", "--pane", pane.pane_id,
    "--timeout", "120000", "--", "--sandbox", "workspace-write", "--ask-for-approval", "on-request",
  ]);
  executeJson(["agent", "prompt", agent.name, onboardingPrompt(agent)]);
}

process.stdout.write("\n✓ Los seis agentes de Zerochi están iniciados y recibieron onboarding.\n");
if (!setupOnly) execute("herdr", [], { inherit: true });
