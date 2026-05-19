import { invoke } from '@tauri-apps/api/core';

export interface ContactIdentifier {
  id: string;
  kind: string;
  value: string;
}

export interface Contact {
  id: string;
  display_name: string;
  is_owner: boolean;
  org: string | null;
  role: string | null;
  attributes: Record<string, unknown>;
  notes: string | null;
  created_at: string;
  updated_at: string;
  identifiers: ContactIdentifier[];
}

export interface ContactInput {
  display_name: string;
  org?: string;
  role?: string;
  notes?: string;
}

export function listContacts(): Promise<Contact[]> {
  return invoke<Contact[]>('list_contacts');
}

export function createContact(input: ContactInput): Promise<Contact> {
  return invoke<Contact>('create_contact', { input });
}

export function deleteContact(id: string): Promise<void> {
  return invoke<void>('delete_contact', { id });
}
