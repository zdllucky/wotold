// M8.1 (#35): read-only SQLite доступ к Wotold app.db.
//
// SECURITY:
// - open в режиме readonly: fileMustExist=true, readonly=true
// - все запросы — parameterized (sqlite biding), никаких string concat
// - возвращаем только полей которые имеет смысл показать LLM (skip embedding BLOB)

import Database from 'better-sqlite3';

export interface Call {
  id: string;
  title: string | null;
  started_at: string;
  ended_at: string | null;
  duration_sec: number | null;
  status: string;
  provider: string | null;
  path_label: string;
  lang_detected: string | null;
  failed_reason: string | null;
}

export interface Contact {
  id: string;
  display_name: string;
  is_owner: boolean;
  org: string | null;
  role: string | null;
  notes: string | null;
}

export interface ContactIdentifier {
  id: string;
  contact_id: string;
  kind: string;
  value: string;
}

export interface ActionItem {
  id: string;
  call_id: string;
  text: string;
  owner_contact_id: string | null;
  due: string | null;
  done: number;
}

export interface CallSpeaker {
  speaker_tag: string;
  contact_id: string | null;
  suggestion_contact_id: string | null;
  suggestion_score: number | null;
  suggestion_source: string | null;
  confirmed: number;
}

export class WotoldDb {
  private db: Database.Database;

  constructor(dbPath: string) {
    this.db = new Database(dbPath, { readonly: true, fileMustExist: true });
    this.db.pragma('foreign_keys = ON');
  }

  close(): void {
    this.db.close();
  }

  /** Полнотекстовый поиск пока заглушка (#30 FTS follow-up) — substring по title + transcript md path label. */
  searchCalls(opts: { query?: string; limit: number }): Call[] {
    const limit = clampLimit(opts.limit);
    if (!opts.query || opts.query.trim() === '') {
      return this.db
        .prepare(
          `SELECT id, title, started_at, ended_at, duration_sec, status, provider,
                  path_label, lang_detected, failed_reason
           FROM calls
           ORDER BY started_at DESC
           LIMIT ?`,
        )
        .all(limit) as Call[];
    }
    const pattern = `%${opts.query.trim()}%`;
    return this.db
      .prepare(
        `SELECT id, title, started_at, ended_at, duration_sec, status, provider,
                path_label, lang_detected, failed_reason
         FROM calls
         WHERE title LIKE ? OR provider LIKE ? OR lang_detected LIKE ?
         ORDER BY started_at DESC
         LIMIT ?`,
      )
      .all(pattern, pattern, pattern, limit) as Call[];
  }

  getCall(id: string): Call | null {
    const row = this.db
      .prepare(
        `SELECT id, title, started_at, ended_at, duration_sec, status, provider,
                path_label, lang_detected, failed_reason
         FROM calls WHERE id = ?`,
      )
      .get(id) as Call | undefined;
    return row ?? null;
  }

  listContacts(): Contact[] {
    return this.db
      .prepare(
        `SELECT id, display_name, is_owner, org, role, notes
         FROM contacts
         ORDER BY is_owner DESC, display_name ASC`,
      )
      .all() as Contact[];
  }

  findContactsByName(query: string): Contact[] {
    const pattern = `%${query.trim()}%`;
    return this.db
      .prepare(
        `SELECT id, display_name, is_owner, org, role, notes
         FROM contacts
         WHERE display_name LIKE ? OR org LIKE ?
         ORDER BY is_owner DESC, display_name ASC
         LIMIT 50`,
      )
      .all(pattern, pattern) as Contact[];
  }

  callsByContact(contactId: string, limit: number): Call[] {
    const lim = clampLimit(limit);
    return this.db
      .prepare(
        `SELECT DISTINCT c.id, c.title, c.started_at, c.ended_at, c.duration_sec,
                c.status, c.provider, c.path_label, c.lang_detected, c.failed_reason
         FROM calls c
         JOIN call_speakers cs ON cs.call_id = c.id
         WHERE (cs.contact_id = ? OR cs.suggestion_contact_id = ?)
         ORDER BY c.started_at DESC
         LIMIT ?`,
      )
      .all(contactId, contactId, lim) as Call[];
  }

  callsInRange(startIso: string, endIso: string, limit: number): Call[] {
    const lim = clampLimit(limit);
    return this.db
      .prepare(
        `SELECT id, title, started_at, ended_at, duration_sec, status, provider,
                path_label, lang_detected, failed_reason
         FROM calls
         WHERE started_at >= ? AND started_at <= ?
         ORDER BY started_at DESC
         LIMIT ?`,
      )
      .all(startIso, endIso, lim) as Call[];
  }

  callSpeakers(callId: string): CallSpeaker[] {
    return this.db
      .prepare(
        `SELECT speaker_tag, contact_id, suggestion_contact_id, suggestion_score,
                suggestion_source, confirmed
         FROM call_speakers
         WHERE call_id = ?
         ORDER BY speaker_tag ASC`,
      )
      .all(callId) as CallSpeaker[];
  }

  callActionItems(callId: string): ActionItem[] {
    return this.db
      .prepare(
        `SELECT id, call_id, text, owner_contact_id, due, done
         FROM action_items
         WHERE call_id = ?
         ORDER BY due ASC NULLS LAST, id ASC`,
      )
      .all(callId) as ActionItem[];
  }
}

function clampLimit(n: number): number {
  if (!Number.isFinite(n) || n <= 0) return 20;
  return Math.min(Math.floor(n), 200);
}
