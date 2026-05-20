import { invoke } from '@tauri-apps/api/core';

export interface ContactIdentifier {
  id: string;
  kind: string;
  value: string;
}

export interface ContactIdentifierInput {
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
  identifiers?: ContactIdentifierInput[];
  attributes?: Record<string, unknown>;
}

export function listContacts(): Promise<Contact[]> {
  return invoke<Contact[]>('list_contacts');
}

export function createContact(input: ContactInput): Promise<Contact> {
  return invoke<Contact>('create_contact', { input });
}

export function updateContact(id: string, input: ContactInput): Promise<Contact> {
  return invoke<Contact>('update_contact', { id, input });
}

export function deleteContact(id: string): Promise<void> {
  return invoke<void>('delete_contact', { id });
}

export interface OwnerContact {
  id: string;
  display_name: string;
}

export function renameOwnerContact(newName: string): Promise<OwnerContact> {
  return invoke<OwnerContact>('rename_owner_contact', { newName });
}

export const IDENTIFIER_KINDS = [
  'phone',
  'email',
  'telegram',
  'whatsapp',
  'signal',
  'slack',
  'other',
] as const;
