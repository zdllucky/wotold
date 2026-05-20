#!/usr/bin/env node
// Local MCP server for Wotold (#35 / M8.1-8.4).
//
// Default app data path:
// - macOS: ~/Library/Application Support/app.wotold.desktop/app.db
// - Override через env WOTOLD_APP_DATA_DIR
//
// Stdio MCP transport — клиент (Claude Desktop / Cursor / mcp-cli) спавнит
// этот процесс и общается через stdin/stdout JSON-RPC.

import { homedir } from 'node:os';
import path from 'node:path';
import process from 'node:process';

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';

import { WotoldDb } from './db.js';
import { buildTools, type ToolContext } from './tools.js';

function defaultAppDataDir(): string {
  if (process.env.WOTOLD_APP_DATA_DIR) return process.env.WOTOLD_APP_DATA_DIR;
  if (process.platform === 'darwin') {
    return path.join(homedir(), 'Library', 'Application Support', 'app.wotold.desktop');
  }
  if (process.platform === 'linux') {
    return path.join(homedir(), '.local', 'share', 'app.wotold.desktop');
  }
  if (process.platform === 'win32') {
    return path.join(
      process.env.APPDATA ?? path.join(homedir(), 'AppData', 'Roaming'),
      'app.wotold.desktop',
    );
  }
  return path.join(homedir(), '.wotold');
}

async function main(): Promise<void> {
  const appDataDir = defaultAppDataDir();
  const dbPath = path.join(appDataDir, 'app.db');

  const db = new WotoldDb(dbPath);
  const ctx: ToolContext = { db, appDataDir };
  const tools = buildTools();
  const toolsByName = new Map(tools.map((t) => [t.name, t]));

  const server = new Server(
    { name: 'wotold-mcp', version: '0.0.1' },
    {
      capabilities: { tools: {} },
    },
  );

  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: tools.map(({ name, description, inputSchema }) => ({
      name,
      description,
      inputSchema,
    })),
  }));

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const tool = toolsByName.get(req.params.name);
    if (!tool) {
      throw new Error(`Unknown tool: ${req.params.name}`);
    }
    return await tool.handler(req.params.arguments ?? {}, ctx);
  });

  // Stderr log — stdout зарезервирован под MCP JSON-RPC.
  process.stderr.write(`[wotold-mcp] db: ${dbPath}\n`);
  process.stderr.write(`[wotold-mcp] tools: ${tools.map((t) => t.name).join(', ')}\n`);

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((e) => {
  process.stderr.write(`[wotold-mcp] fatal: ${e instanceof Error ? e.stack ?? e.message : String(e)}\n`);
  process.exit(1);
});
