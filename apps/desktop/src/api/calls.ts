import { invoke } from '@tauri-apps/api/core';

import type { Call } from './recording';

export interface ActionItem {
  id: string;
  call_id: string;
  text: string;
  owner_contact_id: string | null;
  due: string | null;
  done: boolean;
}

export type CallArtifactKind = 'recap' | 'transcript';

export function getCall(id: string): Promise<Call | null> {
  return invoke<Call | null>('get_call', { id });
}

export function listCallActionItems(callId: string): Promise<ActionItem[]> {
  return invoke<ActionItem[]>('list_call_action_items', { callId });
}

export function readCallArtifact(
  callId: string,
  kind: CallArtifactKind,
): Promise<string | null> {
  return invoke<string | null>('read_call_artifact', { callId, kind });
}
