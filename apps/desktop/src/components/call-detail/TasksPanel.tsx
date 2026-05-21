// Action items list rendered inside Tabs.Panel value="tasks".
// Empty placeholder when no tasks; otherwise ordered list with owner +
// due metadata. Done items получают strike-through.

import type { ActionItem } from '../../api/calls';
import type { Contact } from '../../api/contacts';
import { Empty } from '../../ui';
import { useI18n } from '../../i18n';

interface TasksPanelProps {
  tasks: ActionItem[];
  contacts: Contact[];
}

export function TasksPanel({ tasks, contacts }: TasksPanelProps) {
  const { t } = useI18n();
  if (tasks.length === 0) {
    return <Empty description={t('callDetail.emptyTasks')} />;
  }
  const nameById = new Map(contacts.map((c) => [c.id, c.display_name]));
  return (
    <ol style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {tasks.map((task, i) => {
        const owner = task.owner_contact_id ? nameById.get(task.owner_contact_id) : null;
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
              {task.text}
              {owner && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  — {owner}
                </span>
              )}
              {task.due && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  {t('callDetail.taskDueShort', { date: task.due })}
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
