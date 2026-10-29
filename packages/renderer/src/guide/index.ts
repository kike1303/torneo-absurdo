// @torneo/renderer/guide: the sprite guide layer (007 RF-A2). Pure, no Pixi and no DOM, and never
// imported by the runtime of the renderer (test/guide-imports.test.ts).
export { GUIDE_LIMITS } from './limits.ts';
// The sources the guide derives from, for the tools that document it (007 plan-b decision 24).
export { CONTACT_PX } from '../scene/types.ts';
export { TIMELINE_LIMITS } from '../timeline/types.ts';
export { GUIDE_CODES, RULESET_CODES } from './codes.ts';
export {
  parseRulesetTiming,
  type RulesetError,
  type RulesetResult,
  type RulesetTiming,
} from './ruleset.ts';
export {
  guideTable,
  windupCap,
  type GuideRange,
  type GuideRow,
  type WindupRange,
} from './table.ts';
export {
  validateGuide,
  type GuideError,
  type GuideErrorCode,
  type GuideResult,
  type RgbaImage,
} from './validate.ts';
