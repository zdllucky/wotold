// Recap JSON — структурированный результат генерации рекапа.
// См. M4.2 паспорта.

export interface ActionItem {
  text: string;
  /**
   * Hint about responsible person:
   * - if maps to a confirmed contact: contact id (UUID)
   * - if ambiguous: free-form name/description, flag for manual binding (M4.3)
   */
  ownerHint?: string;
  /** ISO 8601 date. */
  due?: string;
}

export interface RecapParticipant {
  speakerTag: string;
  contactId?: string;
  displayName?: string;
}

export interface RecapJson {
  version: 1;
  summary: string;
  keyPoints: string[];
  /** Minutes of Meeting, Markdown. */
  mom: string;
  actionItems: ActionItem[];
  participants: RecapParticipant[];
}
