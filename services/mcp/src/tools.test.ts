import { describe, expect, test, beforeEach, afterEach } from 'vitest';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import Database from 'better-sqlite3';

import { WotoldDb } from './db.js';
import { buildTools, type ToolContext, type ToolDefinition } from './tools.js';

interface Harness {
  tmpDir: string;
  appDataDir: string;
  db: WotoldDb;
  ctx: ToolContext;
  tools: Map<string, ToolDefinition>;
}

function seedSchema(rawDb: Database.Database) {
  // Минимальная схема под тесты (subset of 0001_initial.sql).
  rawDb.exec(`
    CREATE TABLE calls (
      id TEXT PRIMARY KEY, title TEXT, started_at TEXT NOT NULL, ended_at TEXT,
      duration_sec INTEGER, status TEXT NOT NULL, provider TEXT, path_label TEXT NOT NULL,
      lang_detected TEXT, failed_reason TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    );
    CREATE TABLE contacts (
      id TEXT PRIMARY KEY, display_name TEXT NOT NULL, is_owner INTEGER NOT NULL DEFAULT 0,
      org TEXT, role TEXT, attributes TEXT, notes TEXT,
      created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    );
    CREATE TABLE call_speakers (
      id TEXT PRIMARY KEY,
      call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
      speaker_tag TEXT NOT NULL,
      contact_id TEXT REFERENCES contacts(id),
      suggestion_contact_id TEXT,
      suggestion_score REAL,
      suggestion_source TEXT,
      confirmed INTEGER NOT NULL DEFAULT 0,
      embedding BLOB
    );
    CREATE TABLE action_items (
      id TEXT PRIMARY KEY,
      call_id TEXT NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
      text TEXT NOT NULL, owner_contact_id TEXT REFERENCES contacts(id),
      due TEXT, done INTEGER NOT NULL DEFAULT 0
    );
  `);
}

async function setupHarness(): Promise<Harness> {
  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'wotold-mcp-'));
  const dbPath = path.join(tmpDir, 'app.db');

  // Write + populate db (writable mode), затем закрываем и реоткрываем readonly через WotoldDb.
  const writer = new Database(dbPath);
  seedSchema(writer);
  writer.exec(`
    INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
    VALUES
      ('owner-1', 'Damir', 1, '{}', '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z'),
      ('c1', 'Ivan Petrov', 0, '{}', '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z'),
      ('c2', 'Anna', 0, '{}', '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z');

    INSERT INTO calls (id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, created_at, updated_at)
    VALUES
      ('call-a', 'Acme negotiation', '2026-05-19T14:00:00Z', '2026-05-19T14:30:00Z', 1800, 'ready', 'soniox', 'managed', 'ru', NULL, '2026-05-19T14:00:00Z', '2026-05-19T14:30:00Z'),
      ('call-b', NULL, '2026-05-20T08:00:00Z', '2026-05-20T08:05:00Z', 300, 'processing', NULL, 'managed', NULL, NULL, '2026-05-20T08:00:00Z', '2026-05-20T08:05:00Z'),
      ('call-c', 'Old call', '2026-05-15T10:00:00Z', '2026-05-15T10:05:00Z', 305, 'failed', 'gladia', 'managed', NULL, 'Quota', '2026-05-15T10:00:00Z', '2026-05-15T10:05:00Z');

    INSERT INTO call_speakers (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
    VALUES
      ('cs-1', 'call-a', 'Speaker 0', 'c1', 0.9, 'both', 0),
      ('cs-2', 'call-a', 'owner', 'owner-1', 1.0, 'embedding', 1);

    INSERT INTO action_items (id, call_id, text, owner_contact_id, due, done)
    VALUES
      ('ai-1', 'call-a', 'Send SOW', 'c1', '2026-05-23', 0);
  `);

  // Create artifact files
  const callsDir = path.join(tmpDir, 'calls', 'call-a');
  await fs.mkdir(callsDir, { recursive: true });
  await fs.writeFile(
    path.join(callsDir, 'transcript.md'),
    '**owner** [0:00]:\nДобрый день\n**Speaker 0** [0:05]:\nЗдравствуйте, Дамир',
  );
  await fs.writeFile(path.join(callsDir, 'recap.md'), '# Рекап\n\n- Договорились о SOW');

  writer.close();

  const db = new WotoldDb(dbPath);
  const ctx: ToolContext = { db, appDataDir: tmpDir };
  const tools = new Map(buildTools().map((t) => [t.name, t]));
  return { tmpDir, appDataDir: tmpDir, db, ctx, tools };
}

async function teardown(h: Harness): Promise<void> {
  h.db.close();
  await fs.rm(h.tmpDir, { recursive: true, force: true });
}

let h: Harness;
beforeEach(async () => {
  h = await setupHarness();
});
afterEach(async () => {
  await teardown(h);
});

function getTool(name: string): ToolDefinition {
  const t = h.tools.get(name);
  if (!t) throw new Error(`tool ${name} not built`);
  return t;
}

function unwrapText(result: { content: { type: string; text?: string }[] }): string {
  const c = result.content[0];
  if (!c || c.type !== 'text') throw new Error('expected text content');
  return c.text!;
}

function unwrapJson<T>(result: { content: { type: string; text?: string }[] }): T {
  return JSON.parse(unwrapText(result)) as T;
}

