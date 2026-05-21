// Markdown panel wrapper for recap/transcript tabs in CallDetailPage.
// Renders ReactMarkdown when content present, otherwise Empty placeholder.

import ReactMarkdown from 'react-markdown';
import { Empty } from '../../ui';

interface MdPanelProps {
  md: string | null;
  emptyHint: string;
}

export function MdPanel({ md, emptyHint }: MdPanelProps) {
  if (!md) return <Empty description={emptyHint} />;
  return (
    <div className="markdown">
      <ReactMarkdown>{md}</ReactMarkdown>
    </div>
  );
}
