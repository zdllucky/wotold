// Barrel — sub-components used by pages/CallDetailPage.tsx.
// Phase 5 (R7) extraction: one file per concern, 50-150 lines each.

export { AutoBoundBanner } from './AutoBoundBanner';
export { CallDetailSkeleton } from './CallDetailSkeleton';
export { ErrorScreen } from './ErrorScreen';
export { ErrorDiagnostics } from './ErrorDiagnostics';
export { HeaderActions } from './HeaderActions';
export { MdPanel } from './MdPanel';
export { MenuItem } from './MenuItem';
export { ParticipantsRow } from './ParticipantsRow';
export { ProcessingPanel } from './ProcessingPanel';
export { ReprocessBanner } from './ReprocessBanner';
export { TasksPanel } from './TasksPanel';
// [M14 T-11] V2 UI components.
export { CallTypeBadge } from './CallTypeBadge';
export { DecisionsBlock } from './DecisionsBlock';
export { OpenQuestionsBlock } from './OpenQuestionsBlock';
export { EvidenceTooltip } from './EvidenceTooltip';
export { PrivacyDisclaimer } from './PrivacyDisclaimer';
// [M14 T-15] Opt-in legacy v1 → v2 upgrade banner.
export { LegacyRecapBanner } from './LegacyRecapBanner';
// [Bug-fix #6] Auto-suggest recap regen after speaker bind.
export { RecapRegenSuggestionStrip } from './RecapRegenSuggestionStrip';
