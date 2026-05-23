// Action items list rendered inside Tabs.Panel value="tasks".
// Empty placeholder when no tasks; otherwise ordered list with owner +
// due metadata. Done items получают strike-through.
//
// [M14 T-11] V2 enrichment: category emoji prefix (✅/💡/📝), confidence
// badge при низкой уверенности владельца, EvidenceTooltip 💬 для quote.

import type { ActionItem } from '../../api/calls';
import type { Contact } from '../../api/contacts';
import { Empty } from '../../ui';
import { useI18n } from '../../i18n';
import { EvidenceTooltip } from './EvidenceTooltip';

interface TasksPanelProps {
  tasks: ActionItem[];
  contacts: Contact[];
  /** [M14 T-11] Jump к timestamp в расшифровке (опц.). */
  onJumpToTranscript?: (ms: number) => void;
}

function categoryEmoji(category: string | null): string {
  switch (category) {
    case 'proposal':
      return '💡 ';
    case 'idea':
      return '📝 ';
    case 'commitment':
    default:
      return '✅ ';
  }
}

export function TasksPanel({ tasks, contacts, onJumpToTranscript }: TasksPanelProps) {
  const { t } = useI18n();
  if (tasks.length === 0) {
    return <Empty description={t('callDetail.emptyTasks')} />;
  }
  const nameById = new Map(contacts.map((c) => [c.id, c.display_name]));
  return (
    <ol style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {tasks.map((task, i) => {
        const owner = task.owner_contact_id ? nameById.get(task.owner_contact_id) : null;
        const ownerInferred =
          task.owner_confidence !== null &&
          task.owner_confidence < 0.8 &&
          task.owner_confidence >= 0.4;
        return (
          <li
            key={task.id}
            style={{
              display: 'grid',
              gridTemplateColumns: '24px 1fr auto',
              gap: 14,
              padding: '12px 0',
              borderTop: i === 0 ? 'none' : '1px solid var(--line-soft)',
              alignItems: 'baseline',
              color: task.done ? 'var(--muted)' : 'var(--ink)',
              textDecoration: task.done ? 'line-through' : 'none',
            }}
          >
            <span
              className="mono"
              style={{
                color: 'var(--accent)',
                fontSize: 13,
              }}
              aria-hidden
            >
              {String(i + 1).padStart(2, '0')}
            </span>
            <span
              style={{
                fontFamily: 'var(--font-serif)',
                fontSize: 16,
              }}
            >
              {/* [M14 T-11] Category emoji prefix — ✅/💡/📝. */}
              <span style={{ fontFamily: 'var(--font-sans)' }}>{categoryEmoji(task.category)}</span>
              {task.text}
              {owner && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  — {owner}
                  {ownerInferred && (
                    <span
                      className="confidence-low"
                      title={t('actionItem.ownerInferred')}
                      aria-label={t('actionItem.ownerInferred')}
                      style={{ marginLeft: 4 }}
                    >
                      ?
                    </span>
                  )}
                </span>
              )}
              {task.due && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  {t('callDetail.taskDueShort', { date: task.due })}
                </span>
              )}
              {/* [M14 T-11] Evidence tooltip 💬 для items с quote из транскрипта. */}
              {task.evidence_quote && (
                <span style={{ marginLeft: 6 }}>
                  <EvidenceTooltip
                    quote={task.evidence_quote}
                    speaker={task.evidence_speaker}
                    startMs={task.evidence_start_ms}
                    onJumpToTranscript={onJumpToTranscript}
                  >
                    💬
                  </EvidenceTooltip>
                </span>
              )}
            </span>
            <span
              className="small-caps"
              style={{
                fontSize: 10,
                color: task.done ? 'var(--success)' : 'var(--muted)',
              }}
            >
              {task.done ? t('callDetail.taskStatusDone') : t('callDetail.taskStatusOpen')}
            </span>
          </li>
        );
      })}
    </ol>
  );
}
