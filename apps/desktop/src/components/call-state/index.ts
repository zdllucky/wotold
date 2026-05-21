// [V6.1] Barrel export для async-state presentational components.
//
// Каждый компонент purely presentational — никаких side-effects, нет API
// калов, нет state. Caller подписывается на Tauri `call:progress` event
// stream (V6.2) и кормит фрешими `CallProgress` props.

export { CallStateTag } from './CallStateTag';
export type { CallStateTagProps } from './CallStateTag';
export { ProgressRail } from './ProgressRail';
export type { ProgressRailProps } from './ProgressRail';
export { PipelineStrip } from './PipelineStrip';
export type { PipelineStripProps } from './PipelineStrip';
export { CallErrorRow } from './CallErrorRow';
export type { CallErrorRowProps } from './CallErrorRow';