describe('buildTools — 7 tools registered', () => {
  test('list of names is exact', () => {
    const names = Array.from(h.tools.keys()).sort();
    expect(names).toEqual(
      [
        'calls_in_range',
        'find_calls_by_contact',
        'get_call',
        'get_recap',
        'get_transcript',
        'list_participants',
        'search_calls',
      ].sort(),
    );
  });
});

describe('search_calls', () => {
  test('empty query returns recent calls (descending)', async () => {
    const res = await getTool('search_calls').handler({}, h.ctx);
    const body = unwrapJson<{ calls: { id: string; started_at: string }[] }>(res);
    expect(body.calls.length).toBe(3);
    expect(body.calls[0].id).toBe('call-b'); // most recent
  });

  test('query filters by substring on title/provider/lang', async () => {
    const res = await getTool('search_calls').handler({ query: 'Acme' }, h.ctx);
    const body = unwrapJson<{ calls: { id: string }[] }>(res);
    expect(body.calls.map((c) => c.id)).toEqual(['call-a']);
  });

  test('limit caps result count', async () => {
    const res = await getTool('search_calls').handler({ limit: 1 }, h.ctx);
    const body = unwrapJson<{ calls: unknown[] }>(res);
    expect(body.calls.length).toBe(1);
  });
});

describe('get_call', () => {
  test('returns call + speakers + actions', async () => {
    const res = await getTool('get_call').handler({ call_id: 'call-a' }, h.ctx);
    const body = unwrapJson<{
      call: { id: string };
      speakers: { speaker_tag: string }[];
      action_items: { text: string }[];
    }>(res);
    expect(body.call.id).toBe('call-a');
    expect(body.speakers.length).toBe(2);
    expect(body.action_items.length).toBe(1);
    expect(body.action_items[0].text).toBe('Send SOW');
  });

  test('not found message for unknown id', async () => {
    const res = await getTool('get_call').handler({ call_id: 'ghost' }, h.ctx);
    expect(unwrapText(res)).toMatch(/not found/);
  });
});

describe('get_recap / get_transcript', () => {
  test('get_recap returns md', async () => {
    const res = await getTool('get_recap').handler({ call_id: 'call-a' }, h.ctx);
    expect(unwrapText(res)).toContain('# Рекап');
  });

  test('get_transcript returns md', async () => {
    const res = await getTool('get_transcript').handler({ call_id: 'call-a' }, h.ctx);
    expect(unwrapText(res)).toContain('Speaker 0');
  });

  test('missing artifact → empty stub', async () => {
    const res = await getTool('get_recap').handler({ call_id: 'call-b' }, h.ctx);
    expect(unwrapText(res)).toMatch(/No recap/);
  });
});

describe('list_participants', () => {
  test('returns speakers with bindings', async () => {
    const res = await getTool('list_participants').handler({ call_id: 'call-a' }, h.ctx);
    const body = unwrapJson<{ speakers: { speaker_tag: string; confirmed: number }[] }>(res);
    expect(body.speakers.length).toBe(2);
    const owner = body.speakers.find((s) => s.speaker_tag === 'owner');
    expect(owner?.confirmed).toBe(1);
  });
});

describe('find_calls_by_contact', () => {
  test('finds Ivan by partial name', async () => {
    const res = await getTool('find_calls_by_contact').handler({ contact_name: 'Ivan' }, h.ctx);
    const body = unwrapJson<{ contacts: { id: string }[]; by_contact: { contact_id: string; calls: { id: string }[] }[] }>(res);
    expect(body.contacts.length).toBe(1);
    expect(body.contacts[0].id).toBe('c1');
    expect(body.by_contact[0].calls.map((c) => c.id)).toEqual(['call-a']);
  });

  test('returns empty when no match', async () => {
    const res = await getTool('find_calls_by_contact').handler({ contact_name: 'Xeno' }, h.ctx);
    const body = unwrapJson<{ contacts: unknown[]; calls: unknown[] }>(res);
    expect(body.contacts).toEqual([]);
  });
});

describe('calls_in_range', () => {
  test('returns calls within ISO range', async () => {
    const res = await getTool('calls_in_range').handler(
      { start: '2026-05-19T00:00:00Z', end: '2026-05-21T00:00:00Z' },
      h.ctx,
    );
    const body = unwrapJson<{ calls: { id: string }[] }>(res);
    expect(body.calls.map((c) => c.id).sort()).toEqual(['call-a', 'call-b'].sort());
  });

  test('excludes calls outside range', async () => {
    const res = await getTool('calls_in_range').handler(
      { start: '2026-05-19T00:00:00Z', end: '2026-05-19T23:59:59Z' },
      h.ctx,
    );
    const body = unwrapJson<{ calls: { id: string }[] }>(res);
    expect(body.calls.map((c) => c.id)).toEqual(['call-a']);
  });
});

describe('input validation (zod)', () => {
  test('search_calls rejects negative limit', async () => {
    await expect(getTool('search_calls').handler({ limit: -1 }, h.ctx)).rejects.toThrow();
  });

  test('get_call rejects empty id', async () => {
    await expect(getTool('get_call').handler({ call_id: '' }, h.ctx)).rejects.toThrow();
  });
});
